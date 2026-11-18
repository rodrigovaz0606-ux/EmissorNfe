// Estado do formulário, conversão para o formato do backend, totais e pendências.
// Usa o mesmo arredondamento do backend para que tela e XML calculem igual.
import { cpfValido, cnpjValido } from './fiscal/documentos.js';
import { multiplicarEmCentavos, paraCentavos, centavos } from './fiscal/valores.js';
import { TIPOS_NOTA } from './opcoes.js';

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

// Emitente a partir da fazenda do Campo Certo (cadastro principal): IE e endereço. Nome, regime e
// telefone continuam os do emitente; campos que a fazenda não tem não apagam o que já estava.
export function emitenteDaFazenda(emit, fazenda) {
  const f = fazenda.endereco ?? {};
  const ou = (novo, atual) => (String(novo ?? '').trim() ? String(novo).trim().toUpperCase() : atual);
  return {
    ...emit,
    IE: digitos(fazenda.ie) || emit.IE,
    ender: {
      ...emit.ender,
      xLgr: ou(f.xLgr, emit.ender.xLgr),
      nro: ou(f.nro, emit.ender.nro) || 'SN',
      xCpl: ou(f.xCpl, emit.ender.xCpl),
      xBairro: ou(f.xBairro, emit.ender.xBairro),
      CEP: digitos(f.CEP) || emit.ender.CEP,
      cMun: digitos(f.cMun) || emit.ender.cMun,
      xMun: ou(f.xMun, emit.ender.xMun),
      UF: ou(f.UF, emit.ender.UF),
    },
  };
}

export const itemVazio = () => ({
  cProd: '', xProd: '', NCM: '', CFOP: '5101', uCom: 'UN', qCom: '1', vUnCom: '', vDesc: '', vFrete: '', infAdProd: '', cBenef: '',
  icms: { orig: '0', CST: '51', pICMS: '' },
  pis: { CST: '08', pPIS: '' },
  cofins: { CST: '08', pCOFINS: '' },
  ibscbs: { cClassTrib: '410014' },
});

export const guiaVazia = () => ({ tpGuia: '1', UFGuia: 'GO', serieGuia: '', nGuia: '' });

// ---------- guia de trânsito animal (NT 2024.003) ----------
// Animal vivo: NCM 01xxxxxx e 0301xxxx. Retornos, transferências e afins (pelos 3 últimos dígitos do CFOP)
// e notas que não são "normais" dispensam a GTA (regra 836).
export const ncmAnimalVivo = (ncm) => /^(01|0301)/.test(digitos(ncm));
const CFOP_SEM_GTA = ['105', '106', '111', '112', '113', '114', '115', '155', '156', '907', '949', '454', '922'];
export const itensQuePedemGta = (nota) => nota.itens
  .map((it, i) => ({ it, n: i + 1 }))
  .filter(({ it }) => ncmAnimalVivo(it.NCM) && !CFOP_SEM_GTA.includes(digitos(it.CFOP).slice(1)))
  .map(({ n }) => n);
export const temAnimalVivo = (nota) => nota.itens.some((it) => ncmAnimalVivo(it.NCM));
const guiaDe = (nota) => nota.guia ?? guiaVazia();
export const guiaPreenchida = (nota) => Boolean(digitos(guiaDe(nota).nGuia));
const guiaAnimal = (nota) => guiaPreenchida(nota) && ['1', '2', '3'].includes(guiaDe(nota).tpGuia);

export const notaVazia = () => ({
  ide: { tipoNota: 'venda_producao', natOp: 'VENDA DE PRODUCAO DO ESTABELECIMENTO', serie: '1', nNF: '1', tpNF: '1', finNFe: '1', indFinal: '1', indPres: '1', refNFe: [] },
  emit: { CPF: '', CNPJ: '', xNome: '', IE: '', CRT: '3', ender: enderecoVazio() },
  dest: { tipo: 'PF', CPF: '', CNPJ: '', xNome: '', indIEDest: '9', IE: '', email: '', ender: enderecoVazio() },
  itens: [itemVazio()],
  transp: { modFrete: '9' },
  pag: [{ tPag: '01', vPag: '' }],
  infAdic: { infCpl: '' },
  guia: guiaVazia(),
});

