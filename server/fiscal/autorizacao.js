// NFeAutorizacao4: envio da NF-e assinada para autorização.
//
// Envio síncrono (indSinc=1) com uma nota por lote. Se a SEFAZ processar de forma
// assíncrona (cStat 103, "Lote recebido"), consulta o recibo em NFeRetAutorizacao4.
//
// Códigos principais:
//   104 lote processado (o resultado da nota está em protNFe/infProt)
//   100 autorizado o uso da NF-e      150 autorizado fora de prazo
//   103 lote recebido (assíncrono)    105 lote em processamento
//   demais: rejeição

import crypto from 'node:crypto';
import { parser, encontrar, recortarElemento } from './xmlUtil.js';
import { enviarSoap } from './soap.js';
import { urlServico } from './config.js';

const NS = 'http://www.portalfiscal.inf.br/nfe';
export const AUTORIZADA = ['100', '150'];

export function gerarIdLote() {
  return String(Date.now()).slice(-12) + String(crypto.randomInt(0, 1000)).padStart(3, '0'); // 15 dígitos
}

// O XML da NF-e entra exatamente como foi assinado (sem declaração <?xml?>).
export function montarEnviNFe(xmlNFeAssinado, idLote = gerarIdLote()) {
  const nfe = xmlNFeAssinado.replace(/^\s*<\?xml[^>]*\?>\s*/, '');
  if (!/^<NFe[\s>]/.test(nfe) || !nfe.includes('<Signature')) throw new Error('Esperado o XML <NFe> assinado.');
  return `<enviNFe versao="4.00" xmlns="${NS}"><idLote>${idLote}</idLote><indSinc>1</indSinc>${nfe}</enviNFe>`;
}

function lerProtocolo(respostaSoap) {
  const prot = encontrar(parser.parse(respostaSoap), 'protNFe');
  if (!prot) return null;
  const inf = prot.infProt ?? {};
  return {
    cStat: inf.cStat,
    xMotivo: inf.xMotivo,
    chNFe: inf.chNFe,
    nProt: inf.nProt,
    dhRecbto: inf.dhRecbto,
    digVal: inf.digVal,
    autorizada: AUTORIZADA.includes(inf.cStat),
    xml: recortarElemento(respostaSoap, 'protNFe'), // texto original, para o nfeProc
  };
}

export function lerRetEnviNFe(respostaSoap) {
  const ret = encontrar(parser.parse(respostaSoap), 'retEnviNFe');
  if (!ret) throw new Error(`Resposta sem retEnviNFe:\n${respostaSoap.slice(0, 1500)}`);
  return {
    lote: { cStat: ret.cStat, xMotivo: ret.xMotivo, dhRecbto: ret.dhRecbto, verAplic: ret.verAplic },
    recibo: ret.infRec?.nRec ?? null,
    protocolo: lerProtocolo(respostaSoap),
  };
}

export function lerRetConsReciNFe(respostaSoap) {
  const ret = encontrar(parser.parse(respostaSoap), 'retConsReciNFe');
  if (!ret) throw new Error(`Resposta sem retConsReciNFe:\n${respostaSoap.slice(0, 1500)}`);
  return {
    lote: { cStat: ret.cStat, xMotivo: ret.xMotivo, dhRecbto: ret.dhRecbto, verAplic: ret.verAplic },
    recibo: ret.nRec,
    protocolo: lerProtocolo(respostaSoap),
  };
}

const esperar = (ms) => new Promise((r) => setTimeout(r, ms));

export async function consultarRecibo(config, certificado, nRec) {
  const xmlDados = `<consReciNFe versao="4.00" xmlns="${NS}"><tpAmb>${config.tpAmb}</tpAmb><nRec>${nRec}</nRec></consReciNFe>`;
  const resposta = await enviarSoap({
    url: urlServico(config.uf, 'NFeRetAutorizacao4'),
    servico: 'NFeRetAutorizacao4',
    metodo: 'nfeRetAutorizacaoLote',
    xmlDados,
    certificado,
  });
  return { ...lerRetConsReciNFe(resposta), respostaSoap: resposta };
}

/**
 * Envia a NF-e assinada e devolve o resultado.
 * @returns {Promise<{ lote, recibo, protocolo, enviado, respostaSoap }>}
 */
export async function autorizarNFe(config, certificado, xmlNFeAssinado, { tentativas = 5, intervaloMs = 3000 } = {}) {
  const enviado = montarEnviNFe(xmlNFeAssinado);
  const respostaSoap = await enviarSoap({
    url: urlServico(config.uf, 'NFeAutorizacao4'),
    servico: 'NFeAutorizacao4',
    metodo: 'nfeAutorizacaoLote',
    xmlDados: enviado,
    certificado,
    timeoutMs: 60000,
  });
  let resultado = { ...lerRetEnviNFe(respostaSoap), enviado, respostaSoap };

  // Processamento assíncrono: consulta o recibo até sair do "em processamento".
  if (resultado.lote.cStat === '103' && resultado.recibo) {
    for (let i = 0; i < tentativas; i++) {
      await esperar(intervaloMs);
      const consulta = await consultarRecibo(config, certificado, resultado.recibo);
      resultado = { ...resultado, ...consulta, enviado };
      if (consulta.lote.cStat !== '105') break;
    }
  }
  return resultado;
}

// nfeProc: NF-e assinada + protocolo de autorização. É o XML que deve ser guardado
// e entregue ao destinatário.
export function montarNfeProc(xmlNFeAssinado, xmlProtNFe) {
  const nfe = xmlNFeAssinado.replace(/^\s*<\?xml[^>]*\?>\s*/, '');
  return `<?xml version="1.0" encoding="UTF-8"?><nfeProc versao="4.00" xmlns="${NS}">${nfe}${xmlProtNFe}</nfeProc>`;
}
