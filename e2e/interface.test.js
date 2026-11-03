// Testes de ponta a ponta da interface, no Edge, com a SEFAZ SIMULADA (nada é enviado à SEFAZ).
// Rode com: npm run e2e
import { describe, test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright-core';

const PORTA = 3399;
const URL_APP = `http://127.0.0.1:${PORTA}`;
const DB = path.resolve('data', 'e2e.sqlite');
const PRINTS = process.env.E2E_PRINTS; // pasta opcional para salvar capturas de tela

let servidor;
let edge;
let navegador;
let pagina;
const errosDaPagina = [];

// O Playwright compatível com Node 18 não consegue iniciar o Edge 156 sozinho,
// então abrimos o Edge com a porta de depuração e conectamos via CDP.
const EDGE = ['C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', 'C:/Program Files/Microsoft/Edge/Application/msedge.exe']
  .find((f) => fs.existsSync(f));
const PORTA_CDP = 9334;

async function abrirEdge() {
  if (!EDGE) throw new Error('Microsoft Edge não encontrado');
  const perfil = fs.mkdtempSync(path.join(os.tmpdir(), 'emissor-e2e-'));
  edge = spawn(EDGE, ['--headless=new', `--remote-debugging-port=${PORTA_CDP}`, `--user-data-dir=${perfil}`, '--no-first-run', '--no-default-browser-check', 'about:blank'], { stdio: 'ignore' });
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(`http://127.0.0.1:${PORTA_CDP}/json/version`)).ok) return chromium.connectOverCDP(`http://127.0.0.1:${PORTA_CDP}`);
    } catch { /* subindo */ }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error('Edge não abriu a porta de depuração');
}

async function esperarServidor() {
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(`${URL_APP}/api/emissor`)).ok) return;
    } catch { /* ainda subindo */ }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error('Servidor não subiu');
}

const print = async (nome) => PRINTS && pagina.screenshot({ path: path.join(PRINTS, `${nome}.png`), fullPage: true });