// ---------- cadastro de produtos ----------
const CAMPOS_PRODUTO = ['cProd', 'xProd', 'NCM', 'CFOP', 'uCom', 'vUnCom', 'cBenef'];

export const produtoVazio = () => {
  const { cProd, xProd, NCM, CFOP, uCom, vUnCom, cBenef, icms, pis, cofins, ibscbs } = itemVazio();
  return { cProd, xProd, NCM, CFOP, uCom, vUnCom, cBenef, icms, pis, cofins, ibscbs, controlaEstoque: false, estoqueMinimo: '', estoqueInicial: '' };
};

// Item da nota a partir de um produto cadastrado (quantidade 1; desconto e frete em branco).
export function itemDoProduto(p) {
  const base = itemVazio();
  const item = {
    ...base, icms: { ...base.icms, ...p.icms }, pis: { ...base.pis, ...p.pis }, cofins: { ...base.cofins, ...p.cofins },
    ibscbs: { ...base.ibscbs, ...p.ibscbs },
  };
  for (const c of CAMPOS_PRODUTO) item[c] = p[c] ?? base[c];
  return item;
}

// Produto a partir de um item da nota (para "salvar no cadastro").
export function produtoDoItem(it) {
  const p = { icms: { ...it.icms }, pis: { ...it.pis }, cofins: { ...it.cofins }, ibscbs: { ...(it.ibscbs ?? itemVazio().ibscbs) } };
  for (const c of CAMPOS_PRODUTO) p[c] = it[c];
  return p;
}

export const formatarQuantidade = (q) => (Number(q) || 0).toLocaleString('pt-BR', { maximumFractionDigits: 4 });
export const estoqueBaixo = (p) => p.controlaEstoque && p.saldo < (num(p.estoqueMinimo) || 0);

// Avisos de saldo: não impedem a emissão, porque o estoque pode estar desatualizado.
export function avisosEstoque(nota, produtos) {
  if (!produtos || nota.ide.tpNF !== '1' || !['1', '4'].includes(nota.ide.finNFe)) return [];
  const pedido = new Map();
  for (const it of nota.itens) {
    const c = it.cProd.trim().toUpperCase();
    if (c) pedido.set(c, (pedido.get(c) ?? 0) + (num(it.qCom) || 0));
  }
  const a = [];
  for (const [codigo, quantidade] of pedido) {
    const p = produtos.find((x) => x.cProd.toUpperCase() === codigo);
    if (p?.controlaEstoque && quantidade > p.saldo) {
      a.push(`${p.cProd}: a nota pede ${formatarQuantidade(quantidade)} ${p.uCom}, e o estoque tem ${formatarQuantidade(p.saldo)}`);
    }
  }
  return a;
}

export const itemEstaVazio = (it) => !it.xProd.trim() && !it.cProd.trim() && !it.vUnCom;

// ---------- cadastro de clientes ----------
export const clienteVazio = () => ({ tipo: 'PF', CPF: '', CNPJ: '', xNome: '', indIEDest: '9', IE: '', email: '', ender: enderecoVazio() });

// Destinatário da nota a partir de um cliente cadastrado (sem o id do cadastro).
export function destDoCliente(c) {
  const { id, ...resto } = c;
  return { ...clienteVazio(), ...resto, ender: { ...enderecoVazio(), ...c.ender } };
}

// ---------- tipos de nota ----------
const tipoDaNota = (nota) => TIPOS_NOTA.find((t) => t.id === nota.ide.tipoNota);

// CFOP do tipo de nota para a operação (interna ou interestadual); undefined se for personalizado.
export function cfopDoTipo(nota) {
  return tipoDaNota(nota)?.cfop[idDestDe(nota) - 1];
}

