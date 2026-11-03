import express from 'express';
import { consultarStatusServico } from './fiscal/statusServico.js';
import { prepararNFe, emitirNFe, cancelarNota, cartaCorrecao, inutilizarFaixa, consultarNota, ErroNegocio } from './emissao.js';
import { gerarDanfe } from './danfe/danfe.js';

// Erros de rede/SEFAZ → 502; regras de negócio e dados inválidos → 400.
function tratarErro(res, e) {
  const rede = ['ECONNRESET', 'ETIMEDOUT', 'ENOTFOUND', 'ECONNREFUSED', 'EPROTO'].includes(e.code) || /HTTP \d{3}|Tempo esgotado|Resposta sem/.test(e.message);
  if (!rede && !(e instanceof ErroNegocio)) console.error(e);
  res.status(rede ? 502 : 400).json({ erro: e.message });
}
const rota = (fn) => async (req, res) => {
  try {
    await fn(req, res);
  } catch (e) {
    tratarErro(res, e);
  }
};

function enviarXml(res, xml, nome) {
  if (!xml) return res.status(404).json({ erro: 'XML não disponível' });
  res.set('Content-Type', 'application/xml; charset=utf-8');
  res.set('Content-Disposition', `attachment; filename="${nome}"`);
  res.send(xml);
}

async function enviarPdf(res, xml, nome, opcoes) {
  const pdf = await gerarDanfe(xml, opcoes);
  res.set('Content-Type', 'application/pdf');
  res.set('Content-Disposition', `inline; filename="${nome}"`);
  res.send(pdf);
}

export function rotas(ctx) {
  const { config, certificado, repo } = ctx;
  const r = express.Router();
  const id = (req) => Number(req.params.id);
  const notaOu404 = (req, res) => {
    const nota = repo.nota(id(req));
    if (!nota) res.status(404).json({ erro: 'Nota não encontrada' });
    return nota;
  };

  r.get('/emissor', (req, res) => {
    const { info } = certificado;
    res.json({
      uf: config.uf, cUF: config.cUF, ambiente: 'homologacao', simulada: Boolean(ctx.sefaz),
      certificado: info, podeInutilizar: info.documento?.length === 14,
    });
  });

  r.get('/sefaz/status', rota(async (req, res) => {
    if (ctx.sefaz) return res.json({ cStat: '107', xMotivo: 'Servico em operacao (SIMULADO)', emOperacao: true, tMed: '0' });
    res.json(await consultarStatusServico(config, certificado));
  }));

  r.get('/numeracao/:serie', (req, res) => res.json({ serie: Number(req.params.serie), proximo: repo.proximoNumero(Number(req.params.serie)) }));

  // ----- emissão -----
  r.post('/nfe/validar', rota(async (req, res) => {
    const { assinado, ...resultado } = await prepararNFe(req.body, ctx);
    res.json(resultado);
  }));
  r.post('/nfe/danfe-previa', rota(async (req, res) => {
    const { assinado, chave } = await prepararNFe(req.body, ctx);
    await enviarPdf(res, assinado, `previa-${chave}.pdf`);
  }));
  r.post('/nfe/emitir', rota(async (req, res) => res.json(await emitirNFe(req.body, ctx))));

  // ----- notas -----
  r.get('/notas', (req, res) => res.json(repo.listarNotas({ pagina: Math.max(1, Number(req.query.pagina) || 1), limite: 50 })));
  r.get('/notas/:id', (req, res) => {
    const nota = notaOu404(req, res);
    if (!nota) return;
    const { xml_nfe, xml_proc, xml_resposta, dados_json, ...resto } = nota;
    res.json({ ...resto, dados: JSON.parse(dados_json), tem_proc: Boolean(xml_proc), eventos: repo.eventosDaNota(nota.id) });
  });
  r.get('/notas/:id/xml', (req, res) => {
    const nota = notaOu404(req, res);
    if (!nota) return;
    const tipo = req.query.tipo ?? (nota.xml_proc ? 'proc' : 'nfe');
    const xml = { proc: nota.xml_proc, nfe: nota.xml_nfe, resposta: nota.xml_resposta }[tipo];
    enviarXml(res, xml, `${nota.chave}-${tipo === 'proc' ? 'procNFe' : tipo}.xml`);
  });
  r.get('/notas/:id/danfe', rota(async (req, res) => {
    const nota = notaOu404(req, res);
    if (!nota) return;
    await enviarPdf(res, nota.xml_proc ?? nota.xml_nfe, `DANFE-${nota.chave}.pdf`, { cancelada: nota.status === 'cancelada' });
  }));
  r.post('/notas/:id/cancelar', rota(async (req, res) => res.json(await cancelarNota(id(req), req.body?.xJust, ctx))));
  r.post('/notas/:id/cce', rota(async (req, res) => res.json(await cartaCorrecao(id(req), req.body?.xCorrecao, ctx))));
  r.get('/notas/:id/consultar', rota(async (req, res) => res.json(await consultarNota(id(req), ctx))));
  r.get('/eventos/:id/xml', (req, res) => {
    const ev = repo.evento(id(req));
    if (!ev) return res.status(404).json({ erro: 'Evento não encontrado' });
    enviarXml(res, ev.xml_proc ?? ev.xml_resposta, `evento-${ev.tipo}-${ev.nSeqEvento}.xml`);
  });

  // ----- inutilização -----
  r.get('/inutilizacoes', (req, res) => res.json(repo.listarInutilizacoes()));
  r.post('/inutilizacoes', rota(async (req, res) => {
    res.json(await inutilizarFaixa({ ...req.body, cnpj: certificado.info.documento }, ctx));
  }));

  return r;
}
