// A tela e o XML precisam calcular igual: monta a mesma nota pelos dois caminhos (tela em JS, XML no backend Rust).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { notaVazia, itemVazio, paraPayload, totaisNota, pendencias, centavos, itemDoProduto, produtoDoItem } from './modelo.js';
import { execFileSync } from 'node:child_process';

// Gera o XML pelo backend em Rust (comando gerar-xml), a partir do mesmo payload que a tela envia.
function gerarXmlNoBackend(payload) {
  return execFileSync('cargo', ['run', '-q', '--manifest-path', 'backend/Cargo.toml', '--', 'gerar-xml'], {
    input: JSON.stringify(payload), encoding: 'utf8', stdio: ['pipe', 'pipe', 'inherit'],
  });
}

function exemploSoja(nota) {
  return {
    ...nota,
    dest: {
      tipo: 'PF', CPF: '123.456.789-09', CNPJ: '', xNome: 'COMPRADOR TESTE', indIEDest: '9', IE: '', email: '',
      ender: { CEP: '74000-000', xLgr: 'AVENIDA TESTE', nro: '100', xCpl: '', xBairro: 'CENTRO', cMun: '5208707', xMun: 'GOIANIA', UF: 'GO', fone: '' },
    },
    itens: [{ ...itemVazio(), cProd: 'SOJA01', xProd: 'SOJA EM GRAOS', NCM: '1201.90.00', CFOP: '5101', uCom: 'SC', qCom: '500', vUnCom: '132,50' }],
    infAdic: { infCpl: 'ICMS DIFERIDO.' },
  };
}

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
  const xml = gerarXmlNoBackend(paraPayload(n));
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

test('CEP do emitente aparece nas pendências', () => {
  const n = notaCompleta();
  n.emit.ender.CEP = '';
  assert.ok(pendencias(n).includes('Emitente: CEP'));
});

test('produto do cadastro vira item da nota e volta igual', () => {
  const produto = {
    id: 7, cProd: 'MILHO01', xProd: 'MILHO', NCM: '10059010', CFOP: '5101', uCom: 'SC', vUnCom: '58,40',
    icms: { orig: '0', CST: '00', pICMS: '12' }, pis: { CST: '01', pPIS: '1,65' }, cofins: { CST: '01', pCOFINS: '7,6' },
  };
  const item = itemDoProduto(produto);
  assert.equal(item.qCom, '1');
  assert.equal(item.vDesc, '');
  assert.equal(item.id, undefined);
  assert.deepEqual(item.icms, produto.icms);
  const { id, ...semId } = produto;
  assert.deepEqual(produtoDoItem(item), semId);
});
