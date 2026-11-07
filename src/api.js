// Cliente da API. A sessão é um cookie HttpOnly (o JavaScript não tem acesso ao token).
// 401 → a sessão caiu: avisa o App para mostrar o login.
// 409 → nenhum produtor selecionado: avisa o App para mostrar a escolha de produtor.
export const EVENTO_SESSAO = 'emissor:sessao';

function avisarSessao(status) {
  if (status === 401 || status === 409) window.dispatchEvent(new CustomEvent(EVENTO_SESSAO, { detail: { status } }));
}

async function chamar(url, opcoes = {}, { silencioso = false } = {}) {
  const res = await fetch(url, { credentials: 'same-origin', ...opcoes });
  const corpo = res.status === 204 ? {} : await res.json().catch(() => ({}));
  if (!res.ok) {
    if (!silencioso) avisarSessao(res.status);
    const e = new Error(corpo.erro || `Erro HTTP ${res.status}`);
    e.status = res.status;
    throw e;
  }
  return corpo;
}

const comCorpo = (metodo) => (url, dados, extra) =>
  chamar(url, { method: metodo, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(dados ?? {}) }, extra);
const post = comCorpo('POST');
const put = comCorpo('PUT');
const apagar = (url) => chamar(url, { method: 'DELETE' });

// PDF gerado por POST: abre em nova aba via blob.
async function abrirPdfPost(url, dados) {
  const janela = window.open('', '_blank'); // abre já no clique (evita bloqueio de pop-up)
  const res = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(dados) });
  if (!res.ok) {
    janela?.close();
    avisarSessao(res.status);
    const corpo = await res.json().catch(() => ({}));
    throw new Error(corpo.erro || `Erro HTTP ${res.status}`);
  }
  const blob = await res.blob();
  const link = URL.createObjectURL(blob);
  if (janela) janela.location.href = link;
  else window.location.href = link;
}

// Arquivo escolhido no <input type=file> → base64 (para enviar o .pfx em JSON).
export async function arquivoEmBase64(arquivo) {
  const bytes = new Uint8Array(await arquivo.arrayBuffer());
  let binario = '';
  for (let i = 0; i < bytes.length; i += 0x8000) binario += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binario);
}

export const api = {
  // login (respostas de erro aqui não disparam o aviso de sessão)
  estado: () => chamar('/api/auth/estado', {}, { silencioso: true }),
  configurar: (dados) => post('/api/auth/configurar', dados, { silencioso: true }),
  entrar: (login, senha) => post('/api/auth/entrar', { login, senha }, { silencioso: true }),
  sair: () => post('/api/auth/sair', {}, { silencioso: true }),
  trocarSenha: (atual, nova) => post('/api/auth/senha', { atual, nova }),

  // produtores
  produtores: () => chamar('/api/produtores'),
  adicionarProdutor: (pfx, senha) => post('/api/produtores', { pfx, senha }),
  renovarCertificado: (id, pfx, senha) => put(`/api/produtores/${id}/certificado`, { pfx, senha }),
  definirEmitente: (id, emitente) => put(`/api/produtores/${id}/emitente`, emitente),
  removerProdutor: (id) => apagar(`/api/produtores/${id}`),
  selecionarProdutor: (id) => post(`/api/produtores/${id}/selecionar`, {}),

  // administração
  usuarios: () => chamar('/api/admin/usuarios'),
  criarUsuario: (dados) => post('/api/admin/usuarios', dados),
  alterarUsuario: (id, dados) => put(`/api/admin/usuarios/${id}`, dados),

  // emissão (produtor selecionado)
  emissor: () => chamar('/api/emissor'),
  status: () => chamar('/api/sefaz/status'),
  proximoNumero: (serie) => chamar(`/api/numeracao/${encodeURIComponent(serie)}`),
  validar: (dados) => post('/api/nfe/validar', dados),
  emitir: (dados) => post('/api/nfe/emitir', dados),
  previaDanfe: (dados) => abrirPdfPost('/api/nfe/danfe-previa', dados),

  painel: () => chamar('/api/painel'),
  notas: (pagina = 1, filtros = {}) => {
    const q = new URLSearchParams({ pagina: String(pagina) });
    for (const [k, v] of Object.entries(filtros)) if (v) q.set(k, v);
    return chamar(`/api/notas?${q}`);
  },
  exportarNotas: async (de, ate) => {
    const res = await fetch(`/api/notas/exportar?${new URLSearchParams({ de, ate })}`);
    if (!res.ok) {
      avisarSessao(res.status);
      const corpo = await res.json().catch(() => ({}));
      throw new Error(corpo.erro || `Erro HTTP ${res.status}`);
    }
    const link = document.createElement('a');
    link.href = URL.createObjectURL(await res.blob());
    link.download = `notas-${de}-a-${ate}.zip`;
    link.click();
    URL.revokeObjectURL(link.href);
  },
  nota: (id) => chamar(`/api/notas/${id}`),
  cancelar: (id, xJust) => post(`/api/notas/${id}/cancelar`, { xJust }),
  cce: (id, xCorrecao) => post(`/api/notas/${id}/cce`, { xCorrecao }),
  consultar: (id) => chamar(`/api/notas/${id}/consultar`),
  urlDanfe: (id) => `/api/notas/${id}/danfe`,
  urlXml: (id, tipo) => `/api/notas/${id}/xml${tipo ? `?tipo=${tipo}` : ''}`,
  urlEventoXml: (id) => `/api/eventos/${id}/xml`,

  inutilizacoes: () => chamar('/api/inutilizacoes'),
  inutilizar: (dados) => post('/api/inutilizacoes', dados),

  produtos: () => chamar('/api/produtos'),
  criarProduto: (dados) => post('/api/produtos', dados),
  alterarProduto: (id, dados) => put(`/api/produtos/${id}`, dados),
  excluirProduto: (id) => apagar(`/api/produtos/${id}`),
  estoqueProduto: (id) => chamar(`/api/produtos/${id}/estoque`),
  movimentarEstoque: (id, dados) => post(`/api/produtos/${id}/estoque`, dados),

  clientes: () => chamar('/api/clientes'),
  criarCliente: (dados) => post('/api/clientes', dados),
  alterarCliente: (id, dados) => put(`/api/clientes/${id}`, dados),
  excluirCliente: (id) => apagar(`/api/clientes/${id}`),
};

// Consulta de CEP no ViaCEP (preenche endereço e código IBGE do município).
export async function buscarCep(cep) {
  const d = String(cep).replace(/\D/g, '');
  if (d.length !== 8) throw new Error('CEP deve ter 8 dígitos');
  const r = await fetch(`https://viacep.com.br/ws/${d}/json/`);
  const j = await r.json();
  if (j.erro) throw new Error('CEP não encontrado');
  return { xLgr: j.logradouro, xBairro: j.bairro, xMun: j.localidade, UF: j.uf, cMun: j.ibge };
}
