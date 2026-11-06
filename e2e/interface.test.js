// Testes de ponta a ponta da interface, no Edge, com a SEFAZ SIMULADA (nada é enviado à SEFAZ).
// Rode com: npm run e2e
import { describe, test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright-core';

const PORTA = 3399;
const URL_APP = `http://127.0.0.1:${PORTA}`;
const DB = path.resolve('data', 'e2e.sqlite');
const PRINTS = process.env.E2E_PRINTS; // pasta opcional para salvar capturas de tela
// Backend em Rust, compilado em modo release pelo script npm run e2e.
const BINARIO = path.resolve('backend', 'target', 'release', process.platform === 'win32' ? 'emissor-nfe.exe' : 'emissor-nfe');
// Certificados FICTÍCIOS de teste (autoassinados, senha "teste"). Nenhum certificado real é usado.
const PFX_CPF = path.resolve('backend', 'tests', 'fixtures', 'teste-cpf.pfx'); // PRODUTOR TESTE, CPF 529.982.247-25
const PFX_CNPJ = path.resolve('backend', 'tests', 'fixtures', 'teste.pfx'); // EMPRESA TESTE, CNPJ 12.345.678/0001-95

let servidor;
let edge;
let perfilEdge; // pasta temporária exclusiva deste Edge de teste
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
  // Um Edge esquecido de outra execução na mesma porta faria o teste conversar com ele: falha logo.
  if (await fetch(`http://127.0.0.1:${PORTA_CDP}/json/version`).then(() => true, () => false)) {
    throw new Error(`Já existe um Edge de teste na porta ${PORTA_CDP}; feche-o antes de rodar o e2e.`);
  }
  perfilEdge = fs.mkdtempSync(path.join(os.tmpdir(), 'emissor-e2e-'));
  edge = spawn(EDGE, ['--headless=new', `--remote-debugging-port=${PORTA_CDP}`, `--user-data-dir=${perfilEdge}`, '--no-first-run', '--no-default-browser-check', 'about:blank'], { stdio: 'ignore' });
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
      if ((await fetch(`${URL_APP}/api/auth/estado`)).ok) return;
    } catch { /* ainda subindo */ }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error('Servidor não subiu');
}

const print = async (nome) => PRINTS && pagina.screenshot({ path: path.join(PRINTS, `${nome}.png`), fullPage: true });

