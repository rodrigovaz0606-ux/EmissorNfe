// A tela e o XML precisam calcular igual: monta a mesma nota pelos dois caminhos.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { notaVazia, exemploSoja, itemVazio, paraPayload, totaisNota, pendencias, centavos } from './modelo.js';
import { gerarXmlNFe } from '../server/fiscal/nfe/gerarXml.js';

function notaCompleta() {
  const n = exemploSoja(notaVazia());
  n.emit = {
    ...n.emit, CPF: '52998224725', xNome: 'PRODUTOR TESTE', IE: '109876547',
    ender: { CEP: '74000000', xLgr: 'RODOVIA', nro: 'SN', xCpl: '', xBairro: 'ZONA RURAL', cMun: '5208707', xMun: 'GOIANIA', UF: 'GO', fone: '' },
  };
  n.itens.push({
    ...itemVazio(), xProd: 'MILHO', NCM: '1005.90.10', CFOP: '5101', uCom: 'SC', qCom: '200', vUnCom: '58,40', vDesc: '120,00', vFrete: '50',
    icms: { orig: '0', CST: '00', pICMS: '12' }, pis: { CST: '01', pPIS: '1,65' }, cofins: { CST: '01', pCOFINS: '7,6' },
  });
  return n;
}

const pegar = (xml, t) => xml.match(new RegExp(`<ICMSTot>.*<${t}>([^<]*)</${t}>.*</ICMSTot>`))?.[1];

test('nota de exemplo completa não tem pendências', () => {
  assert.deepEqual(pendencias(notaCompleta()), []);
});

test('totais da tela = totais do XML', () => {
  const n = notaCompleta();
  const t = totaisNota(n);
  const { xml } = gerarXmlNFe(paraPayload(n), { cUF: 52, tpAmb: 2 });
  for (const [campo, valor] of Object.entries({ vProd: t.vProd, vDesc: t.vDesc, vFrete: t.vFrete, vBC: t.vBC, vICMS: t.vICMS, vPIS: t.vPIS, vCOFINS: t.vCOFINS, vNF: t.vNF })) {
    assert.equal(pegar(xml, campo), centavos(valor), campo);
  }
});

test('números digitados no padrão brasileiro', () => {
  const p = paraPayload(notaCompleta());
  assert.equal(p.itens[0].vUnCom, 132.5);
  assert.equal(p.itens[1].vDesc, 120);
  assert.equal(p.itens[1].cofins.pCOFINS, 7.6);
  assert.equal(p.emit.ender.CEP, '74000000');
  assert.equal(p.ide.idDest, 1);
});
