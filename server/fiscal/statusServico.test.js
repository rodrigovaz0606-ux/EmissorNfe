import { test } from 'node:test';
import assert from 'node:assert/strict';
import { montarConsStatServ, lerRetConsStatServ } from './statusServico.js';
import { carregarConfig } from './config.js';

test('monta consStatServ para GO em homologação', () => {
  const xml = montarConsStatServ({ tpAmb: 2, cUF: 52 });
  assert.equal(
    xml,
    '<consStatServ versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><cUF>52</cUF><xServ>STATUS</xServ></consStatServ>',
  );
});

test('lê retConsStatServ de dentro do envelope SOAP', () => {
  const resposta =
    '<?xml version="1.0" encoding="utf-8"?><soap:Envelope xmlns:soap="http://www.w3.org/2003/05/soap-envelope"><soap:Body>' +
    '<nfeResultMsg xmlns="http://www.portalfiscal.inf.br/nfe/wsdl/NFeStatusServico4">' +
    '<retConsStatServ versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>' +
    '<cStat>107</cStat><xMotivo>Servico em Operacao</xMotivo><cUF>52</cUF><dhRecbto>2026-11-03T10:00:00-03:00</dhRecbto><tMed>1</tMed>' +
    '</retConsStatServ></nfeResultMsg></soap:Body></soap:Envelope>';
  const r = lerRetConsStatServ(resposta);
  assert.equal(r.cStat, '107');
  assert.equal(r.emOperacao, true);
  assert.equal(r.xMotivo, 'Servico em Operacao');
});

test('recusa qualquer ambiente que não seja homologação', () => {
  assert.throws(() => carregarConfig({ NFE_AMBIENTE: 'producao', NFE_UF: 'GO' }), /só opera em homologação/);
  assert.equal(carregarConfig({ NFE_AMBIENTE: 'homologacao', NFE_UF: 'GO' }).tpAmb, 2);
});