// Aplica um modelo de operação à nota. Id vazio ("personalizado") só desliga o modelo.
export function aplicarTipoNota(nota, id) {
  const t = TIPOS_NOTA.find((x) => x.id === id);
  if (!t) return { ...nota, ide: { ...nota.ide, tipoNota: '' } };
  const cfop = t.cfop[idDestDe(nota) - 1];
  const semPagamento = nota.pag.every((p) => p.tPag === '90');
  return {
    ...nota,
    ide: {
      ...nota.ide, tipoNota: id, natOp: t.natOp, tpNF: t.tpNF, finNFe: t.finNFe,
      indFinal: t.venda ? '1' : '0', indPres: t.venda ? '1' : '9',
    },
    itens: nota.itens.map((it) => ({ ...it, CFOP: cfop })),
    pag: t.semPagamento ? [{ tPag: '90', vPag: '' }] : semPagamento ? [{ tPag: '01', vPag: '' }] : nota.pag,
  };
}

export const precisaReferencia = (nota) => ['2', '4'].includes(nota.ide.finNFe);
export const aceitaReferencia = (nota) => ['2', '3', '4'].includes(nota.ide.finNFe);
const referencias = (nota) => (nota.ide.refNFe ?? []).map(digitos).filter(Boolean);

// ---------- duplicar nota ----------
const texto = (v) => (v === undefined || v === null ? '' : String(v));
const decimal = (v) => texto(v).replace('.', ',');
const enderecoDoPayload = (e = {}) => ({
  ...enderecoVazio(), CEP: texto(e.CEP), xLgr: texto(e.xLgr), nro: texto(e.nro), xCpl: texto(e.xCpl), xBairro: texto(e.xBairro),
  cMun: texto(e.cMun), xMun: texto(e.xMun), UF: texto(e.UF) || 'GO', fone: texto(e.fone),
});

// Formulário a partir dos dados guardados de uma nota. O emitente continua o atual e o número
// fica em branco, para a tela sugerir o próximo livre.
export function notaDoPayload(p, atual) {
  const d = p.dest ?? {};
  const tipo = TIPOS_NOTA.find((t) => t.natOp === p.ide?.natOp && t.tpNF === texto(p.ide?.tpNF) && t.finNFe === texto(p.ide?.finNFe));
  const vazia = notaVazia();
  return {
    ...vazia,
    ide: {
      ...vazia.ide, tipoNota: tipo?.id ?? '', natOp: texto(p.ide?.natOp), serie: texto(p.ide?.serie) || '1', nNF: '',
      tpNF: texto(p.ide?.tpNF) || '1', finNFe: texto(p.ide?.finNFe) || '1', indFinal: texto(p.ide?.indFinal) || '1',
      indPres: texto(p.ide?.indPres) || '1', refNFe: (p.ide?.refNFe ?? []).map(texto),
    },
    emit: atual.emit,
    dest: {
      tipo: d.CNPJ ? 'PJ' : 'PF', CPF: texto(d.CPF), CNPJ: texto(d.CNPJ), xNome: texto(d.xNome), indIEDest: texto(d.indIEDest) || '9',
      IE: texto(d.IE), email: texto(d.email), ender: enderecoDoPayload(d.ender),
    },
    itens: (p.itens ?? []).map((it) => ({
      ...itemVazio(), cProd: texto(it.cProd), xProd: texto(it.xProd), NCM: texto(it.NCM), CFOP: texto(it.CFOP), uCom: texto(it.uCom),
      qCom: decimal(it.qCom), vUnCom: decimal(it.vUnCom), vDesc: decimal(it.vDesc), vFrete: decimal(it.vFrete), infAdProd: texto(it.infAdProd),
      cBenef: texto(it.cBenef),
      ibscbs: { cClassTrib: texto(it.ibscbs?.cClassTrib) || '410014' },
      icms: { orig: texto(it.icms?.orig) || '0', CST: texto(it.icms?.CST) || '51', pICMS: decimal(it.icms?.pICMS) },
      pis: { CST: texto(it.pis?.CST) || '08', pPIS: decimal(it.pis?.pPIS) },
      cofins: { CST: texto(it.cofins?.CST) || '08', pCOFINS: decimal(it.cofins?.pCOFINS) },
    })),
    transp: { modFrete: texto(p.transp?.modFrete) || '9' },
    pag: (p.pag?.length ? p.pag : [{ tPag: '90' }]).map((pg) => ({ tPag: texto(pg.tPag), vPag: decimal(pg.vPag) })),
    infAdic: { infCpl: texto(p.infAdic?.infCpl) },
    // A guia é de um transporte só: a cópia começa sem número, para não repetir a GTA (rejeição 838).
    guia: p.agropecuario?.guiaTransito ? { ...guiaVazia(), tpGuia: texto(p.agropecuario.guiaTransito.tpGuia), UFGuia: texto(p.agropecuario.guiaTransito.UFGuia) } : guiaVazia(),
  };
}

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
  // IBS/CBS (NT 2025.002): base = produto + frete − desconto − PIS − COFINS − ICMS; alíquotas de 2026
  // em décimos de milésimo de % (0,1% = 1000) e redução de 60% no 200036. Mesma conta do backend.
  const classe = classificacaoIbsCbs(it);
  const baseIbsCbs = vProd + vFrete - vDesc - vPIS - vCOFINS - vICMS;
  const ibs = (p) => (classe.valores ? valorIbsCbs(baseIbsCbs, aliquotaEfetiva(p, classe.reducao)) : 0);
  const vIBS = ibs(ALIQ_2026.uf) + ibs(ALIQ_2026.mun);
  const vCBS = ibs(ALIQ_2026.cbs);
  return { vProd, vDesc, vFrete, vICMS, vBC: icms00 ? baseICMS : 0, vPIS, vCOFINS, vIBS, vCBS };
}

