// NFeInutilizacao4: inutiliza uma faixa de numeração que não será usada.
// No leiaute 4.00 o emitente precisa ter CNPJ. Retorno 102 = inutilização homologada.

import { assinarXml } from './nfe/assinar.js';
import { escaparXml, texto, soDigitos } from './nfe/formatos.js';
import { parser, encontrar, recortarElemento } from './xmlUtil.js';
import { enviarSoap } from './soap.js';
import { urlServico } from './config.js';

const NS = 'http://www.portalfiscal.inf.br/nfe';

/** @param {{ cnpj, serie, nNFIni, nNFFin, xJust, ano? }} d */
export function montarInutNFe(d, config) {
  if (config?.tpAmb !== 2) throw new Error('Somente homologação (tpAmb=2) é permitida nesta versão.');
  const cnpj = soDigitos(d.cnpj);
  if (cnpj?.length !== 14) throw new Error('A inutilização exige emitente com CNPJ (o leiaute 4.00 não aceita CPF).');
  const serie = Number(d.serie);
  const ini = Number(d.nNFIni);
  const fim = Number(d.nNFFin);
  if (!(serie >= 0 && serie <= 999)) throw new Error('Série inválida.');
  if (!(ini >= 1 && fim >= ini && fim <= 999_999_999)) throw new Error('Faixa de numeração inválida.');
  const xJust = texto(d.xJust) ?? '';
  if (xJust.length < 15 || xJust.length > 255) throw new Error('A justificativa deve ter de 15 a 255 caracteres.');
  const ano = String(d.ano ?? new Date().getFullYear()).slice(-2);

  const id = `ID${config.cUF}${ano}${cnpj}55${String(serie).padStart(3, '0')}${String(ini).padStart(9, '0')}${String(fim).padStart(9, '0')}`;
  return (
    `<inutNFe versao="4.00" xmlns="${NS}"><infInut Id="${id}">` +
    `<tpAmb>${config.tpAmb}</tpAmb><xServ>INUTILIZAR</xServ><cUF>${config.cUF}</cUF><ano>${ano}</ano><CNPJ>${cnpj}</CNPJ>` +
    `<mod>55</mod><serie>${serie}</serie><nNFIni>${ini}</nNFIni><nNFFin>${fim}</nNFFin><xJust>${escaparXml(xJust)}</xJust>` +
    '</infInut></inutNFe>'
  );
}

export function prepararInutilizacao(d, config, certificado) {
  return assinarXml(montarInutNFe(d, config), certificado, 'infInut');
}

export function lerRetInutNFe(respostaSoap) {
  const ret = encontrar(parser.parse(respostaSoap), 'retInutNFe');
  const inf = ret?.infInut;
  if (!inf) throw new Error(`Resposta sem retInutNFe:\n${respostaSoap.slice(0, 1500)}`);
  return {
    cStat: inf.cStat, xMotivo: inf.xMotivo, nProt: inf.nProt, dhRecbto: inf.dhRecbto,
    homologada: inf.cStat === '102',
    xmlRet: recortarElemento(respostaSoap, 'retInutNFe'),
  };
}

export function montarProcInut(inutAssinado, xmlRet) {
  return `<?xml version="1.0" encoding="UTF-8"?><ProcInutNFe versao="4.00" xmlns="${NS}">${inutAssinado}${xmlRet}</ProcInutNFe>`;
}

export async function enviarInutilizacao(config, certificado, inutAssinado) {
  const respostaSoap = await enviarSoap({
    url: urlServico(config.uf, 'NFeInutilizacao4'),
    servico: 'NFeInutilizacao4',
    metodo: 'nfeInutilizacaoNF',
    xmlDados: inutAssinado,
    certificado,
  });
  return { ...lerRetInutNFe(respostaSoap), respostaSoap };
}