describe('interface (Edge + SEFAZ simulada)', () => {
  before(async () => {
    // Só o banco de teste. Nunca apague data/chave-mestra.key: é a chave real (o teste usa NFE_CHAVE_MESTRA).
    for (const f of [DB, `${DB}-wal`, `${DB}-shm`]) fs.rmSync(f, { force: true });
    if (!fs.existsSync(BINARIO)) throw new Error(`Backend não compilado: ${BINARIO} (rode npm run e2e)`);
    servidor = spawn(BINARIO, ['servidor'], {
      env: { ...process.env, PORT: String(PORTA), NFE_SEFAZ_SIMULADA: '1', NFE_DB: DB, NFE_CHAVE_MESTRA: 'ab'.repeat(32) },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    servidor.stderr.on('data', (d) => process.stderr.write(`[api] ${d}`));
    servidor.on('exit', (c) => c && console.error(`[api] servidor saiu com código ${c}`));
    await esperarServidor();
    navegador = await abrirEdge();
    pagina = await (await navegador.newContext({ viewport: { width: 1440, height: 900 }, locale: 'pt-BR' })).newPage();
    pagina.on('dialog', (d) => d.accept()); // confirmações de "Emitir" e "Nova nota"
    pagina.on('pageerror', (e) => errosDaPagina.push(e.message));
  });

  after(async () => {
    await navegador?.close().catch(() => {});
    // O msedge.exe iniciado repassa para outro processo e sai; por isso encerra todos os
    // processos do Edge que usam o perfil temporário deste teste (e só eles).
    if (perfilEdge && process.platform === 'win32') {
      const filtro = `$_.CommandLine -like '*${path.basename(perfilEdge)}*'`;
      spawnSync('powershell', ['-NoProfile', '-Command',
        `Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { ${filtro} } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`],
      { stdio: 'ignore' });
    } else {
      edge?.kill();
    }
    servidor?.kill();
  });

  test('primeiro acesso cria o administrador', async () => {
    await pagina.goto(URL_APP);
    await pagina.evaluate(() => localStorage.clear());
    await pagina.reload();
    await pagina.getByRole('heading', { name: 'Primeiro acesso' }).waitFor();
    await pagina.getByLabel('Seu nome').fill('Rodrigo Teste');
    await pagina.getByLabel('Usuário').fill('rodrigo');
    await pagina.getByLabel('Senha', { exact: true }).fill('senha-do-admin');
    await pagina.getByLabel('Confirme a senha').fill('senha-diferente');
    await pagina.getByText('As senhas não conferem.').waitFor();
    await pagina.getByLabel('Confirme a senha').fill('senha-do-admin');
    await print('00-primeiro-acesso');
    await pagina.getByRole('button', { name: 'Criar administrador e entrar' }).click();
    await pagina.getByRole('heading', { name: 'Escolha o produtor' }).waitFor();
    assert.equal(await pagina.getByRole('tab', { name: 'Emitir nota' }).count(), 0); // sem produtor, não emite
    await pagina.getByRole('tab', { name: 'Administração' }).waitFor();
  });

  test('adiciona o produtor pelo certificado (senha errada é recusada)', async () => {
    const enviar = async (arquivo, senha) => {
      await pagina.getByRole('button', { name: '+ Adicionar produtor' }).click();
      const janela = pagina.getByRole('dialog');
      await janela.getByLabel('Arquivo do certificado').setInputFiles(arquivo);
      await janela.getByLabel('Senha do certificado').fill(senha);
      await janela.getByRole('button', { name: 'Adicionar e usar' }).click();
      return janela;
    };
    const janela = await enviar(PFX_CPF, 'errada');
    await janela.getByText(/senha incorreta/).waitFor();
    await janela.getByRole('button', { name: 'Cancelar' }).click();
    await enviar(PFX_CPF, 'teste');
    await pagina.getByRole('tab', { name: 'Emitir nota', selected: true }).waitFor();
    await print('01-produtor-adicionado');
  });

  test('abre com dados do certificado e faixa de SEFAZ simulada', async () => {
    await pagina.getByText('SEFAZ SIMULADA — modo de teste').waitFor();
    await pagina.getByText('SEFAZ em operação').waitFor();
    assert.match(await pagina.locator('.produtor-atual').innerText(), /PRODUTOR TESTE[\s\S]*e-CPF 529\.982\.247-25/);
    assert.equal(await pagina.getByRole('button', { name: 'Emitir nota' }).isDisabled(), true);
    assert.match(await pagina.locator('.pendencias summary').innerText(), /\d+ pendências/);
  });

  test('cadastra, edita e exclui produtos na aba Produtos', async () => {
    await pagina.getByRole('tab', { name: 'Produtos' }).click();
    await pagina.getByText('Nenhum produto cadastrado').waitFor();

    const cadastrar = async (campos) => {
      await pagina.getByRole('button', { name: '+ Novo produto' }).click();
      const janela = pagina.getByRole('dialog');
      for (const [rotulo, valor] of Object.entries(campos)) await janela.getByLabel(rotulo).fill(valor);
      await janela.getByRole('button', { name: 'Cadastrar produto' }).click();
      await janela.waitFor({ state: 'detached' });
    };
    await cadastrar({ Código: 'SOJA01', Descrição: 'soja em graos', NCM: '1201.90.00', 'CFOP padrão': '5101', Unidade: 'SC', 'Preço unitário': '132,50' });
    await cadastrar({ Código: 'MILHO01', Descrição: 'MILHO', NCM: '10059010', 'CFOP padrão': '5101', Unidade: 'SC' });

    // Código repetido é recusado pelo backend
    await pagina.getByRole('button', { name: '+ Novo produto' }).click();
    const dup = pagina.getByRole('dialog');
    for (const [r, v] of [['Código', 'soja01'], ['Descrição', 'X'], ['NCM', '12019000'], ['CFOP padrão', '5101']]) await dup.getByLabel(r).fill(v);
    await dup.getByRole('button', { name: 'Cadastrar produto' }).click();
    await dup.getByText(/Já existe um produto com o código SOJA01/i).waitFor();
    await dup.getByRole('button', { name: 'Cancelar' }).click();

    const linhas = pagina.locator('.tabela-notas tbody tr');
    assert.equal(await linhas.count(), 2);
    assert.match(await linhas.nth(1).innerText(), /SOJA01\s+SOJA EM GRAOS\s+1201\.90\.00/); // ordem alfabética, descrição em maiúsculas
    await print('0-produtos');

    // Edição: muda o preço do milho
    await linhas.nth(0).click();
    await pagina.getByRole('dialog').getByLabel('Preço unitário').fill('58,40');
    await pagina.getByRole('dialog').getByRole('button', { name: 'Salvar alterações' }).click();
    await pagina.getByText('Produto MILHO alterado.').waitFor();
    assert.match(await linhas.nth(0).innerText(), /R\$\s*58,40/);

    // Busca e exclusão
    await pagina.getByLabel('Buscar produto').fill('soja');
    assert.equal(await linhas.count(), 1);
    await pagina.getByLabel('Buscar produto').fill('');
    await pagina.getByRole('button', { name: 'Excluir MILHO' }).click();
    await pagina.getByText('Produto MILHO excluído.').waitFor();
    assert.equal(await linhas.count(), 1);
  });

  test('preenche a nota com produto do cadastro, busca CEP e valida (assinatura + XSD)', async () => {
    await pagina.getByRole('tab', { name: 'Emitir nota' }).click();
    assert.equal(await pagina.getByRole('button', { name: /exemplo/i }).count(), 0);
    const itens = pagina.locator('#itens');
    await itens.getByLabel('Produto cadastrado').selectOption({ label: 'SOJA EM GRAOS (SOJA01)' });
    assert.equal(await itens.locator('.item').count(), 1); // substituiu o item em branco
    assert.equal(await itens.getByLabel('Descrição').inputValue(), 'SOJA EM GRAOS');
    assert.equal(await itens.getByLabel('Valor unitário').inputValue(), '132,50');
    await itens.getByLabel('Quantidade').fill('500');

    const emitente = pagina.locator('#emitente');
    await emitente.getByLabel('Inscrição estadual').fill('109876547');
    // CEP do emitente é obrigatório: sem ele a nota não fica completa
    assert.match(await pagina.locator('.pendencias').textContent(), /Emitente: CEP/); // lista recolhida: textContent
    await emitente.getByLabel('CEP').fill('74000-000');
    await emitente.getByLabel('Logradouro').fill('RODOVIA GO-060 KM 10');
    await emitente.getByLabel('Bairro').fill('ZONA RURAL');
    await emitente.getByLabel('Município').fill('GOIANIA');
    await emitente.getByLabel('Cód. IBGE').fill('5208707');

    // Busca de CEP real (ViaCEP) no destinatário. Se a internet falhar, o teste avisa e segue.
    const dest = pagina.locator('#destinatario');
    await dest.getByLabel('CPF').fill('123.456.789-09');
    await dest.getByLabel('Nome / Razão social').fill('COMPRADOR TESTE');
    await dest.getByLabel('Número').fill('100');
    await dest.getByLabel('CEP').fill('74003-010');
    await dest.getByRole('button', { name: 'Buscar' }).click();
    try {
      await pagina.waitForFunction(() => document.querySelectorAll('#destinatario input')[0] && [...document.querySelectorAll('#destinatario input')].some((i) => i.value === '5208707'), null, { timeout: 8000 });
    } catch {
      console.warn('  ! ViaCEP não respondeu; preenchendo o endereço à mão');
      await dest.getByLabel('Logradouro').fill('AVENIDA TESTE');
      await dest.getByLabel('Bairro').fill('CENTRO');
      await dest.getByLabel('Município').fill('GOIANIA');
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
    // dentro do navegador logado o PDF vem; sem a sessão (fora do navegador), não
    const tipo = await pagina.evaluate((u) => fetch(u).then((r) => r.headers.get('content-type')), href);
    assert.equal(tipo, 'application/pdf');
    assert.equal((await fetch(URL_APP + href)).status, 401);
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
    await pagina.locator('#itens').getByLabel('Produto cadastrado').selectOption({ label: 'SOJA EM GRAOS (SOJA01)' });
    const ncm = pagina.locator('#itens').getByLabel('NCM');
    await ncm.fill('1201');
    await ncm.blur();
    await pagina.locator('#itens').getByText('8 dígitos').waitFor();
    await pagina.locator('.pendencias').getByText('Item 1: NCM com 8 dígitos').waitFor({ state: 'attached' });
    await print('5-erro-ncm');
  });

  test('item da nota pode ser salvo no cadastro de produtos', async () => {
    const itens = pagina.locator('#itens');
    await itens.getByRole('button', { name: '+ Item em branco' }).click();
    const item = itens.locator('.item').nth(1);
    await item.getByLabel('Código').fill('FEIJAO01');
    await item.getByLabel('Descrição').fill('FEIJAO CARIOCA');
    await item.getByLabel('NCM').fill('07133399');
    await item.getByRole('button', { name: 'Salvar no cadastro de produtos' }).click();
    await item.getByText('Produto FEIJAO01 cadastrado.').waitFor();
    await itens.getByLabel('Produto cadastrado').locator('option', { hasText: 'FEIJAO CARIOCA (FEIJAO01)' }).waitFor({ state: 'attached' });
  });

  test('cadastra cliente, usa na nota e salva destinatário digitado', async () => {
    await pagina.getByRole('tab', { name: 'Clientes' }).click();
    await pagina.getByText('Nenhum cliente cadastrado').first().waitFor();
    await pagina.getByRole('button', { name: '+ Novo cliente' }).click();
    const janela = pagina.getByRole('dialog');
    await janela.getByRole('button', { name: 'Cadastrar cliente' }).waitFor();
    assert.equal(await janela.getByRole('button', { name: 'Cadastrar cliente' }).isDisabled(), true);
    await janela.getByLabel('CPF').fill('390.533.447-05');
    await janela.getByLabel('Nome / Razão social').fill('joao comprador');
    await janela.getByLabel('Logradouro').fill('RUA DAS FLORES');
    await janela.getByLabel('Bairro').fill('CENTRO');
    await janela.getByLabel('Município').fill('GOIANIA');
    await janela.getByLabel('Cód. IBGE').fill('5208707');
    await janela.getByRole('button', { name: 'Cadastrar cliente' }).click();
    await pagina.getByText('Cliente JOAO COMPRADOR cadastrado.').waitFor();
    assert.match(await pagina.locator('.tabela-notas').innerText(), /JOAO COMPRADOR\s+390\.533\.447-05\s+GOIANIA \/ GO/);
    await print('09-clientes');

    await pagina.getByRole('tab', { name: 'Emitir nota' }).click();
    const dest = pagina.locator('#destinatario');
    await dest.getByLabel('Cliente cadastrado').selectOption({ label: 'JOAO COMPRADOR (390.533.447-05)' });
    assert.equal(await dest.getByLabel('Nome / Razão social').inputValue(), 'JOAO COMPRADOR');
    assert.equal(await dest.getByLabel('CPF').inputValue(), '390.533.447-05');
    assert.equal(await dest.getByLabel('Logradouro').inputValue(), 'RUA DAS FLORES');

    // destinatário digitado vai para o cadastro; documento repetido pergunta e atualiza
    await dest.getByLabel('CPF').fill('529.982.247-25');
    await dest.getByLabel('Nome / Razão social').fill('MARIA COMPRADORA');
    await dest.getByRole('button', { name: 'Salvar no cadastro de clientes' }).click();
    await dest.getByText('Cliente MARIA COMPRADORA cadastrado.').waitFor();
    await dest.getByLabel('Cliente cadastrado').locator('option', { hasText: 'MARIA COMPRADORA' }).waitFor({ state: 'attached' });
  });

  test('filtra as notas por situação e por busca', async () => {
    await pagina.getByRole('tab', { name: 'Notas emitidas' }).click();
    const linhas = pagina.locator('.tabela-notas tbody tr');
    await linhas.first().waitFor();
    await pagina.getByLabel('Situação').selectOption({ label: 'Rejeitada' });
    await pagina.getByText('Nenhuma nota encontrada').waitFor();
    await pagina.getByLabel('Situação').selectOption({ label: 'Cancelada' });
    await linhas.first().getByText('Cancelada').waitFor();
    await pagina.getByRole('button', { name: 'Limpar filtros' }).click();
    await pagina.getByLabel('Buscar').fill('nome que nao existe');
    await pagina.getByText('Nenhuma nota encontrada').waitFor();
    await pagina.getByLabel('Buscar').fill('comprador');
    await linhas.first().waitFor();
    assert.equal(await linhas.count(), 1);
    await print('10-filtros');
    await pagina.getByRole('button', { name: 'Limpar filtros' }).click();
  });

  test('duplica uma nota: abre com os dados dela e o próximo número', async () => {
    await pagina.locator('.tabela-notas tbody tr').first().click();
    await pagina.getByRole('dialog').getByRole('button', { name: 'Duplicar nota' }).click();
    await pagina.getByRole('heading', { name: /Nota fiscal nº 2\b/ }).waitFor();
    assert.equal(await pagina.getByRole('tab', { name: 'Emitir nota', selected: true }).count(), 1);
    assert.equal(await pagina.locator('#destinatario').getByLabel('Nome / Razão social').inputValue(), 'COMPRADOR TESTE');
    const itens = pagina.locator('#itens');
    assert.equal(await itens.getByLabel('Descrição').inputValue(), 'SOJA EM GRAOS');
    assert.equal(await itens.getByLabel('Quantidade').inputValue(), '500');
    await pagina.getByText('Todos os campos obrigatórios preenchidos').waitFor();
    await pagina.getByRole('button', { name: 'Emitir nota' }).click();
    await pagina.locator('.resultado').getByText('Nota autorizada').waitFor({ timeout: 15000 });
  });

  test('tipo de nota: devolução pede a nota referenciada e passa no schema', async () => {
    const chave = await pagina.evaluate(() => fetch('/api/notas').then((r) => r.json()).then((d) => d.notas[0].chave));
    await pagina.getByRole('button', { name: 'Nova nota' }).click();
    await pagina.getByRole('heading', { name: /Nota fiscal nº 3\b/ }).waitFor();
    const ident = pagina.locator('#identificacao');
    assert.equal(await ident.getByLabel('Natureza da operação').inputValue(), 'VENDA DE PRODUCAO DO ESTABELECIMENTO');
    assert.equal(await ident.getByLabel('Chave da NF-e referenciada').count(), 0);

    const itens = pagina.locator('#itens');
    await itens.getByLabel('Produto cadastrado').selectOption({ label: 'SOJA EM GRAOS (SOJA01)' });
    await ident.getByLabel('Tipo de nota').selectOption({ label: 'Devolução de compra (saída)' });
    assert.equal(await ident.getByLabel('Natureza da operação').inputValue(), 'DEVOLUCAO DE COMPRA');
    assert.equal(await itens.getByLabel('CFOP').inputValue(), '5201');
    assert.equal(await pagina.locator('#pagamento').getByLabel('Forma de pagamento').inputValue(), '90');
    await pagina.locator('.pendencias').getByText('Chave da NF-e referenciada').waitFor({ state: 'attached' });
    await ident.getByLabel('Chave da NF-e referenciada').fill(chave);
    await print('11-devolucao');

    await pagina.locator('#destinatario').getByLabel('Cliente cadastrado').selectOption({ label: 'JOAO COMPRADOR (390.533.447-05)' });
    await pagina.getByText('Todos os campos obrigatórios preenchidos').waitFor();
    await pagina.getByRole('button', { name: 'Validar' }).click();
    await pagina.locator('.resultado').getByText('Pronta para emitir').waitFor();
    assert.match(await pagina.locator('.resultado').innerText(), /Schema \(XSD\)\s+Estrutura válida/);

    // produto escolhido depois segue o CFOP da operação, não o padrão do cadastro
    await itens.getByLabel('Produto cadastrado').selectOption({ label: 'SOJA EM GRAOS (SOJA01)' });
    assert.equal(await itens.locator('.item.aberto').getByLabel('CFOP').inputValue(), '5201');
    await pagina.getByRole('button', { name: 'Nova nota' }).click();
    await ident.getByLabel('Chave da NF-e referenciada').waitFor({ state: 'detached' });
  });

  test('exporta o ZIP para o contador e avisa quando não há nada', async () => {
    await pagina.getByRole('tab', { name: 'Notas emitidas' }).click();
    await pagina.getByRole('button', { name: 'Exportar para o contador' }).click();
    const janela = pagina.getByRole('dialog');
    await janela.getByRole('button', { name: 'Mês passado' }).click();
    await janela.getByRole('button', { name: 'Baixar ZIP' }).click();
    await janela.getByText('Nenhum documento com valor fiscal nesse período.').waitFor();
    await janela.getByRole('button', { name: 'Este mês' }).click();
    const [resposta] = await Promise.all([
      pagina.waitForResponse((r) => r.url().includes('/api/notas/exportar')),
      janela.getByRole('button', { name: 'Baixar ZIP' }).click(),
    ]);
    assert.equal(resposta.status(), 200);
    assert.equal(resposta.headers()['content-type'], 'application/zip');
    await janela.getByText('Arquivo gerado.').waitFor();
    await print('12-exportar');
    await janela.getByRole('button', { name: 'Fechar' }).first().click();
  });

  test('painel mostra os números do mês e abre a lista filtrada', async () => {
    await pagina.getByRole('tab', { name: 'Painel' }).click();
    const indicadores = pagina.locator('.painel-indicadores');
    await indicadores.waitFor();
    const texto = await indicadores.innerText();
    assert.match(texto, /Total autorizado no mês\s+R\$\s*66\.250,00/);
    assert.match(texto, /Canceladas\s+1/);
    assert.equal(await pagina.locator('.barra').count(), 1);
    await pagina.getByText('Nenhum certificado vence nos próximos 30 dias.').waitFor();
    await pagina.locator('.coluna-barra').last().hover();
    await print('13-painel');

    await indicadores.getByRole('button', { name: /Canceladas/ }).click();
    await pagina.getByRole('tab', { name: 'Notas emitidas', selected: true }).waitFor();
    assert.equal(await pagina.getByLabel('Situação').inputValue(), 'cancelada');
    await pagina.locator('.tabela-notas tbody tr').first().getByText('Cancelada').waitFor();
    assert.equal(await pagina.locator('.tabela-notas tbody tr').count(), 1);
  });

  test('troca de produtor separa notas, produtos e numeração', async () => {
    await pagina.locator('.produtor-atual').click(); // atalho do topo para a tela de produtores
    await pagina.getByRole('button', { name: '+ Adicionar produtor' }).click();
    const janela = pagina.getByRole('dialog');
    await janela.getByLabel('Arquivo do certificado').setInputFiles(PFX_CNPJ);
    await janela.getByLabel('Senha do certificado').fill('teste');
    await janela.getByRole('button', { name: 'Adicionar e usar' }).click();
    await pagina.getByRole('heading', { name: /Nota fiscal nº 1\b/ }).waitFor(); // numeração própria do novo produtor
    assert.match(await pagina.locator('.produtor-atual').innerText(), /EMPRESA TESTE[\s\S]*12\.345\.678\/0001-95/);
    assert.equal(await pagina.locator('#itens').getByLabel('Produto cadastrado').isDisabled(), true); // produtos são do outro produtor

    await pagina.getByRole('tab', { name: 'Notas emitidas' }).click();
    await pagina.getByText('Nenhuma nota ainda').waitFor();
    assert.equal(await pagina.getByRole('button', { name: 'Inutilizar numeração' }).isDisabled(), false); // CNPJ pode inutilizar

    // volta para o produtor CPF: as notas dele continuam lá
    await pagina.getByRole('tab', { name: 'Produtores' }).click();
    const cartaoCpf = pagina.locator('.cartao-produtor[data-documento="52998224725"]');
    await cartaoCpf.getByRole('button', { name: 'Usar este produtor' }).click();
    await pagina.getByRole('tab', { name: 'Notas emitidas' }).click();
    await pagina.locator('.tabela-notas tbody tr').first().getByText('Autorizada').waitFor();
    await pagina.getByRole('tab', { name: 'Produtores' }).click();
    await cartaoCpf.getByText('Em uso').waitFor();
    await print('06-produtores');
  });

  test('administrador cria usuário; o usuário não vê os produtores do admin nem a administração', async () => {
    await pagina.getByRole('tab', { name: 'Administração' }).click();
    await pagina.getByRole('button', { name: '+ Novo usuário' }).click();
    const janela = pagina.getByRole('dialog');
    await janela.getByLabel('Nome').fill('Maria Operadora');
    await janela.getByLabel('Usuário (login)').fill('maria');
    await janela.getByLabel('Senha inicial').fill('senha-da-maria');
    await janela.getByRole('button', { name: 'Criar usuário' }).click();
    await pagina.getByText('Usuário maria criado.').waitFor();
    assert.match(await pagina.locator('.tabela-notas').innerText(), /Maria Operadora\s+maria\s+Usuário\s+0/);
    await print('07-administracao');

    // sair e entrar como Maria
    await pagina.locator('.botao-usuario').click();
    await pagina.getByRole('menuitem', { name: 'Sair' }).click();
    await pagina.getByRole('heading', { name: 'Entrar' }).waitFor();
    await pagina.getByLabel('Usuário').fill('maria');
    await pagina.getByLabel('Senha').fill('senha-errada');
    await pagina.getByRole('button', { name: 'Entrar' }).click();
    await pagina.getByText('Usuário ou senha incorretos.').waitFor();
    await pagina.getByLabel('Senha').fill('senha-da-maria');
    await pagina.getByRole('button', { name: 'Entrar' }).click();
    await pagina.getByRole('heading', { name: 'Escolha o produtor' }).waitFor();
    await pagina.getByText('Nenhum produtor cadastrado').waitFor();
    assert.equal(await pagina.getByRole('tab', { name: 'Administração' }).count(), 0);
    await print('08-usuario-comum');

    // a API também barra (não é só a tela)
    const r = await pagina.evaluate(() => fetch('/api/admin/usuarios').then((x) => x.status));
    assert.equal(r, 403);
  });

  test('sair encerra a sessão no servidor', async () => {
    await pagina.locator('.botao-usuario').click();
    await pagina.getByRole('menuitem', { name: 'Sair' }).click();
    await pagina.getByRole('heading', { name: 'Entrar' }).waitFor();
    const r = await pagina.evaluate(() => fetch('/api/produtores').then((x) => x.status));
    assert.equal(r, 401);
    // o administrador entra de novo e encontra o produtor que estava usando
    await pagina.getByLabel('Usuário').fill('rodrigo');
    await pagina.getByLabel('Senha').fill('senha-do-admin');
    await pagina.getByRole('button', { name: 'Entrar' }).click();
    await pagina.getByRole('heading', { name: 'Escolha o produtor' }).waitFor(); // sessão nova começa sem produtor
    assert.equal(await pagina.locator('.cartao-produtor').count(), 2);
  });

  test('nenhum erro de JavaScript na página', () => {
    assert.deepEqual(errosDaPagina, []);
  });
});
