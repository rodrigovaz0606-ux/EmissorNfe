// NFeConsultaProtocolo4: situação atual de uma NF-e pela chave de acesso.
//   100 autorizada · 101 cancelada · 110/301/302 denegada · 217 não consta na base da SEFAZ

import { parser, encontrar } from './xmlUtil.js';
import { enviarSoap } from './soap.js';
import { urlServico } from './config.js';
import { validarChave } from './chaveAcesso.js';

const NS = 'http://www.portalfiscal.inf.br/nfe';

export function montarConsSitNFe(chave, tpAmb) {
  if (!validarChave(chave)) throw new Error('Chave de acesso inválida.');
  return `<consSitNFe versao="4.00" xmlns="${NS}"><tpAmb>${tpAmb}</tpAmb><xServ>CONSULTAR</xServ><chNFe>${chave}</chNFe></consSitNFe>`;
}

export function lerRetConsSitNFe(respostaSoap) {
  const ret = encontrar(parser.parse(respostaSoap), 'retConsSitNFe');
  if (!ret) throw new Error(`Resposta sem retConsSitNFe:\n${respostaSoap.slice(0, 1500)}`);
  const prot = ret.protNFe?.infProt;
  const eventos = [ret.procEventoNFe ?? []].flat().map((p) => p.retEvento?.infEvento).filter(Boolean);
  return {
    cStat: ret.cStat,
    xMotivo: ret.xMotivo,
    chNFe: ret.chNFe,
    protocolo: prot ? { cStat: prot.cStat, xMotivo: prot.xMotivo, nProt: prot.nProt, dhRecbto: prot.dhRecbto } : null,
    eventos: eventos.map((e) => ({ tpEvento: e.tpEvento, xEvento: e.xEvento, cStat: e.cStat, nProt: e.nProt, dhRegEvento: e.dhRegEvento })),
  };
}

export async function consultarSituacao(config, certificado, chave) {
  const respostaSoap = await enviarSoap({
    url: urlServico(config.uf, 'NFeConsultaProtocolo4'),
    servico: 'NFeConsultaProtocolo4',
    metodo: 'nfeConsultaNF',
    xmlDados: montarConsSitNFe(chave, config.tpAmb),
    certificado,
  });
  return lerRetConsSitNFe(respostaSoap);
}
