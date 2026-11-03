import { test } from 'node:test';
import assert from 'node:assert/strict';
import { gerarXmlNFe, NOME_DEST_HOMOLOGACAO } from './gerarXml.js';
import { validarXsd } from './validarXsd.js';
import { validarChave } from '../chaveAcesso.js';
import { cpfValido, cnpjValido } from './documentos.js';
import { centavos, multiplicarEmCentavos, paraCentavos, unitario, dataHora, texto } from './formatos.js';
import { vendaSoja } from '../../../exemplos/venda-soja.js';

const config = { cUF: 52, tpAmb: 2 };
const DH = '2026-11-03T10:00:00-03:00';
// CPF fictício (DV válido) para não depender do certificado real nos testes.
const cenario = (extra = {}) => {
  const d = vendaSoja({ cpfEmitente: '52998224725', nomeEmitente: 'PRODUTOR TESTE' });
  return { ...d, ide: { ...d.ide, dhEmi: DH, cNF: '48201736' }, ...extra };
};
const pegar = (xml, t) => xml.match(new RegExp(`<${t}>([^<]*)</${t}>`))?.[1];

test('cenário da soja passa no XSD (faltando só a assinatura)', async () => {
  const { xml } = gerarXmlNFe(cenario(), config);
  const { erros } = await validarXsd(xml);
  assert.equal(erros.length, 1, erros.join('\n'));
  assert.match(erros[0], /Expected is one of \( infNFeSupl, ds:Signature \)/);
});

test('chave, Id e campos do ide são coerentes', () => {
  const { xml, chave } = gerarXmlNFe(cenario(), config);
  assert.ok(validarChave(chave));
  assert.match(xml, new RegExp(`<infNFe Id="NFe${chave}" versao="4.00">`));
  assert.equal(pegar(xml, 'cNF'), '48201736');
  assert.equal(pegar(xml, 'cDV'), chave.at(-1));
  assert.equal(pegar(xml, 'tpAmb'), '2');
  assert.equal(chave.slice(6, 20), '00052998224725');
});

test('totais: 500 sacas × 132,50 = 66.250,00 e pagamento igual ao total', () => {
  const { xml, vNF } = gerarXmlNFe(cenario(), config);
  assert.equal(vNF, '66250.00');
  assert.equal(pegar(xml, 'vProd'), '66250.00');
  assert.equal(pegar(xml, 'vNF'), '66250.00');
  assert.equal(pegar(xml, 'vPag'), '66250.00');
  assert.equal(pegar(xml, 'qCom'), '500.0000');
  assert.equal(pegar(xml, 'vUnCom'), '132.50');
});

test('em homologação o nome do destinatário é o texto obrigatório', () => {
  const { xml } = gerarXmlNFe(cenario(), config);
  assert.match(xml, new RegExp(`<dest><CPF>12345678909</CPF><xNome>${NOME_DEST_HOMOLOGACAO}</xNome>`));
});

test('ICMS 00 e PIS/COFINS por alíquota calculam valores e totais', async () => {
  const d = cenario();
  d.itens = [
    { ...d.itens[0], icms: { CST: '00', pICMS: 12 }, pis: { CST: '01', pPIS: 1.65 }, cofins: { CST: '01', pCOFINS: 7.6 } },
    { ...d.itens[0], cProd: 'MILHO01', xProd: 'MILHO', NCM: '10059010', qCom: 3, vUnCom: 0.335, vDesc: 0.01 },
  ];
  const { xml, vNF } = gerarXmlNFe(d, config);
  const tot = xml.match(/<ICMSTot>.*<\/ICMSTot>/)[0];
  assert.equal(pegar(tot, 'vBC'), '66250.00');
  assert.equal(pegar(tot, 'vICMS'), '7950.00'); // 12%
  assert.equal(pegar(tot, 'vPIS'), '1093.13'); // 1,65% de 66.250 = 1.093,125 → 1.093,13
  assert.equal(pegar(tot, 'vCOFINS'), '5035.00');
  assert.equal(pegar(tot, 'vProd'), '66251.01'); // 3 × 0,335 = 1,005 → 1,01
  assert.equal(pegar(tot, 'vDesc'), '0.01');
  assert.equal(vNF, '66251.00');
  const { erros } = await validarXsd(xml);
  assert.equal(erros.length, 1, erros.join('\n'));
});

test('textos são limpos e caracteres especiais escapados', () => {
  const d = cenario();
  d.itens[0].xProd = '  SOJA   "A&B" <TIPO 1>  ';
  const { xml } = gerarXmlNFe(d, config);
  assert.match(xml, /<xProd>SOJA &quot;A&amp;B&quot; &lt;TIPO 1&gt;<\/xProd>/);
});

test('validações de entrada', () => {
  assert.throws(() => gerarXmlNFe(cenario(), { cUF: 52, tpAmb: 1 }), /homologação/);
  assert.throws(() => gerarXmlNFe(cenario({ itens: [] }), config), /1 a 990 itens/);
  const d = cenario();
  d.emit.CPF = '11111111111';
  assert.throws(() => gerarXmlNFe(d, config), /CPF do emitente inválido/);
  const d2 = cenario();
  d2.itens[0].icms = { CST: '60' };
  assert.throws(() => gerarXmlNFe(d2, config), /CST de ICMS "60" ainda não suportado/);
});

test('formatos', () => {
  assert.equal(centavos(123456), '1234.56');
  assert.equal(centavos(5), '0.05');
  assert.equal(multiplicarEmCentavos(3, 0.335), 101);
  assert.equal(paraCentavos('1,005'), 101);
  assert.equal(unitario(132.5), '132.50');
  assert.equal(unitario(0.1234567), '0.1234567');
  assert.equal(dataHora(new Date('2026-11-03T13:00:00Z')), '2026-11-03T10:00:00-03:00');
  assert.equal(texto(' a \n  b '), 'a b');
  assert.ok(cpfValido('529.982.247-25'));
  assert.ok(!cpfValido('529.982.247-24'));
  assert.ok(cnpjValido('12.345.678/0001-95'));
  assert.ok(!cnpjValido('12.345.678/0001-96'));
});

test('bases: ICMS = produto − desconto + frete; PIS/COFINS = produto − desconto', () => {
  const d = cenario();
  d.itens = [{
    ...d.itens[0], qCom: 200, vUnCom: 58.4, vDesc: 120, vFrete: 50,
    icms: { CST: '00', pICMS: 12 }, pis: { CST: '01', pPIS: 1.65 }, cofins: { CST: '01', pCOFINS: 7.6 },
  }];
  const { xml, vNF } = gerarXmlNFe(d, config);
  const icms = xml.match(/<ICMS00>.*<\/ICMS00>/)[0];
  assert.equal(pegar(icms, 'vBC'), '11610.00'); // 11.680 − 120 + 50
  assert.equal(pegar(icms, 'vICMS'), '1393.20');
  assert.equal(pegar(xml.match(/<PISAliq>.*<\/PISAliq>/)[0], 'vBC'), '11560.00'); // 11.680 − 120
  assert.equal(pegar(xml.match(/<PISAliq>.*<\/PISAliq>/)[0], 'vPIS'), '190.74');
  assert.equal(pegar(xml.match(/<COFINSAliq>.*<\/COFINSAliq>/)[0], 'vCOFINS'), '878.56');
  assert.equal(vNF, '11610.00');
  d.itens[0].vDesc = 99999;
  assert.throws(() => gerarXmlNFe(d, config), /desconto maior/);
});
