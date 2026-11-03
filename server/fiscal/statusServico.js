// NFeStatusServico4: consulta se o webservice da SEFAZ está em operação.

import { parser, encontrar } from './xmlUtil.js';
import { enviarSoap } from './soap.js';
import { urlServico } from './config.js';

const SERVICO = 'NFeStatusServico4';
const METODO = 'nfeStatusServicoNF';

export function montarConsStatServ({ tpAmb, cUF }) {
  return (
    '<consStatServ versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe">' +
    `<tpAmb>${tpAmb}</tpAmb><cUF>${cUF}</cUF><xServ>STATUS</xServ>` +
    '</consStatServ>'
  );
}

export function lerRetConsStatServ(respostaSoap) {
  const doc = parser.parse(respostaSoap);
  const ret = encontrar(doc, 'retConsStatServ');
  if (!ret) throw new Error(`Resposta sem retConsStatServ:\n${respostaSoap.slice(0, 1000)}`);
  return {
    tpAmb: ret.tpAmb,
    verAplic: ret.verAplic,
    cStat: ret.cStat,
    xMotivo: ret.xMotivo,
    cUF: ret.cUF,
    dhRecbto: ret.dhRecbto,
    tMed: ret.tMed,
    emOperacao: ret.cStat === '107',
  };
}

export async function consultarStatusServico(config, certificado) {
  const resposta = await enviarSoap({
    url: urlServico(config.uf, SERVICO),
    servico: SERVICO,
    metodo: METODO,
    xmlDados: montarConsStatServ(config),
    certificado,
  });
  return lerRetConsStatServ(resposta);
}
