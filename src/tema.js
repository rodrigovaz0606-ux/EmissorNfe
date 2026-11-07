// Tema claro/escuro. Sem escolha salva, segue o sistema; depois que a pessoa escolhe, vale a escolha.
// O index.html aplica o tema antes do primeiro desenho, para a tela não piscar em branco.
import React from 'react';

const CHAVE = 'emissor.tema';
const ouvintes = new Set();

function salvo() {
  try {
    const v = localStorage.getItem(CHAVE);
    return v === 'escuro' || v === 'claro' ? v : null;
  } catch {
    return null; // navegador sem acesso ao armazenamento: segue o sistema
  }
}

const consultaSistema = window.matchMedia('(prefers-color-scheme: dark)');
const doSistema = () => (consultaSistema.matches ? 'escuro' : 'claro');

let atual = salvo() ?? doSistema();

function aplicar(tema, suave) {
  atual = tema;
  const raiz = document.documentElement;
  if (suave) {
    // As cores só ganham transição durante a troca, para não pesar o resto do tempo.
    raiz.classList.add('tema-trocando');
    setTimeout(() => raiz.classList.remove('tema-trocando'), 320);
  }
  raiz.dataset.tema = tema;
  ouvintes.forEach((f) => f());
}

export function trocarTema() {
  const novo = atual === 'escuro' ? 'claro' : 'escuro';
  try {
    localStorage.setItem(CHAVE, novo);
  } catch { /* a troca vale até fechar a página */ }
  aplicar(novo, true);
}

consultaSistema.addEventListener('change', () => {
  if (!salvo()) aplicar(doSistema(), true);
});
aplicar(atual, false);

const assinar = (f) => {
  ouvintes.add(f);
  return () => ouvintes.delete(f);
};

export const useTema = () => React.useSyncExternalStore(assinar, () => atual);
