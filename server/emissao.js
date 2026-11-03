// Serviço de emissão usado pela API: gera, assina, valida e (opcionalmente) envia.
import fs from 'node:fs';
import path from 'node:path';
import { gerarXmlNFe } from './fiscal/nfe/gerarXml.js';
import { assinarNFe, verificarAssinatura } from './fiscal/nfe/assinar.js';
import { validarXsd } from './fiscal/nfe/validarXsd.js';
import { autorizarNFe, montarNfeProc } from './fiscal/autorizacao.js';

export const PASTA_SAIDA = path.resolve('saida');

function salvar(nome, conteudo) {
  fs.mkdirSync(PASTA_SAIDA, { recursive: true });
  fs.writeFileSync(path.join(PASTA_SAIDA, nome), conteudo, 'utf8');
  return nome;
}

export async function prepararNFe(dados, config, certificado) {
  const { xml, chave, vNF } = gerarXmlNFe(dados, config);
  const assinado = assinarNFe(xml, certificado);
  const assinatura = verificarAssinatura(assinado);
  const xsd = await validarXsd(assinado);
  return { chave, vNF, assinado, assinaturaValida: assinatura.valida, xsd };
}

export async function emitirNFe(dados, config, certificado) {
  const p = await prepararNFe(dados, config, certificado);
  const base = { chave: p.chave, vNF: p.vNF, assinaturaValida: p.assinaturaValida, xsd: p.xsd };
  if (!p.assinaturaValida || !p.xsd.valido) return { ...base, enviada: false };

  const r = await autorizarNFe(config, certificado, p.assinado);
  const arquivos = {
    nfe: salvar(`${p.chave}-nfe.xml`, '<?xml version="1.0" encoding="UTF-8"?>' + p.assinado),
    resposta: salvar(`${p.chave}-resposta.xml`, r.respostaSoap),
  };
  if (r.protocolo?.autorizada) arquivos.procNFe = salvar(`${p.chave}-procNFe.xml`, montarNfeProc(p.assinado, r.protocolo.xml));

  const { xml: _xml, ...protocolo } = r.protocolo ?? {};
  return { ...base, enviada: true, lote: r.lote, recibo: r.recibo, protocolo: r.protocolo ? protocolo : null, arquivos };
}
