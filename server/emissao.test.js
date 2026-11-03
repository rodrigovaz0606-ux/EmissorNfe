// Integração backend: API + banco em memória + SEFAZ falsa (nada é enviado).
import { describe, test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { criarApp } from './index.js';
import { abrirBanco, repositorio } from './db.js';
import { sefazSimulada } from './sefazSimulada.js';
import { certificadoDeTeste } from './fiscal/certificadoTeste.js';
import { descreverCertificado } from './fiscal/certificado.js';
import forge from 'node-forge';
import { vendaSoja } from '../exemplos/venda-soja.js';

const config = { uf: 'GO', cUF: 52, tpAmb: 2 };
const cert = certificadoDeTeste('EMPRESA TESTE:12345678000195');
cert.info = descreverCertificado(forge.pki.certificateFromPem(cert.certPem));

// SEFAZ falsa que rejeita a primeira tentativa e autoriza a segunda.
let rejeitarProxima = false;
const sefaz = {
  ...sefazSimulada,
  async autorizar(...args) {
    if (!rejeitarProxima) return sefazSimulada.autorizar(...args);
    rejeitarProxima = false;
    return { lote: { cStat: '104', xMotivo: 'Lote processado' }, recibo: null, respostaSoap: '<x/>',
      protocolo: { cStat: '539', xMotivo: 'Rejeicao: Duplicidade (teste)', autorizada: false, xml: null } };
  },
};

let servidor;
let base;
const repo = repositorio(abrirBanco(':memory:'));

const post = (url, corpo) => fetch(base + url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(corpo) });
const nota = (nNF) => {
  const d = vendaSoja({ cpfEmitente: '52998224725', nomeEmitente: 'EMPRESA TESTE' });
  delete d.emit.CPF;
  d.emit.CNPJ = '12345678000195';
  d.ide.nNF = nNF;
  return d;
};

describe('API com SEFAZ falsa', () => {
before(async () => {
  servidor = criarApp({ config, certificado: cert, repo, sefaz }).listen(0, '127.0.0.1');
  await new Promise((r) => servidor.once('listening', r));
  base = `http://127.0.0.1:${servidor.address().port}/api`;
});
after(() => servidor.close());

test('fluxo completo: rejeição, autorização, numeração, DANFE, CC-e, cancelamento, inutilização', async () => {
  assert.equal((await (await fetch(`${base}/numeracao/1`)).json()).proximo, 1);

  // 1) rejeitada: grava, mas não consome o número
  rejeitarProxima = true;
  let r = await (await post('/nfe/emitir', nota(1))).json();
  assert.equal(r.status, 'rejeitada');
  assert.equal(r.protocolo.cStat, '539');
  assert.equal((await (await fetch(`${base}/numeracao/1`)).json()).proximo, 1);

  // 2) autorizada: consome o número e gera nfeProc
  r = await (await post('/nfe/emitir', nota(1))).json();
  assert.equal(r.status, 'autorizada', JSON.stringify(r));
  const id = r.id;
  assert.equal((await (await fetch(`${base}/numeracao/1`)).json()).proximo, 2);

  // número repetido é barrado antes de enviar
  const dup = await post('/nfe/emitir', nota(1));
  assert.equal(dup.status, 400);
  assert.match((await dup.json()).erro, /já foi usado.*Próximo disponível: 2/);

  // listagem e detalhe
  const lista = await (await fetch(`${base}/notas`)).json();
  assert.equal(lista.total, 2);
  assert.deepEqual(lista.notas.map((n) => n.status), ['autorizada', 'rejeitada']);
  const det = await (await fetch(`${base}/notas/${id}`)).json();
  assert.equal(det.tem_proc, true);
  assert.equal(det.dados.ide.nNF, 1);

  // XML autorizado e DANFE
  const xml = await (await fetch(`${base}/notas/${id}/xml`)).text();
  assert.match(xml, /^<\?xml[^>]*\?><nfeProc .*<\/nfeProc>$/s);
  const danfe = await fetch(`${base}/notas/${id}/danfe`);
  assert.equal(danfe.headers.get('content-type'), 'application/pdf');
  assert.equal(Buffer.from(await danfe.arrayBuffer()).subarray(0, 5).toString(), '%PDF-');

  // CC-e: sequência 1 e 2
  const c1 = await (await post(`/notas/${id}/cce`, { xCorrecao: 'Corrigir complemento do endereco para Sala 2' })).json();
  const c2 = await (await post(`/notas/${id}/cce`, { xCorrecao: 'Corrigir complemento do endereco para Sala 3' })).json();
  assert.deepEqual([c1.nSeqEvento, c2.nSeqEvento, c1.registrado], [1, 2, true]);
  const curta = await post(`/notas/${id}/cce`, { xCorrecao: 'curta' });
  assert.equal(curta.status, 400);

  // cancelamento
  assert.equal((await post(`/notas/${id}/cancelar`, { xJust: 'curta' })).status, 400);
  const canc = await (await post(`/notas/${id}/cancelar`, { xJust: 'Cancelamento de teste automatizado' })).json();
  assert.equal(canc.registrado, true);
  const depois = await (await fetch(`${base}/notas/${id}`)).json();
  assert.equal(depois.status, 'cancelada');
  assert.equal(depois.eventos.length, 3);
  const evXml = await (await fetch(`${base}/eventos/${depois.eventos[2].id}/xml`)).text();
  assert.match(evXml, /<procEventoNFe .*<tpEvento>110111<\/tpEvento>.*<\/procEventoNFe>$/s);
  // cancelada continua consumindo o número; não cancela de novo
  assert.equal((await (await fetch(`${base}/numeracao/1`)).json()).proximo, 2);
  assert.equal((await post(`/notas/${id}/cancelar`, { xJust: 'Cancelamento de teste automatizado' })).status, 400);

  // inutilização: não pode incluir número usado; faixa livre avança a numeração
  assert.equal((await post('/inutilizacoes', { serie: 1, nNFIni: 1, nNFFin: 3, xJust: 'Teste de inutilizacao automatizado' })).status, 400);
  const inut = await (await post('/inutilizacoes', { serie: 1, nNFIni: 2, nNFFin: 4, xJust: 'Teste de inutilizacao automatizado' })).json();
  assert.equal(inut.homologada, true);
  assert.equal((await (await fetch(`${base}/numeracao/1`)).json()).proximo, 5);
});

test('API: erros de dados viram 400 com mensagem; prévia do DANFE', async () => {
  const d = nota(9);
  d.itens[0].NCM = '12';
  const r = await post('/nfe/validar', d);
  assert.equal(r.status, 400);
  assert.match((await r.json()).erro, /NCM/);
  const previa = await post('/nfe/danfe-previa', nota(9));
  assert.equal(previa.headers.get('content-type'), 'application/pdf');
  assert.equal((await fetch(`${base}/notas/999`)).status, 404);
});
});
