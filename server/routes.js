import express from 'express';
import fs from 'node:fs';
import path from 'node:path';
import { consultarStatusServico } from './fiscal/statusServico.js';
import { prepararNFe, emitirNFe, PASTA_SAIDA } from './emissao.js';

// Erros de dados (gerados por gerarXml com mensagens em português) viram 400;
// falhas de rede/SEFAZ viram 502.
function tratarErro(res, e) {
  const rede = ['ECONNRESET', 'ETIMEDOUT', 'ENOTFOUND', 'ECONNREFUSED', 'EPROTO'].includes(e.code) || /HTTP \d{3}|Tempo esgotado/.test(e.message);
  res.status(rede ? 502 : 400).json({ erro: e.message });
}

export function rotas({ config, certificado }) {
  const r = express.Router();

  r.get('/emissor', (req, res) => {
    const { info } = certificado;
    res.json({
      uf: config.uf,
      cUF: config.cUF,
      ambiente: 'homologacao',
      certificado: { ...info, validoAte: info.validoAte, validoDe: info.validoDe },
    });
  });

  r.get('/sefaz/status', async (req, res) => {
    try {
      res.json(await consultarStatusServico(config, certificado));
    } catch (e) {
      tratarErro(res, e);
    }
  });

  r.post('/nfe/validar', async (req, res) => {
    try {
      const { assinado, ...resultado } = await prepararNFe(req.body, config, certificado);
      res.json(resultado);
    } catch (e) {
      tratarErro(res, e);
    }
  });

  r.post('/nfe/emitir', async (req, res) => {
    try {
      res.json(await emitirNFe(req.body, config, certificado));
    } catch (e) {
      tratarErro(res, e);
    }
  });

  // Download dos XMLs gerados (só nomes de arquivo simples dentro de saida/).
  r.get('/arquivos/:nome', (req, res) => {
    const nome = path.basename(req.params.nome);
    if (!/^\d{44}-[A-Za-z]+\.xml$/.test(nome)) return res.status(400).json({ erro: 'Nome inválido' });
    const arquivo = path.join(PASTA_SAIDA, nome);
    if (!fs.existsSync(arquivo)) return res.status(404).json({ erro: 'Arquivo não encontrado' });
    res.download(arquivo);
  });

  return r;
}
