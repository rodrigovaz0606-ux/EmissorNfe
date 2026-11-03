// Código de barras Code-128, conjunto C (pares de dígitos), usado na chave de acesso do DANFE.

// Larguras (barra, espaço, barra, espaço, barra, espaço) de cada símbolo, em módulos.
export const PADROES = [
  '212222', '222122', '222221', '121223', '121322', '131222', '122213', '122312', '132212', '221213',
  '221312', '231212', '112232', '122132', '122231', '113222', '123122', '123221', '223211', '221132',
  '221231', '213212', '223112', '312131', '311222', '321122', '321221', '312212', '322112', '322211',
  '212123', '212321', '232121', '111323', '131123', '131321', '112313', '132113', '132311', '211313',
  '231113', '231311', '112133', '112331', '132131', '113123', '113321', '133121', '313121', '211331',
  '231131', '213113', '213311', '213131', '311123', '311321', '331121', '312113', '312311', '332111',
  '314111', '221411', '431111', '111224', '111422', '121124', '121421', '141122', '141221', '112214',
  '112412', '122114', '122411', '142112', '142211', '241211', '221114', '413111', '241112', '134111',
  '111242', '121142', '121241', '114212', '124112', '124211', '411212', '421112', '421211', '212141',
  '214121', '412121', '111143', '111341', '131141', '114113', '114311', '411113', '411311', '113141',
  '114131', '311141', '411131', '211412', '211214', '211232', '2331112',
];
const START_C = 105;
const STOP = 106;

/** Valores dos símbolos (start, dados, checksum, stop) para uma string de dígitos de tamanho par. */
export function simbolosC(digitos) {
  if (!/^(\d\d)+$/.test(digitos)) throw new Error('Code-128C exige quantidade par de dígitos.');
  const dados = digitos.match(/\d\d/g).map(Number);
  const soma = dados.reduce((s, v, i) => s + v * (i + 1), START_C);
  return [START_C, ...dados, soma % 103, STOP];
}

/** Lista de barras { x, largura } em módulos, mais a largura total em módulos. */
export function barras(digitos) {
  const lista = [];
  let x = 0;
  for (const s of simbolosC(digitos)) {
    [...PADROES[s]].forEach((w, i) => {
      const largura = Number(w);
      if (i % 2 === 0) lista.push({ x, largura });
      x += largura;
    });
  }
  return { barras: lista, modulos: x };
}

// Decodificador (usado nos testes para conferir a codificação de ponta a ponta).
export function decodificar(lista, modulos) {
  // Reconstrói a sequência de larguras alternando barra/espaço.
  const larguras = [];
  let pos = 0;
  for (const b of lista) {
    if (b.x > pos) larguras.push(b.x - pos);
    else if (larguras.length) throw new Error('barras sobrepostas');
    larguras.push(b.largura);
    pos = b.x + b.largura;
  }
  if (pos !== modulos) throw new Error('largura total inconsistente');
  const simbolos = [];
  for (let i = 0; i < larguras.length;) {
    const tam = larguras.length - i === 7 ? 7 : 6;
    const padrao = larguras.slice(i, i + tam).join('');
    const v = PADROES.indexOf(padrao);
    if (v < 0) throw new Error(`padrão desconhecido: ${padrao}`);
    simbolos.push(v);
    i += tam;
  }
  const [start, ...resto] = simbolos;
  const stop = resto.pop();
  const check = resto.pop();
  if (start !== START_C || stop !== STOP) throw new Error('start/stop inválidos');
  const soma = resto.reduce((s, v, i) => s + v * (i + 1), START_C);
  if (soma % 103 !== check) throw new Error('checksum inválido');
  return resto.map((v) => String(v).padStart(2, '0')).join('');
}