describe('interface (Edge + SEFAZ simulada)', () => {
  before(async () => {
    for (const f of [DB, `${DB}-wal`, `${DB}-shm`]) fs.rmSync(f, { force: true });
    servidor = spawn(process.execPath, ['server/index.js'], {
      env: { ...process.env, PORT: String(PORTA), NFE_SEFAZ_SIMULADA: '1', NFE_DB: DB },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    servidor.stderr.on('data', (d) => process.stderr.write(`[api] ${d}`));
    await esperarServidor();
    navegador = await abrirEdge();
    pagina = await (await navegador.newContext({ viewport: { width: 1440, height: 900 }, locale: 'pt-BR' })).newPage();
    pagina.on('dialog', (d) => d.accept()); // confirmações de "Emitir" e "Nova nota"
    pagina.on('pageerror', (e) => errosDaPagina.push(e.message));
  });

  after(async () => {
    await navegador?.close().catch(() => {});
    edge?.kill();
    servidor?.kill();
  });

  test('abre com dados do certificado e faixa de SEFAZ simulada', async () => {
    await pagina.goto(URL_APP);
    await pagina.evaluate(() => localStorage.clear());
    await pagina.reload();
    await pagina.getByText('SEFAZ SIMULADA — modo de teste').waitFor();
    await pagina.getByText('SEFAZ em operação').waitFor();
    assert.match(await pagina.locator('.certificado').innerText(), /e-CPF/);
    assert.equal(await pagina.getByRole('button', { name: 'Emitir nota' }).isDisabled(), true);
    assert.match(await pagina.locator('.pendencias summary').innerText(), /\d+ pendências/);
  });

  test('preenche a nota, busca CEP e valida (assinatura + XSD)', async () => {
    await pagina.getByRole('button', { name: 'Carregar exemplo (soja)' }).click();

    const emitente = pagina.locator('#emitente');
    await emitente.getByLabel('Inscrição estadual').fill('109876547');
    // CEP do emitente é obrigatório: sem ele a nota não fica completa
    assert.match(await pagina.locator('.pendencias').innerText(), /Emitente: CEP/);
    await emitente.getByLabel('CEP').fill('74000-000');
    await emitente.getByLabel('Logradouro').fill('RODOVIA GO-060 KM 10');
    await emitente.getByLabel('Bairro').fill('ZONA RURAL');
    await emitente.getByLabel('Município').fill('GOIANIA');
    await emitente.getByLabel('Cód. IBGE').fill('5208707');

    // Busca de CEP real (ViaCEP) no destinatário. Se a internet falhar, o teste avisa e segue.
    const dest = pagina.locator('#destinatario');
    await dest.getByLabel('CEP').fill('74003-010');
    await dest.getByRole('button', { name: 'Buscar' }).click();
    try {
      await pagina.waitForFunction(() => document.querySelectorAll('#destinatario input')[0] && [...document.querySelectorAll('#destinatario input')].some((i) => i.value === '5208707'), null, { timeout: 8000 });
    } catch {
      console.warn('  ! ViaCEP não respondeu; preenchendo o município à mão');
      await dest.getByLabel('Cód. IBGE').fill('5208707');
    }

    await pagina.getByText('Todos os campos obrigatórios preenchidos').waitFor();
    await print('1-preenchida');

    await pagina.getByRole('button', { name: 'Validar' }).click();
    await pagina.locator('.resultado').waitFor();
    const resultado = await pagina.locator('.resultado').innerText();
    assert.match(resultado, /Pronta para emitir/);
    assert.match(resultado, /Assinatura digital\s+Conferida/);
    assert.match(resultado, /Schema \(XSD\)\s+Estrutura válida/);
  });

  test('prévia do DANFE devolve PDF', async () => {
    const [resposta] = await Promise.all([
      pagina.waitForResponse((r) => r.url().endsWith('/api/nfe/danfe-previa')),
      pagina.getByRole('button', { name: 'Pré-visualizar DANFE' }).click(),
    ]);
    assert.equal(resposta.status(), 200);
    assert.equal(resposta.headers()['content-type'], 'application/pdf');
    for (const p of pagina.context().pages()) if (p !== pagina) await p.close();
  });

  test('emite (simulado), mostra protocolo e oferece DANFE', async () => {
    await pagina.getByRole('button', { name: 'Emitir nota' }).click();
    await pagina.locator('.resultado').waitFor({ timeout: 15000 });
    const resultado = await pagina.locator('.resultado').innerText();
    assert.match(resultado, /Nota autorizada/);
    assert.match(resultado, /100 — Autorizado o uso da NF-e \(SIMULADO\)/);
    assert.match(resultado, /Protocolo \d{15}/);
    const href = await pagina.getByRole('link', { name: 'DANFE (PDF)' }).getAttribute('href');
    const pdf = await fetch(URL_APP + href);
    assert.equal(pdf.headers.get('content-type'), 'application/pdf');
    await print('2-autorizada');
  });

  test('lista de notas, carta de correção e cancelamento', async () => {
    await pagina.getByRole('button', { name: 'Ver em Notas emitidas ›' }).click();
    const linha = pagina.locator('.tabela-notas tbody tr').first();
    await linha.getByText('Autorizada').waitFor();
    await print('3-lista');

    await linha.click();
    const janela = pagina.getByRole('dialog');
    await janela.getByText('Nenhum evento registrado.').waitFor();

    // CC-e: botão só habilita com 15+ caracteres
    await janela.getByRole('button', { name: 'Carta de correção' }).click();
    const cce = pagina.getByRole('dialog');
    await cce.locator('textarea').fill('curto');
    assert.equal(await cce.getByRole('button', { name: 'Enviar carta de correção' }).isDisabled(), true);
    await cce.locator('textarea').fill('Corrigir o complemento do endereco do destinatario para Sala 2');
    await cce.getByRole('button', { name: 'Enviar carta de correção' }).click();
    await pagina.getByText(/Carta de correção: 135/).waitFor();
    await pagina.getByText('Carta de correção nº 1').waitFor();

    // Cancelamento
    await pagina.getByRole('dialog').getByRole('button', { name: 'Cancelar nota' }).click();
    const canc = pagina.getByRole('dialog');
    await canc.locator('textarea').fill('Cancelamento de teste da interface');
    await canc.getByRole('button', { name: 'Cancelar a nota' }).click();
    await pagina.getByText(/Cancelamento: 135/).waitFor();
    await print('4-cancelada');
    assert.equal(await pagina.getByRole('dialog').getByRole('button', { name: 'Cancelar nota' }).isDisabled(), true);

    await pagina.keyboard.press('Escape');
    await pagina.locator('.tabela-notas tbody tr').first().getByText('Cancelada').waitFor();
    assert.match(await pagina.locator('.tabela-notas tbody tr').first().innerText(), /CC-e 1/);
  });

  test('inutilização fica bloqueada para emitente com CPF', async () => {
    const botao = pagina.getByRole('button', { name: 'Inutilizar numeração' });
    assert.equal(await botao.isDisabled(), true);
    assert.match(await botao.getAttribute('title'), /CNPJ/);
  });

  test('nova nota usa o próximo número livre e pendências aparecem', async () => {
    await pagina.getByRole('tab', { name: 'Emitir nota' }).click();
    await pagina.getByRole('button', { name: 'Nova nota' }).click();
    await pagina.getByRole('heading', { name: /Nota fiscal nº 2/ }).waitFor();
    // Emitente foi mantido, destinatário e itens não
    assert.equal(await pagina.locator('#emitente').getByLabel('Inscrição estadual').inputValue(), '109876547');
    assert.match(await pagina.locator('.pendencias summary').innerText(), /pendência/);
    assert.equal(await pagina.getByRole('button', { name: 'Emitir nota' }).isDisabled(), true);
  });

  test('NCM inválido aparece como erro no campo e nas pendências', async () => {
    await pagina.getByRole('button', { name: 'Carregar exemplo (soja)' }).click();
    const ncm = pagina.locator('#itens').getByLabel('NCM');
    await ncm.fill('1201');
    await ncm.blur();
    await pagina.locator('#itens').getByText('8 dígitos').waitFor();
    await pagina.locator('.pendencias').getByText('Item 1: NCM com 8 dígitos').waitFor();
    await print('5-erro-ncm');
  });

  test('nenhum erro de JavaScript na página', () => {
    assert.deepEqual(errosDaPagina, []);
  });
});
