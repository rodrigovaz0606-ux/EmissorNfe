// Formatação de valores no padrão do leiaute da NF-e.

// Texto: a SEFAZ rejeita espaços no início/fim, espaços duplos e quebras de linha.
export function texto(valor) {
  if (valor === undefined || valor === null) return undefined;
  return String(valor).replace(/[\r\n\t]+/g, ' ').replace(/\s{2,}/g, ' ').trim();
}

export function escaparXml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export const soDigitos = (v) => (v === undefined || v === null ? undefined : String(v).replace(/\D/g, ''));

// --- dinheiro em centavos inteiros (evita 0.1 + 0.2 = 0.30000000000000004) ---

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

// Decimal com número fixo de casas (quantidades: 4; alíquotas: 2 ou 4).
export function decimal(valor, casas) {
  const n = Number(valor);
  if (!Number.isFinite(n)) throw new Error(`Número inválido: "${valor}"`);
  return n.toFixed(casas);
}

// Valor unitário: até 10 casas, sem zeros sobrando, mas com pelo menos 2.
export function unitario(valor) {
  const s = Number(valor).toFixed(10).replace(/0+$/, '');
  const [int, dec = ''] = s.split('.');
  return `${int}.${dec.padEnd(2, '0')}`;
}

// vProd = quantidade × unitário, arredondado em centavos.
export function multiplicarEmCentavos(qtd, unit) {
  // +1e-7 compensa erro de ponto flutuante (ex.: 1.005 × 100 = 100.4999…).
  return Math.round(Number(qtd) * Number(unit) * 100 + 1e-7);
}

// Data/hora no formato AAAA-MM-DDThh:mm:ss±hh:mm, num fuso fixo (GO = -03:00).
export function dataHora(data = new Date(), offsetMinutos = -180) {
  const local = new Date(data.getTime() + offsetMinutos * 60_000);
  const p = (n) => String(n).padStart(2, '0');
  const sinal = offsetMinutos < 0 ? '-' : '+';
  const off = Math.abs(offsetMinutos);
  return (
    `${local.getUTCFullYear()}-${p(local.getUTCMonth() + 1)}-${p(local.getUTCDate())}` +
    `T${p(local.getUTCHours())}:${p(local.getUTCMinutes())}:${p(local.getUTCSeconds())}` +
    `${sinal}${p(Math.floor(off / 60))}:${p(off % 60)}`
  );
}
