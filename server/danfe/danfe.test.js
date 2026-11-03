import { test } from 'node:test';
import assert from 'node:assert/strict';
import { PADROES, simbolosC, barras, decodificar } from './code128.js';
import { gerarDanfe, lerNotaParaDanfe } from './danfe.js';
import { gerarXmlNFe } from '../fiscal/nfe/gerarXml.js';
import { vendaSoja } from '../../exemplos/venda-soja.js';

test('tabela Code-128: 107 símbolos, 11 módulos cada (stop 13), todos distintos', () => {
  assert.equal(PADROES.length, 107);
  PADROES.forEach((p, i) => {
    const soma = [...p].reduce((s, d) => s + Number(d), 0);
    assert.equal(soma, i === 106 ? 13 : 11, `símbolo ${i}: ${p}`);
    // barras ocupam um número par de módulos e espaços ímpar? Regra da especificação: soma das barras é par.
    const barrasMod = [...p].filter((_, j) => j % 2 === 0).reduce((s, d) => s + Number(d), 0);
    assert.equal(barrasMod % 2, 0, `paridade do símbolo ${i}`);
  });
  assert.equal(new Set(PADROES).size, 107);
  // Padrões de referência da especificação (bits): 0 = 11011001100, Start C = 11010011100, Stop = 1100011101011
  const bits = (p) => [...p].map((w, i) => (i % 2 === 0 ? '1' : '0').repeat(Number(w))).join('');
  assert.equal(bits(PADROES[0]), '11011001100');
  assert.equal(bits(PADROES[105]), '11010011100');
  assert.equal(bits(PADROES[106]), '1100011101011');
});

test('Code-128C: checksum e ida-e-volta com a chave de acesso', () => {
  // Exemplo clássico: "1234" → Start C(105), 12, 34, checksum (105 + 12×1 + 34×2) % 103 = 82, Stop
  assert.deepEqual(simbolosC('1234'), [105, 12, 34, 82, 106]);
  const chave = '52261112345678000195550010000000011482017365';
  const b = barras(chave);
  assert.equal(b.modulos, 11 * (1 + 22 + 1) + 13);
  assert.equal(decodificar(b.barras, b.modulos), chave);
  assert.throws(() => simbolosC('123'), /par/);
});

const xmlExemplo = (ajuste = (d) => d) =>
  gerarXmlNFe(ajuste(vendaSoja({ cpfEmitente: '52998224725', nomeEmitente: 'PRODUTOR TESTE' })), { cUF: 52, tpAmb: 2 });

const paginas = (pdf) => (pdf.toString('latin1').match(/\/Type \/Page\b/g) ?? []).length;

test('DANFE: gera PDF de 1 página para a nota de exemplo', async () => {
  const { xml, chave } = xmlExemplo();
  const n = lerNotaParaDanfe(xml);
  assert.equal(n.chave, chave);
  assert.equal(n.protocolo, null);
  const pdf = await gerarDanfe(xml);
  assert.equal(pdf.subarray(0, 5).toString(), '%PDF-');
  assert.equal(paginas(pdf), 1);
});

test('DANFE: quebra em várias folhas com muitos itens', async () => {
  const { xml } = xmlExemplo((d) => ({
    ...d,
    itens: Array.from({ length: 120 }, (_, i) => ({ ...d.itens[0], cProd: `P${i}`, xProd: `PRODUTO ${i}`, qCom: 1, vUnCom: 1 })),
  }));
  const pdf = await gerarDanfe(xml);
  assert.ok(paginas(pdf) >= 3, `páginas: ${paginas(pdf)}`);
});

test('DANFE: lê protocolo do nfeProc', () => {
  const { xml, chave } = xmlExemplo();
  const proc =
    `<nfeProc versao="4.00" xmlns="http://www.portalfiscal.inf.br/nfe">${xml}<protNFe versao="4.00"><infProt><tpAmb>2</tpAmb>` +
    `<chNFe>${chave}</chNFe><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><nProt>152260000000001</nProt><cStat>100</cStat>` +
    '<xMotivo>Autorizado o uso da NF-e</xMotivo></infProt></protNFe></nfeProc>';
  assert.equal(lerNotaParaDanfe(proc).protocolo.nProt, '152260000000001');
});
