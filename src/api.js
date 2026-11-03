async function chamar(url, opcoes) {
  const res = await fetch(url, opcoes);
  const corpo = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(corpo.erro || `Erro HTTP ${res.status}`);
  return corpo;
}

const post = (url, dados) => chamar(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(dados ?? {}) });

// PDF gerado por POST: abre em nova aba via blob.
async function abrirPdfPost(url, dados) {
  const janela = window.open('', '_blank'); // abre já no clique (evita bloqueio de pop-up)
  const res = await fetch(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(dados) });
  if (!res.ok) {
    janela?.close();
    const corpo = await res.json().catch(() => ({}));
    throw new Error(corpo.erro || `Erro HTTP ${res.status}`);
  }
  const blob = await res.blob();
  const link = URL.createObjectURL(blob);
  if (janela) janela.location.href = link;
  else window.location.href = link;
}

export const api = {
  emissor: () => chamar('/api/emissor'),
  status: () => chamar('/api/sefaz/status'),
  proximoNumero: (serie) => chamar(`/api/numeracao/${encodeURIComponent(serie)}`),
  validar: (dados) => post('/api/nfe/validar', dados),
  emitir: (dados) => post('/api/nfe/emitir', dados),
  previaDanfe: (dados) => abrirPdfPost('/api/nfe/danfe-previa', dados),

  notas: (pagina = 1) => chamar(`/api/notas?pagina=${pagina}`),
  nota: (id) => chamar(`/api/notas/${id}`),
  cancelar: (id, xJust) => post(`/api/notas/${id}/cancelar`, { xJust }),
  cce: (id, xCorrecao) => post(`/api/notas/${id}/cce`, { xCorrecao }),
  consultar: (id) => chamar(`/api/notas/${id}/consultar`),
  urlDanfe: (id) => `/api/notas/${id}/danfe`,
  urlXml: (id, tipo) => `/api/notas/${id}/xml${tipo ? `?tipo=${tipo}` : ''}`,
  urlEventoXml: (id) => `/api/eventos/${id}/xml`,

  inutilizacoes: () => chamar('/api/inutilizacoes'),
  inutilizar: (dados) => post('/api/inutilizacoes', dados),
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
