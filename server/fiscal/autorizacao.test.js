import { test } from 'node:test';
import assert from 'node:assert/strict';
import { montarEnviNFe, lerRetEnviNFe, lerRetConsReciNFe, montarNfeProc, gerarIdLote } from './autorizacao.js';

const CHAVE = '52261112345678000195550010000000011482017365'; // fictícia
const envelope = (corpo) =>
  '<?xml version="1.0" encoding="UTF-8"?><env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope"><env:Header/><env:Body>' +
  `<nfeResultMsg xmlns="http://www.portalfiscal.inf.br/nfe/wsdl/NFeAutorizacao4">${corpo}</nfeResultMsg></env:Body></env:Envelope>`;

// Formato real devolvido pela SEFAZ-GO em homologação (chave trocada).
const REJEITADA = envelope(
  '<retEnviNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe" xmlns:ns0="http://www.w3.org/2000/09/xmldsig#">' +
  '<tpAmb>2</tpAmb><verAplic>GO4.0</verAplic><cStat>104</cStat><xMotivo>Lote processado</xMotivo><cUF>52</cUF>' +
  '<dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><protNFe versao="4.00"><infProt><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>' +
  `<chNFe>${CHAVE}</chNFe><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><cStat>407</cStat>` +
  '<xMotivo>Rejeição: CPF do Emitente somente no serviço de Nota Fiscal Avulsa no site do Fisco</xMotivo></infProt></protNFe></retEnviNFe>',
);

const PROT_AUTORIZADA =
  `<protNFe versao="4.00"><infProt Id="NFe152260000000001"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic><chNFe>${CHAVE}</chNFe>` +
  '<dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><nProt>152260000000001</nProt><digVal>abc=</digVal><cStat>100</cStat>' +
  '<xMotivo>Autorizado o uso da NF-e</xMotivo></infProt></protNFe>';
const AUTORIZADA = envelope(
  '<retEnviNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>' +
  `<cStat>104</cStat><xMotivo>Lote processado</xMotivo><cUF>52</cUF><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto>${PROT_AUTORIZADA}</retEnviNFe>`,
);

const NFE_ASSINADA = `<?xml version="1.0" encoding="UTF-8"?><NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe Id="NFe${CHAVE}" versao="4.00"></infNFe><Signature xmlns="http://www.w3.org/2000/09/xmldsig#"></Signature></NFe>`;

test('enviNFe: lote síncrono com a NF-e sem a declaração xml', () => {
  const env = montarEnviNFe(NFE_ASSINADA, '123456789012345');
  assert.equal(
    env,
    '<enviNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><idLote>123456789012345</idLote><indSinc>1</indSinc>' +
      NFE_ASSINADA.replace(/^<\?xml[^>]*\?>/, '') + '</enviNFe>',
  );
  assert.throws(() => montarEnviNFe('<NFe></NFe>'), /assinado/);
  assert.match(gerarIdLote(), /^\d{15}$/);
});

test('lê rejeição (formato real da SEFAZ-GO)', () => {
  const r = lerRetEnviNFe(REJEITADA);
  assert.equal(r.lote.cStat, '104');
  assert.equal(r.protocolo.cStat, '407');
  assert.equal(r.protocolo.autorizada, false);
  assert.equal(r.protocolo.chNFe, CHAVE);
  assert.match(r.protocolo.xMotivo, /CPF do Emitente/);
});

test('lê autorização e monta o nfeProc com o protNFe original', () => {
  const r = lerRetEnviNFe(AUTORIZADA);
  assert.equal(r.protocolo.autorizada, true);
  assert.equal(r.protocolo.nProt, '152260000000001');
  assert.equal(r.protocolo.xml, PROT_AUTORIZADA);
  const proc = montarNfeProc(NFE_ASSINADA, r.protocolo.xml);
  assert.ok(proc.startsWith('<?xml version="1.0" encoding="UTF-8"?><nfeProc versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><NFe '));
  assert.ok(proc.endsWith(`</NFe>${PROT_AUTORIZADA}</nfeProc>`));
});

test('lote assíncrono: recibo e consulta', () => {
  const recebido = lerRetEnviNFe(envelope(
    '<retEnviNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><cStat>103</cStat>' +
    '<xMotivo>Lote recebido com sucesso</xMotivo><infRec><nRec>521000000000001</nRec><tMed>1</tMed></infRec></retEnviNFe>',
  ));
  assert.equal(recebido.recibo, '521000000000001');
  assert.equal(recebido.protocolo, null);

  const consulta = lerRetConsReciNFe(envelope(
    '<retConsReciNFe versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><nRec>521000000000001</nRec>' +
    `<cStat>104</cStat><xMotivo>Lote processado</xMotivo>${PROT_AUTORIZADA}</retConsReciNFe>`,
  ));
  assert.equal(consulta.protocolo.autorizada, true);
});
