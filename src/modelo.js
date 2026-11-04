// Estado do formulário, conversão para o formato do backend, totais e pendências.
// Usa o mesmo arredondamento do backend para que tela e XML calculem igual.
import { cpfValido, cnpjValido } from './fiscal/documentos.js';
import { multiplicarEmCentavos, paraCentavos, centavos } from './fiscal/valores.js';

export { cpfValido, cnpjValido };

// ---------- números digitados ("1.234,56" ou "1234.56") ----------
export function num(v) {
  if (v === undefined || v === null || v === '') return undefined;
  const s = String(v).trim();
  const n = s.includes(',') ? Number(s.replace(/\./g, '').replace(',', '.')) : Number(s);
  return Number.isFinite(n) ? n : NaN;
}
export const digitos = (v) => String(v ?? '').replace(/\D/g, '');

export function moeda(c) {
  return (c / 100).toLocaleString('pt-BR', { style: 'currency', currency: 'BRL' });
}

// ---------- estado inicial ----------
export const enderecoVazio = () => ({ CEP: '', xLgr: '', nro: '', xCpl: '', xBairro: '', cMun: '', xMun: '', UF: 'GO', fone: '' });

export const itemVazio = () => ({
  cProd: '', xProd: '', NCM: '', CFOP: '5101', uCom: 'UN', qCom: '1', vUnCom: '', vDesc: '', vFrete: '', infAdProd: '',
  icms: { orig: '0', CST: '51', pICMS: '' },
  pis: { CST: '08', pPIS: '' },
  cofins: { CST: '08', pCOFINS: '' },
});

export const notaVazia = () => ({
  ide: { natOp: 'VENDA DE PRODUCAO DO ESTABELECIMENTO', serie: '1', nNF: '1', tpNF: '1', finNFe: '1', indFinal: '1', indPres: '1' },
  emit: { CPF: '', CNPJ: '', xNome: '', IE: '', CRT: '3', ender: enderecoVazio() },
  dest: { tipo: 'PF', CPF: '', CNPJ: '', xNome: '', indIEDest: '9', IE: '', email: '', ender: enderecoVazio() },
  itens: [itemVazio()],
  transp: { modFrete: '9' },
  pag: [{ tPag: '01', vPag: '' }],
  infAdic: { infCpl: '' },
});

// ---------- cadastro de produtos ----------
const CAMPOS_PRODUTO = ['cProd', 'xProd', 'NCM', 'CFOP', 'uCom', 'vUnCom'];

export const produtoVazio = () => {
  const { cProd, xProd, NCM, CFOP, uCom, vUnCom, icms, pis, cofins } = itemVazio();
  return { cProd, xProd, NCM, CFOP, uCom, vUnCom, icms, pis, cofins };
};

// Item da nota a partir de um produto cadastrado (quantidade 1; desconto e frete em branco).
export function itemDoProduto(p) {
  const base = itemVazio();
  const item = { ...base, icms: { ...base.icms, ...p.icms }, pis: { ...base.pis, ...p.pis }, cofins: { ...base.cofins, ...p.cofins } };
  for (const c of CAMPOS_PRODUTO) item[c] = p[c] ?? base[c];
  return item;
}

// Produto a partir de um item da nota (para "salvar no cadastro").
export function produtoDoItem(it) {
  const p = { icms: { ...it.icms }, pis: { ...it.pis }, cofins: { ...it.cofins } };
  for (const c of CAMPOS_PRODUTO) p[c] = it[c];
  return p;
}

export const itemEstaVazio = (it) => !it.xProd.trim() && !it.cProd.trim() && !it.vUnCom;

