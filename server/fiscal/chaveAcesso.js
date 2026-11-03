// Chave de acesso da NF-e (44 dígitos), conforme o Manual de Orientação do Contribuinte:
//
//   cUF(2) AAMM(4) CNPJ/CPF(14) mod(2) serie(3) nNF(9) tpEmis(1) cNF(8) cDV(1)
//
// Para emitente pessoa física, o CPF ocupa as 14 posições completado com zeros à esquerda.

import crypto from 'node:crypto';

const soDigitos = (v) => String(v ?? '').replace(/\D/g, '');

function campo(nome, valor, tamanho) {
  const d = soDigitos(valor);
  if (!d || d.length > tamanho) throw new Error(`${nome} inválido: "${valor}" (até ${tamanho} dígitos)`);
  return d.padStart(tamanho, '0');
}

// Módulo 11 com pesos 2..9 aplicados da direita para a esquerda.
// Resto 0 ou 1 → DV 0; caso contrário DV = 11 - resto.
export function calcularDV(chave43) {
  if (!/^\d{43}$/.test(chave43)) throw new Error('A base da chave precisa ter 43 dígitos.');
  let soma = 0;
  let peso = 2;
  for (let i = chave43.length - 1; i >= 0; i--) {
    soma += Number(chave43[i]) * peso;
    peso = peso === 9 ? 2 : peso + 1;
  }
  const resto = soma % 11;
  return resto < 2 ? 0 : 11 - resto;
}

// Código numérico aleatório de 8 dígitos. A SEFAZ rejeita cNF igual ao nNF
// e sequências óbvias, então sorteamos até obter um valor aceitável.
export function gerarCNF(nNF) {
  const proibidos = new Set(['00000000', '12345678', '23456789', '34567890', '45678901', '56789012', '67890123', '78901234', '89012345', '90123456', '01234567']);
  for (let d = 0; d <= 9; d++) proibidos.add(String(d).repeat(8));
  const nnf8 = soDigitos(nNF).padStart(8, '0').slice(-8);
  for (;;) {
    const cNF = String(crypto.randomInt(0, 100_000_000)).padStart(8, '0');
    if (!proibidos.has(cNF) && cNF !== nnf8) return cNF;
  }
}

// AAMM da emissão. Para texto ISO (com fuso, como vai no XML), usa o ano/mês
// escritos, sem converter de fuso, para a chave bater com o dhEmi do XML.
function anoMes(dhEmi) {
  if (typeof dhEmi === 'string') {
    const m = /^(\d{4})-(\d{2})/.exec(dhEmi);
    if (!m) throw new Error(`dhEmi inválida: "${dhEmi}"`);
    return m[1].slice(-2) + m[2];
  }
  if (!(dhEmi instanceof Date) || Number.isNaN(dhEmi.getTime())) throw new Error(`dhEmi inválida: "${dhEmi}"`);
  return String(dhEmi.getFullYear()).slice(-2) + String(dhEmi.getMonth() + 1).padStart(2, '0');
}

/**
 * @param {object} p
 * @param {number|string} p.cUF      código IBGE da UF (GO = 52)
 * @param {Date|string}   p.dhEmi    data de emissão (usa ano e mês)
 * @param {string}        p.documento CNPJ (14) ou CPF (11) do emitente
 * @param {number|string} [p.mod=55]
 * @param {number|string} p.serie
 * @param {number|string} p.nNF
 * @param {number|string} [p.tpEmis=1] 1 = emissão normal
 * @param {string}        [p.cNF]    se omitido, é sorteado
 */
export function montarChave({ cUF, dhEmi, documento, mod = 55, serie, nNF, tpEmis = 1, cNF }) {
  const aamm = anoMes(dhEmi);

  const doc = soDigitos(documento);
  if (doc.length !== 11 && doc.length !== 14) throw new Error(`Documento do emitente deve ser CPF ou CNPJ: "${documento}"`);

  const n = Number(soDigitos(nNF));
  if (!(n >= 1 && n <= 999_999_999)) throw new Error(`nNF fora da faixa 1–999999999: "${nNF}"`);

  const codigo = cNF ? campo('cNF', cNF, 8) : gerarCNF(nNF);

  const base =
    campo('cUF', cUF, 2) +
    aamm +
    doc.padStart(14, '0') +
    campo('mod', mod, 2) +
    campo('serie', serie, 3) +
    campo('nNF', nNF, 9) +
    campo('tpEmis', tpEmis, 1) +
    codigo;

  const cDV = calcularDV(base);
  return { chave: base + cDV, cNF: codigo, cDV };
}

export function validarChave(chave) {
  const c = soDigitos(chave);
  return c.length === 44 && calcularDV(c.slice(0, 43)) === Number(c[43]);
}

// Separa os campos de uma chave (útil para conferência e para o DANFE).
export function decomporChave(chave) {
  const c = soDigitos(chave);
  if (c.length !== 44) throw new Error('Chave precisa ter 44 dígitos.');
  return {
    cUF: c.slice(0, 2),
    aamm: c.slice(2, 6),
    documento: c.slice(6, 20),
    mod: c.slice(20, 22),
    serie: c.slice(22, 25),
    nNF: c.slice(25, 34),
    tpEmis: c.slice(34, 35),
    cNF: c.slice(35, 43),
    cDV: c.slice(43),
    valida: validarChave(c),
  };
}
