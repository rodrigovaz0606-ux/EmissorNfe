// A tela e o XML precisam calcular igual: monta a mesma nota pelos dois caminhos (tela em JS, XML no backend Rust).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  notaVazia, itemVazio, paraPayload, totaisNota, pendencias, avisos, centavos, itemDoProduto, produtoDoItem,
  aplicarTipoNota, cfopDoTipo, notaDoPayload, destDoCliente, avisosEstoque, estoqueBaixo,
} from './modelo.js';
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

test('duplicar: os dados guardados voltam para o formulário e geram o mesmo payload', () => {
  const n = notaCompleta();
  n.ide.nNF = '12';
  const payload = JSON.parse(JSON.stringify(paraPayload(n)));
  const copia = notaDoPayload(payload, n);
  assert.equal(copia.ide.nNF, '', 'o número fica para a tela sugerir');
  assert.equal(copia.itens[1].vUnCom, '58,4');
  assert.equal(copia.ide.tipoNota, 'venda_producao');
  copia.ide.nNF = '12';
  assert.deepEqual(JSON.parse(JSON.stringify(paraPayload(copia))), payload);
  assert.deepEqual(pendencias(copia), []);
});

test('tipo de nota: devolução exige referência, muda CFOP e tira o pagamento', () => {
  const n = aplicarTipoNota(notaCompleta(), 'devolucao_compra');
  assert.equal(n.ide.finNFe, '4');
  assert.equal(n.ide.natOp, 'DEVOLUCAO DE COMPRA');
  assert.deepEqual(n.itens.map((i) => i.CFOP), ['5201', '5201']);
  assert.deepEqual(n.pag, [{ tPag: '90', vPag: '' }]);
  assert.ok(pendencias(n).includes('Chave da NF-e referenciada'));

  n.ide.refNFe = ['123'];
  assert.ok(pendencias(n).some((p) => p.includes('44 dígitos')));
  n.ide.refNFe = ['5226 1112 3456 7800 0195 5500 1000 0000 0114 8201 7365'];
  assert.ok(!pendencias(n).some((p) => p.includes('referenciada')));
  assert.deepEqual(paraPayload(n).ide.refNFe, ['52261112345678000195550010000000011482017365']);
  assert.deepEqual(avisos(n), []);

  // voltando para venda, a referência não vai no payload e o pagamento volta
  const venda = aplicarTipoNota(n, 'venda_producao');
  assert.equal(paraPayload(venda).ide.refNFe, undefined);
  assert.equal(venda.pag[0].tPag, '01');
});

test('tipo de nota: entrada e operação interestadual usam o CFOP certo', () => {
  const n = aplicarTipoNota(notaCompleta(), 'entrada_compra');
  assert.equal(n.ide.tpNF, '0');
  assert.equal(n.itens[0].CFOP, '1101');
  assert.deepEqual(avisos(n), []);
  n.dest.ender.UF = 'SP';
  assert.equal(cfopDoTipo(n), '2101');
  assert.ok(avisos(n).some((a) => a.includes('2xxx')));
  assert.equal(aplicarTipoNota(n, 'entrada_compra').itens[0].CFOP, '2101');
  assert.equal(aplicarTipoNota(n, '').ide.tipoNota, '');
});

test('cliente do cadastro vira destinatário sem o id', () => {
  const d = destDoCliente({ id: 3, tipo: 'PJ', CNPJ: '12345678000195', xNome: 'X', indIEDest: '1', IE: '10', ender: { xLgr: 'RUA' } });
  assert.equal(d.id, undefined);
  assert.equal(d.tipo, 'PJ');
  assert.equal(d.ender.xLgr, 'RUA');
  assert.equal(d.ender.UF, 'GO');
});

test('estoque: avisa quando a nota pede mais que o saldo, somando itens repetidos', () => {
  const produtos = [
    { cProd: 'SOJA01', uCom: 'SC', controlaEstoque: true, saldo: 100 },
    { cProd: 'LIVRE', uCom: 'UN', controlaEstoque: false, saldo: 0 },
  ];
  const n = notaVazia();
  const item = (cProd, qCom) => ({ ...itemVazio(), cProd, qCom });
  n.itens = [item('soja01', '60'), item('LIVRE', '5')];
  assert.deepEqual(avisosEstoque(n, produtos), []);
  n.itens.push(item('SOJA01', '50,5'));
  assert.deepEqual(avisosEstoque(n, produtos), ['SOJA01: a nota pede 110,5 SC, e o estoque tem 100']);
  assert.deepEqual(avisosEstoque({ ...n, ide: { ...n.ide, tpNF: '0' } }, produtos), [], 'entrada não falta');
  assert.deepEqual(avisosEstoque({ ...n, ide: { ...n.ide, finNFe: '2' } }, produtos), [], 'complementar não move');
  assert.deepEqual(avisosEstoque(n, null), []);
  assert.equal(estoqueBaixo({ controlaEstoque: true, saldo: 5, estoqueMinimo: '10,5' }), true);
  assert.equal(estoqueBaixo({ controlaEstoque: true, saldo: 11, estoqueMinimo: '10,5' }), false);
  assert.equal(estoqueBaixo({ controlaEstoque: false, saldo: -1, estoqueMinimo: '' }), false);
});