// ---------- totais (mesmas regras de gerarXml.js) ----------
export function totaisItem(it) {
  const q = num(it.qCom);
  const u = num(it.vUnCom);
  const vProd = q > 0 && u >= 0 ? multiplicarEmCentavos(q, u) : 0;
  const seguro = (v) => (Number.isFinite(num(v)) ? paraCentavos(num(v)) : 0);
  const vDesc = seguro(it.vDesc);
  const vFrete = seguro(it.vFrete);
  // Mesmas bases do gerarXml: ICMS = produto − desconto + frete; PIS/COFINS = produto − desconto.
  const baseICMS = vProd - vDesc + vFrete;
  const basePisCofins = vProd - vDesc;
  const pct = (base, p) => (Number.isFinite(num(p)) ? Math.round((base * num(p)) / 100) : 0);
  const icms00 = it.icms.CST === '00';
  const vICMS = icms00 ? pct(baseICMS, it.icms.pICMS) : 0;
  const vPIS = ['01', '02'].includes(it.pis.CST) ? pct(basePisCofins, it.pis.pPIS) : 0;
  const vCOFINS = ['01', '02'].includes(it.cofins.CST) ? pct(basePisCofins, it.cofins.pCOFINS) : 0;
  return { vProd, vDesc, vFrete, vICMS, vBC: icms00 ? baseICMS : 0, vPIS, vCOFINS };
}

export function totaisNota(nota) {
  const t = { vProd: 0, vDesc: 0, vFrete: 0, vICMS: 0, vBC: 0, vPIS: 0, vCOFINS: 0 };
  for (const it of nota.itens) {
    const ti = totaisItem(it);
    for (const k of Object.keys(t)) t[k] += ti[k];
  }
  t.vNF = t.vProd - t.vDesc + t.vFrete;
  return t;
}

// ---------- operação interna x interestadual ----------
export const idDestDe = (nota) => (nota.dest.ender.UF && nota.dest.ender.UF !== nota.emit.ender.UF ? 2 : 1);

// ---------- pendências (o que falta para a nota estar completa) ----------
export function pendencias(nota) {
  const p = [];
  const falta = (cond, msg) => { if (!cond) p.push(msg); };
  const end = (e, quem) => {
    falta(e.xLgr.trim(), `${quem}: logradouro`);
    falta(e.xBairro.trim(), `${quem}: bairro`);
    falta(/^\d{7}$/.test(digitos(e.cMun)), `${quem}: código IBGE do município (7 dígitos)`);
    falta(e.xMun.trim(), `${quem}: município`);
    falta(e.UF, `${quem}: UF`);
  };

  falta(nota.ide.natOp.trim(), 'Natureza da operação');
  falta(Number(nota.ide.nNF) >= 1, 'Número da nota');

  falta(nota.emit.xNome.trim(), 'Emitente: nome');
  falta(digitos(nota.emit.IE), 'Emitente: inscrição estadual');
  falta(digitos(nota.emit.ender.CEP).length === 8, 'Emitente: CEP');
  end(nota.emit.ender, 'Emitente');

  const d = nota.dest;
  if (d.tipo === 'PF') falta(cpfValido(d.CPF), 'Destinatário: CPF válido');
  else falta(cnpjValido(d.CNPJ), 'Destinatário: CNPJ válido');
  falta(d.xNome.trim(), 'Destinatário: nome');
  if (d.indIEDest === '1') falta(digitos(d.IE), 'Destinatário: IE (contribuinte)');
  end(d.ender, 'Destinatário');

  nota.itens.forEach((it, i) => {
    const n = `Item ${i + 1}`;
    falta(it.xProd.trim(), `${n}: descrição`);
    falta(/^\d{8}$/.test(digitos(it.NCM)), `${n}: NCM com 8 dígitos`);
    falta(/^\d{4}$/.test(digitos(it.CFOP)), `${n}: CFOP com 4 dígitos`);
    falta(num(it.qCom) > 0, `${n}: quantidade`);
    falta(num(it.vUnCom) > 0, `${n}: valor unitário`);
    if (it.icms.CST === '00') falta(num(it.icms.pICMS) >= 0, `${n}: alíquota de ICMS`);
    if (['01', '02'].includes(it.pis.CST)) falta(num(it.pis.pPIS) >= 0, `${n}: alíquota de PIS`);
    if (['01', '02'].includes(it.cofins.CST)) falta(num(it.cofins.pCOFINS) >= 0, `${n}: alíquota de COFINS`);
  });

  if (nota.pag.length > 1) nota.pag.forEach((pg, i) => falta(num(pg.vPag) >= 0, `Pagamento ${i + 1}: valor`));
  return p;
}