const CLASSES_IBSCBS = { '410014': { reducao: 0, valores: false }, '000001': { reducao: 0, valores: true }, '200036': { reducao: 60, valores: true } };
const ALIQ_2026 = { uf: 1000, mun: 0, cbs: 9000 };
export const classificacaoIbsCbs = (it) => CLASSES_IBSCBS[it.ibscbs?.cClassTrib ?? '410014'] ?? { reducao: 0, valores: false };
const aliquotaEfetiva = (p, reducao) => Math.floor((p * (100 - reducao) + 50) / 100);
const valorIbsCbs = (base, p) => Math.floor((base * p + 500000) / 1000000);

export function totaisNota(nota) {
  const t = { vProd: 0, vDesc: 0, vFrete: 0, vICMS: 0, vBC: 0, vPIS: 0, vCOFINS: 0, vIBS: 0, vCBS: 0 };
  for (const it of nota.itens) {
    const ti = totaisItem(it);
    for (const k of Object.keys(t)) t[k] += ti[k];
  }
  t.vNF = t.vProd - t.vDesc + t.vFrete;
  return t;
}

// ---------- operação interna x interestadual ----------
export const idDestDe = (nota) => (nota.dest.ender.UF && nota.dest.ender.UF !== nota.emit.ender.UF ? 2 : 1);

// ---------- série por tipo de emitente (NT 2018.001) ----------
// Emitente com CPF usa as séries 920 a 969; fora disso a SEFAZ rejeita com 407.
export const emitenteCpf = (nota) => digitos(nota.emit.CPF).length === 11;
export function serieAdequada(nota) {
  const n = Number(nota.ide.serie);
  if (!Number.isInteger(n) || n < 0 || n > 999) return false;
  const faixaCpf = n >= 920 && n <= 969;
  if (emitenteCpf(nota)) return faixaCpf;
  return !digitos(nota.emit.CNPJ) || !faixaCpf;
}
export const serieSugerida = (nota) => (emitenteCpf(nota) ? '920' : '1');

