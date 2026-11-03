// SEFAZ SIMULADA — só para testes de interface (NFE_SEFAZ_SIMULADA=1).
// Nada é enviado à SEFAZ; as respostas imitam o formato real com o texto "SIMULADO".
import crypto from 'node:crypto';
import { dataHora } from './fiscal/nfe/formatos.js';

const protocolo = () => `152${String(Date.now()).slice(-12)}`;
const envelope = (corpo) => `<soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope"><soap:Body>${corpo}</soap:Body></soap:Envelope>`;

export const sefazSimulada = {
  async autorizar(config, certificado, xmlAssinado) {
    const chave = xmlAssinado.match(/Id="NFe(\d{44})"/)[1];
    const nProt = protocolo();
    const dh = dataHora();
    const digVal = crypto.createHash('sha1').update(xmlAssinado).digest('base64');
    const xml = `<protNFe versao="4.00"><infProt><tpAmb>2</tpAmb><verAplic>SIMULADOR</verAplic><chNFe>${chave}</chNFe><dhRecbto>${dh}</dhRecbto><nProt>${nProt}</nProt><digVal>${digVal}</digVal><cStat>100</cStat><xMotivo>Autorizado o uso da NF-e (SIMULADO)</xMotivo></infProt></protNFe>`;
    return {
      lote: { cStat: '104', xMotivo: 'Lote processado (SIMULADO)', dhRecbto: dh },
      recibo: null,
      protocolo: { cStat: '100', xMotivo: 'Autorizado o uso da NF-e (SIMULADO)', chNFe: chave, nProt, dhRecbto: dh, digVal, autorizada: true, xml },
      respostaSoap: envelope(`<retEnviNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><cStat>104</cStat><xMotivo>Lote processado (SIMULADO)</xMotivo>${xml}</retEnviNFe>`),
    };
  },
  async evento(config, certificado, envEvento) {
    const chNFe = envEvento.match(/<chNFe>(\d{44})<\/chNFe>/)[1];
    const tpEvento = envEvento.match(/<tpEvento>(\d+)<\/tpEvento>/)[1];
    const nSeqEvento = envEvento.match(/<nSeqEvento>(\d+)<\/nSeqEvento>/)[1];
    const nProt = protocolo();
    const dh = dataHora();
    const ret = `<retEvento versao="1.00"><infEvento><tpAmb>2</tpAmb><verAplic>SIMULADOR</verAplic><cOrgao>52</cOrgao><cStat>135</cStat><xMotivo>Evento registrado e vinculado a NF-e (SIMULADO)</xMotivo><chNFe>${chNFe}</chNFe><tpEvento>${tpEvento}</tpEvento><nSeqEvento>${nSeqEvento}</nSeqEvento><dhRegEvento>${dh}</dhRegEvento><nProt>${nProt}</nProt></infEvento></retEvento>`;
    return {
      lote: { cStat: '128', xMotivo: 'Lote de evento processado (SIMULADO)' },
      evento: { cStat: '135', xMotivo: 'Evento registrado e vinculado a NF-e (SIMULADO)', chNFe, tpEvento, nSeqEvento, dhRegEvento: dh, nProt, registrado: true },
      xmlRetEvento: ret,
      respostaSoap: envelope(ret),
    };
  },
  async inutilizar() {
    const nProt = protocolo();
    const ret = `<retInutNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><infInut><tpAmb>2</tpAmb><cStat>102</cStat><xMotivo>Inutilizacao homologada (SIMULADO)</xMotivo><nProt>${nProt}</nProt></infInut></retInutNFe>`;
    return { cStat: '102', xMotivo: 'Inutilizacao homologada (SIMULADO)', nProt, homologada: true, xmlRet: ret, respostaSoap: envelope(ret) };
  },
  async consultar(config, certificado, chave) {
    return { cStat: '100', xMotivo: 'Autorizado o uso da NF-e (SIMULADO)', chNFe: chave, protocolo: null, eventos: [] };
  },
};
