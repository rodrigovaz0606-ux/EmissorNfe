// Eventos da NF-e (NFeRecepcaoEvento4): cancelamento (110111) e Carta de Correção (110110).
//
// Códigos de retorno do evento (retEvento/infEvento/cStat):
//   135 evento registrado e vinculado à NF-e
//   136 evento registrado, mas não vinculado à NF-e
//   155 cancelamento homologado fora de prazo
//   demais: rejeição

import { assinarXml } from './nfe/assinar.js';
import { escaparXml, texto, soDigitos, dataHora } from './nfe/formatos.js';
import { parser, encontrar, recortarElemento } from './xmlUtil.js';
import { enviarSoap } from './soap.js';
import { urlServico } from './config.js';
import { gerarIdLote } from './autorizacao.js';
import { validarChave } from './chaveAcesso.js';

const NS = 'http://www.portalfiscal.inf.br/nfe';
export const EVENTO_REGISTRADO = ['135', '136', '155'];

export const TIPOS = {
  cancelamento: { tpEvento: '110111', descEvento: 'Cancelamento', schema: 'cancelamento' },
  cce: { tpEvento: '110110', descEvento: 'Carta de Correcao', schema: 'cce' },
};

export const COND_USO_CCE =
  'A Carta de Correcao e disciplinada pelo paragrafo 1o-A do art. 7o do Convenio S/N, de 15 de dezembro de 1970 ' +
  'e pode ser utilizada para regularizacao de erro ocorrido na emissao de documento fiscal, desde que o erro nao esteja ' +
  'relacionado com: I - as variaveis que determinam o valor do imposto tais como: base de calculo, aliquota, diferenca ' +
  'de preco, quantidade, valor da operacao ou da prestacao; II - a correcao de dados cadastrais que implique mudanca do ' +
  'remetente ou do destinatario; III - a data de emissao ou de saida.';

function detalhe(tipo, dados) {
  if (tipo === 'cancelamento') {
    const nProt = soDigitos(dados.nProt);
    const xJust = texto(dados.xJust) ?? '';
    if (!/^\d{15}$/.test(nProt ?? '')) throw new Error('Cancelamento exige o protocolo de autorização (15 dígitos).');
    if (xJust.length < 15 || xJust.length > 255) throw new Error('A justificativa do cancelamento deve ter de 15 a 255 caracteres.');
    return `<descEvento>Cancelamento</descEvento><nProt>${nProt}</nProt><xJust>${escaparXml(xJust)}</xJust>`;
  }
  const xCorrecao = texto(dados.xCorrecao) ?? '';
  if (xCorrecao.length < 15 || xCorrecao.length > 1000) throw new Error('O texto da correção deve ter de 15 a 1000 caracteres.');
  return `<descEvento>Carta de Correcao</descEvento><xCorrecao>${escaparXml(xCorrecao)}</xCorrecao><xCondUso>${COND_USO_CCE}</xCondUso>`;
}

/**
 * Monta o <evento> (ainda sem assinatura).
 * @param {'cancelamento'|'cce'} tipo
 * @param {{ chave, documento, nSeqEvento?, nProt?, xJust?, xCorrecao?, dhEvento? }} dados
 */
export function montarEvento(tipo, dados, config) {
  const t = TIPOS[tipo];
  if (!t) throw new Error(`Tipo de evento desconhecido: ${tipo}`);
  if (config?.tpAmb !== 2) throw new Error('Somente homologação (tpAmb=2) é permitida nesta versão.');
  if (!validarChave(dados.chave)) throw new Error('Chave de acesso inválida.');

  const nSeq = Number(dados.nSeqEvento ?? 1);
  if (!(nSeq >= 1 && nSeq <= 20)) throw new Error('nSeqEvento deve estar entre 1 e 20.');
  const doc = soDigitos(dados.documento);
  const tagDoc = doc.length === 14 ? `<CNPJ>${doc}</CNPJ>` : doc.length === 11 ? `<CPF>${doc}</CPF>` : null;
  if (!tagDoc) throw new Error('Documento do autor do evento deve ser CPF ou CNPJ.');

  const id = `ID${t.tpEvento}${dados.chave}${String(nSeq).padStart(2, '0')}`;
  return (
    `<evento versao="1.00" xmlns="${NS}"><infEvento Id="${id}">` +
    `<cOrgao>${config.cUF}</cOrgao><tpAmb>${config.tpAmb}</tpAmb>${tagDoc}<chNFe>${dados.chave}</chNFe>` +
    `<dhEvento>${dados.dhEvento ?? dataHora()}</dhEvento><tpEvento>${t.tpEvento}</tpEvento><nSeqEvento>${nSeq}</nSeqEvento>` +
    `<verEvento>1.00</verEvento><detEvento versao="1.00">${detalhe(tipo, dados)}</detEvento>` +
    '</infEvento></evento>'
  );
}

export function montarEnvEvento(eventoAssinado, idLote = gerarIdLote()) {
  // O <evento> vai dentro do lote sem repetir o xmlns (herda do envEvento).
  const evento = eventoAssinado.replace(`<evento versao="1.00" xmlns="${NS}">`, '<evento versao="1.00">');
  return `<envEvento versao="1.00" xmlns="${NS}"><idLote>${idLote}</idLote>${evento}</envEvento>`;
}

export function lerRetEnvEvento(respostaSoap) {
  const ret = encontrar(parser.parse(respostaSoap), 'retEnvEvento');
  if (!ret) throw new Error(`Resposta sem retEnvEvento:\n${respostaSoap.slice(0, 1500)}`);
  const inf = (Array.isArray(ret.retEvento) ? ret.retEvento[0] : ret.retEvento)?.infEvento ?? {};
  return {
    lote: { cStat: ret.cStat, xMotivo: ret.xMotivo },
    evento: inf.cStat
      ? {
        cStat: inf.cStat, xMotivo: inf.xMotivo, chNFe: inf.chNFe, tpEvento: inf.tpEvento, nSeqEvento: inf.nSeqEvento,
        dhRegEvento: inf.dhRegEvento, nProt: inf.nProt, registrado: EVENTO_REGISTRADO.includes(inf.cStat),
      }
      : null,
    xmlRetEvento: recortarElemento(respostaSoap, 'retEvento'),
  };
}

export function montarProcEvento(eventoAssinado, xmlRetEvento) {
  const evento = eventoAssinado.replace(` xmlns="${NS}"`, '');
  return `<?xml version="1.0" encoding="UTF-8"?><procEventoNFe versao="1.00" xmlns="${NS}">${evento}${xmlRetEvento}</procEventoNFe>`;
}

/** Monta, assina e devolve o lote do evento (não envia). */
export function prepararEvento(tipo, dados, config, certificado) {
  const evento = assinarXml(montarEvento(tipo, dados, config), certificado, 'infEvento');
  return { evento, envEvento: montarEnvEvento(evento), schema: TIPOS[tipo].schema };
}

export async function enviarEvento(config, certificado, envEvento) {
  const respostaSoap = await enviarSoap({
    url: urlServico(config.uf, 'NFeRecepcaoEvento4'),
    servico: 'NFeRecepcaoEvento4',
    metodo: 'nfeRecepcaoEvento',
    xmlDados: envEvento,
    certificado,
  });
  return { ...lerRetEnvEvento(respostaSoap), respostaSoap };
}
