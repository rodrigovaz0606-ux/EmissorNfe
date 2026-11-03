// Regras de negócio usadas pela API: emitir, cancelar, corrigir, inutilizar e consultar.
// Toda comunicação com a SEFAZ passa por aqui e o resultado é gravado no banco.
import { gerarXmlNFe } from './fiscal/nfe/gerarXml.js';
import { assinarNFe, verificarAssinatura } from './fiscal/nfe/assinar.js';
import { validarXsd } from './fiscal/nfe/validarXsd.js';
import { autorizarNFe, montarNfeProc } from './fiscal/autorizacao.js';
import { prepararEvento, enviarEvento, montarProcEvento } from './fiscal/eventos.js';
import { prepararInutilizacao, enviarInutilizacao, montarProcInut } from './fiscal/inutilizacao.js';
import { consultarSituacao } from './fiscal/consulta.js';

export class ErroNegocio extends Error {}
const DENEGADA = ['110', '301', '302', '303'];

/**
 * Contexto: { config, certificado, repo, sefaz? }
 * `sefaz` permite trocar as chamadas reais por simuladas nos testes.
 */
function sefazDe(ctx) {
  return {
    autorizar: autorizarNFe,
    evento: enviarEvento,
    inutilizar: enviarInutilizacao,
    consultar: consultarSituacao,
    ...ctx.sefaz,
  };
}

export async function prepararNFe(dados, { config, certificado }) {
  const { xml, chave, vNF } = gerarXmlNFe(dados, config);
  const assinado = assinarNFe(xml, certificado);
  const assinatura = verificarAssinatura(assinado);
  const xsd = await validarXsd(assinado);
  return { chave, vNF, assinado, assinaturaValida: assinatura.valida, xsd };
}

export async function emitirNFe(dados, ctx) {
  const { config, certificado, repo } = ctx;
  const serie = Number(dados.ide?.serie);
  const nNF = Number(dados.ide?.nNF);
  if (repo.numeroJaUsado(serie, nNF)) {
    throw new ErroNegocio(`O número ${nNF} da série ${serie} já foi usado. Próximo disponível: ${repo.proximoNumero(serie)}.`);
  }

  const p = await prepararNFe(dados, ctx);
  const base = { chave: p.chave, vNF: p.vNF, assinaturaValida: p.assinaturaValida, xsd: p.xsd };
  if (!p.assinaturaValida || !p.xsd.valido) return { ...base, enviada: false };

  // Grava antes de enviar: se a conexão cair, a nota fica "pendente" e pode ser consultada.
  const id = repo.salvarNota({
    chave: p.chave, serie, nNF, status: 'pendente', vNF: p.vNF,
    emit_doc: dados.emit.CNPJ ?? dados.emit.CPF, dest_doc: dados.dest.CNPJ ?? dados.dest.CPF, dest_nome: dados.dest.xNome,
    dados_json: JSON.stringify(dados), xml_nfe: '<?xml version="1.0" encoding="UTF-8"?>' + p.assinado,
  });

  const r = await sefazDe(ctx).autorizar(config, certificado, p.assinado);
  const prot = r.protocolo;
  const status = prot?.autorizada ? 'autorizada' : DENEGADA.includes(prot?.cStat) ? 'denegada' : r.lote.cStat === '105' ? 'pendente' : 'rejeitada';
  repo.atualizarResultado(id, {
    status,
    cStat: prot?.cStat ?? r.lote.cStat,
    xMotivo: prot?.xMotivo ?? r.lote.xMotivo,
    nProt: prot?.nProt ?? null,
    dhRecbto: prot?.dhRecbto ?? r.lote.dhRecbto ?? null,
    xml_proc: prot?.autorizada ? montarNfeProc(p.assinado, prot.xml) : null,
    xml_resposta: r.respostaSoap,
  });

  const { xml: _xml, ...protocolo } = prot ?? {};
  return { ...base, id, status, enviada: true, lote: r.lote, recibo: r.recibo, protocolo: prot ? protocolo : null };
}

