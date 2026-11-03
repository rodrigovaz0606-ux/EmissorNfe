async function chamar(url, opcoes) {
  const res = await fetch(url, opcoes);
  const corpo = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(corpo.erro || `Erro HTTP ${res.status}`);
  return corpo;
}

const post = (url, dados) => chamar(url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(dados) });

export const api = {
  emissor: () => chamar('/api/emissor'),
  status: () => chamar('/api/sefaz/status'),
  validar: (dados) => post('/api/nfe/validar', dados),
  emitir: (dados) => post('/api/nfe/emitir', dados),
  urlArquivo: (nome) => `/api/arquivos/${encodeURIComponent(nome)}`,
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
