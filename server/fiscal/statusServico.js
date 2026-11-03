// NFeStatusServico4: consulta se o webservice da SEFAZ está em operação.

import { XMLParser } from 'fast-xml-parser';
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

const parser = new XMLParser({ removeNSPrefix: true, ignoreAttributes: false, parseTagValue: false });

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

// Busca uma tag em qualquer profundidade (o envelope SOAP varia entre UFs).
function encontrar(no, nome) {
  if (!no || typeof no !== 'object') return null;
  if (no[nome]) return no[nome];
  for (const valor of Object.values(no)) {
    const achado = encontrar(valor, nome);
    if (achado) return achado;
  }
  return null;
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