// ---------- código de benefício fiscal (cBenef) ----------
// CSTs de ICMS que indicam benefício (isenção, não tributação, suspensão, diferimento):
// em Goiás, sem cBenef a SEFAZ rejeita com 930. O código vem da tabela da SEFAZ-GO (contador).
export const CST_COM_BENEFICIO = ['20', '30', '40', '41', '50', '51', '70', '90'];
export const pedeCbenef = (it) => CST_COM_BENEFICIO.includes(it.icms.CST);
export const cBenefValido = (c) => {
  const v = String(c ?? '').trim().toUpperCase();
  return v === '' || v === 'SEM CBENEF' || /^[A-Z0-9]{8}([A-Z0-9]{2})?$/.test(v);
};

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
  if (nota.emit.ender.UF === 'GO') {
    const semGuia = itensQuePedemGta(nota);
    if (nota.ide.finNFe === '1' && semGuia.length && !guiaAnimal(nota)) falta(false, `Item ${semGuia.join(', ')}: animal vivo exige a Guia de Trânsito Animal (seção 5)`);
    if (guiaAnimal(nota) && !temAnimalVivo(nota)) falta(false, 'Guia de trânsito animal informada sem nenhum item de animal vivo');
  }
  if (guiaPreenchida(nota)) {
    const g = guiaDe(nota);
    falta(/^\d{1,9}$/.test(String(g.nGuia).trim()), 'Guia de trânsito: número só com dígitos, até 9');
    falta(String(g.serieGuia ?? '').trim().length <= 9, 'Guia de trânsito: série até 9 caracteres');
  }
  falta(serieAdequada(nota), emitenteCpf(nota) ? 'Série: emitente com CPF usa série de 920 a 969' : 'Série: 920 a 969 é só para emitente com CPF');
  const refs = referencias(nota);
  if (precisaReferencia(nota)) falta(refs.length > 0, 'Chave da NF-e referenciada');
  if (aceitaReferencia(nota)) refs.forEach((r, i) => falta(r.length === 44, `NF-e referenciada ${i + 1}: chave com 44 dígitos`));

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
    falta(cBenefValido(it.cBenef), `${n}: código de benefício fiscal com 8 ou 10 caracteres`);
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
    // Saída interna 5xxx, interestadual 6xxx; entrada interna 1xxx, interestadual 2xxx.
    const esperado = nota.ide.tpNF === '1' ? (idDest === 1 ? '5' : '6') : (idDest === 1 ? '1' : '2');
    const operacao = idDest === 1 ? 'interna' : 'interestadual';
    if (c && c !== esperado) a.push(`Item ${i + 1}: ${nota.ide.tpNF === '1' ? 'saída' : 'entrada'} ${operacao} normalmente usa CFOP ${esperado}xxx`);
  });
  nota.itens.forEach((it, i) => {
    if ((it.ibscbs?.cClassTrib ?? '410014') === '410014' && digitos(nota.emit.CNPJ)) {
      a.push(`Item ${i + 1}: IBS/CBS 410014 é para produtor rural não contribuinte (receita até R$ 3,6 milhões/ano). Confirme com o contador`);
    }
  });
  if (nota.emit.ender.UF === 'GO') {
    nota.itens.forEach((it, i) => {
      if (pedeCbenef(it) && !String(it.cBenef ?? '').trim()) {
        a.push(`Item ${i + 1}: Goiás exige o código de benefício fiscal (cBenef) para o CST ${it.icms.CST} (rejeição 930)`);
      }
    });
  }
  if (['3', '4'].includes(nota.ide.finNFe) && nota.pag.some((p) => p.tPag !== '90')) {
    a.push('Nota de devolução ou ajuste normalmente usa a forma de pagamento "Sem pagamento"');
  }
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
      refNFe: aceitaReferencia(nota) && referencias(nota).length > 0 ? referencias(nota) : undefined,
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
      cBenef: String(it.cBenef ?? '').trim().toUpperCase() || undefined,
      ibscbs: { cClassTrib: it.ibscbs?.cClassTrib ?? '410014' },
      icms: { orig: Number(it.icms.orig), CST: it.icms.CST, pICMS: num(it.icms.pICMS) },
      pis: { CST: it.pis.CST, pPIS: num(it.pis.pPIS) },
      cofins: { CST: it.cofins.CST, pCOFINS: num(it.cofins.pCOFINS) },
    })),
    transp: { modFrete: Number(nota.transp.modFrete) },
    pag: nota.pag.map((p) => ({ tPag: p.tPag, vPag: nota.pag.length > 1 || p.vPag !== '' ? num(p.vPag) : undefined })),
    infAdic: { infCpl: nota.infAdic.infCpl || undefined },
    agropecuario: guiaPreenchida(nota) ? {
      guiaTransito: {
        tpGuia: guiaDe(nota).tpGuia, UFGuia: guiaDe(nota).UFGuia,
        serieGuia: String(guiaDe(nota).serieGuia ?? '').trim() || undefined, nGuia: String(guiaDe(nota).nGuia).trim(),
      },
    } : undefined,
  };
}

export const formatarChave = (c) => (c ? c.replace(/(\d{4})(?=\d)/g, '$1 ') : '');
export { centavos };
