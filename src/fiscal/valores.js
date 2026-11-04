// Dinheiro em centavos inteiros, com o mesmo arredondamento do backend (Rust),
// para os totais da tela baterem com os do XML.

export function paraCentavos(valor) {
  if (valor === undefined || valor === null || valor === '') return 0;
  const n = typeof valor === 'number' ? valor : Number(String(valor).replace(',', '.'));
  if (!Number.isFinite(n)) throw new Error(`Valor inválido: "${valor}"`);
  return Math.round(n * 100 + (n >= 0 ? 1e-7 : -1e-7));
}

export function centavos(c) {
  const sinal = c < 0 ? '-' : '';
  const abs = Math.abs(c);
  return `${sinal}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, '0')}`;
}

// vProd = quantidade × unitário, arredondado em centavos (+1e-7 compensa o ponto flutuante).
export function multiplicarEmCentavos(qtd, unit) {
  return Math.round(Number(qtd) * Number(unit) * 100 + 1e-7);
}