function notaOuErro(repo, id) {
  const nota = repo.nota(id);
  if (!nota) throw new ErroNegocio('Nota não encontrada.');
  return nota;
}

async function registrarEvento(tipo, nota, campos, ctx) {
  const { config, certificado, repo } = ctx;
  const nSeqEvento = tipo === 'cce' ? repo.proximoSeqCce(nota.id) : 1;
  if (nSeqEvento > 20) throw new ErroNegocio('Limite de 20 cartas de correção por nota atingido.');

  const { evento, envEvento, schema } = prepararEvento(tipo, { chave: nota.chave, documento: nota.emit_doc, nSeqEvento, ...campos }, config, certificado);
  const xsd = await validarXsd(envEvento, schema);
  if (!xsd.valido) throw new ErroNegocio(`Evento inválido no XSD: ${xsd.erros.join('; ')}`);

  const r = await sefazDe(ctx).evento(config, certificado, envEvento);
  const ev = r.evento;
  const registrado = Boolean(ev?.registrado);
  const eventoId = repo.salvarEvento({
    nota_id: nota.id, tipo, nSeqEvento, texto: campos.xJust ?? campos.xCorrecao,
    cStat: ev?.cStat ?? r.lote.cStat, xMotivo: ev?.xMotivo ?? r.lote.xMotivo, nProt: ev?.nProt ?? null, dhRegEvento: ev?.dhRegEvento ?? null,
    registrado, xml_proc: registrado && r.xmlRetEvento ? montarProcEvento(evento, r.xmlRetEvento) : null, xml_resposta: r.respostaSoap,
  });
  return { eventoId, registrado, cStat: ev?.cStat ?? r.lote.cStat, xMotivo: ev?.xMotivo ?? r.lote.xMotivo, nProt: ev?.nProt ?? null, nSeqEvento };
}

export async function cancelarNota(id, xJust, ctx) {
  const nota = notaOuErro(ctx.repo, id);
  if (nota.status !== 'autorizada') throw new ErroNegocio('Só é possível cancelar nota autorizada.');
  const r = await registrarEvento('cancelamento', nota, { nProt: nota.nProt, xJust }, ctx);
  if (r.registrado) ctx.repo.marcarCancelada(nota.id);
  return r;
}

export async function cartaCorrecao(id, xCorrecao, ctx) {
  const nota = notaOuErro(ctx.repo, id);
  if (nota.status !== 'autorizada') throw new ErroNegocio('Só é possível emitir carta de correção para nota autorizada.');
  return registrarEvento('cce', nota, { xCorrecao }, ctx);
}

export async function inutilizarFaixa(dados, ctx) {
  const { config, certificado, repo } = ctx;
  const serie = Number(dados.serie);
  for (let n = Number(dados.nNFIni); n <= Number(dados.nNFFin); n++) {
    if (repo.numeroJaUsado(serie, n)) throw new ErroNegocio(`O número ${n} já foi usado por uma nota e não pode ser inutilizado.`);
    if (n - Number(dados.nNFIni) > 10_000) break;
  }
  const inut = prepararInutilizacao({ ...dados, cnpj: dados.cnpj }, config, certificado);
  const xsd = await validarXsd(inut, 'inutilizacao');
  if (!xsd.valido) throw new ErroNegocio(`Inutilização inválida no XSD: ${xsd.erros.join('; ')}`);

  const r = await sefazDe(ctx).inutilizar(config, certificado, inut);
  repo.salvarInutilizacao({
    serie, nNFIni: Number(dados.nNFIni), nNFFin: Number(dados.nNFFin), xJust: dados.xJust,
    cStat: r.cStat, xMotivo: r.xMotivo, nProt: r.nProt, homologada: r.homologada,
    xml_proc: r.homologada ? montarProcInut(inut, r.xmlRet) : null, xml_resposta: r.respostaSoap,
  });
  return { cStat: r.cStat, xMotivo: r.xMotivo, nProt: r.nProt, homologada: r.homologada };
}

export async function consultarNota(id, ctx) {
  const nota = notaOuErro(ctx.repo, id);
  return sefazDe(ctx).consultar(ctx.config, ctx.certificado, nota.chave);
}