// Avisos (não impedem o envio, mas provavelmente geram rejeição).
export function avisos(nota) {
  const a = [];
  const idDest = idDestDe(nota);
  nota.itens.forEach((it, i) => {
    const c = digitos(it.CFOP)[0];
    if (idDest === 1 && c && c !== '5' && nota.ide.tpNF === '1') a.push(`Item ${i + 1}: operação interna normalmente usa CFOP 5xxx`);
    if (idDest === 2 && c && c !== '6' && nota.ide.tpNF === '1') a.push(`Item ${i + 1}: operação interestadual normalmente usa CFOP 6xxx`);
  });
  if (nota.pag.length > 1) {
    const soma = nota.pag.reduce((s, p) => s + (Number.isFinite(num(p.vPag)) ? paraCentavos(num(p.vPag)) : 0), 0);
    if (soma !== totaisNota(nota).vNF) a.push('A soma dos pagamentos difere do total da nota');
  }
  return a;
}

// ---------- conversão para o formato de gerarXmlNFe ----------
function enderecoPayload(e) {
  return {
    xLgr: e.xLgr, nro: e.nro || 'SN', xCpl: e.xCpl || undefined, xBairro: e.xBairro, cMun: digitos(e.cMun),
    xMun: e.xMun, UF: e.UF, CEP: digitos(e.CEP) || undefined, fone: digitos(e.fone) || undefined,
  };
}

export function paraPayload(nota) {
  const doc = (p) => (digitos(p.CNPJ).length === 14 ? { CNPJ: digitos(p.CNPJ) } : { CPF: digitos(p.CPF) });
  const d = nota.dest;
  return {
    ide: {
      natOp: nota.ide.natOp, serie: Number(nota.ide.serie), nNF: Number(nota.ide.nNF), tpNF: Number(nota.ide.tpNF),
      idDest: idDestDe(nota), cMunFG: digitos(nota.emit.ender.cMun), finNFe: Number(nota.ide.finNFe),
      indFinal: Number(nota.ide.indFinal), indPres: Number(nota.ide.indPres),
    },
    emit: { ...doc(nota.emit), xNome: nota.emit.xNome, IE: digitos(nota.emit.IE), CRT: Number(nota.emit.CRT), ender: enderecoPayload(nota.emit.ender) },
    dest: {
      ...(d.tipo === 'PF' ? { CPF: digitos(d.CPF) } : { CNPJ: digitos(d.CNPJ) }),
      xNome: d.xNome, indIEDest: Number(d.indIEDest), IE: d.indIEDest === '1' ? digitos(d.IE) : undefined,
      email: d.email || undefined, ender: enderecoPayload(d.ender),
    },
    itens: nota.itens.map((it) => ({
      cProd: it.cProd || String(nota.itens.indexOf(it) + 1), xProd: it.xProd, NCM: digitos(it.NCM), CFOP: digitos(it.CFOP),
      uCom: it.uCom, qCom: num(it.qCom), vUnCom: num(it.vUnCom),
      vDesc: num(it.vDesc) || undefined, vFrete: num(it.vFrete) || undefined, infAdProd: it.infAdProd || undefined,
      icms: { orig: Number(it.icms.orig), CST: it.icms.CST, pICMS: num(it.icms.pICMS) },
      pis: { CST: it.pis.CST, pPIS: num(it.pis.pPIS) },
      cofins: { CST: it.cofins.CST, pCOFINS: num(it.cofins.pCOFINS) },
    })),
    transp: { modFrete: Number(nota.transp.modFrete) },
    pag: nota.pag.map((p) => ({ tPag: p.tPag, vPag: nota.pag.length > 1 || p.vPag !== '' ? num(p.vPag) : undefined })),
    infAdic: { infCpl: nota.infAdic.infCpl || undefined },
  };
}

export const formatarChave = (c) => (c ? c.replace(/(\d{4})(?=\d)/g, '$1 ') : '');
export { centavos };
