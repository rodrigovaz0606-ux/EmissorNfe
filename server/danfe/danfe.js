// DANFE (Documento Auxiliar da NF-e) em PDF, retrato A4, a partir do XML (NFe ou nfeProc).
import PDFDocument from 'pdfkit';
import { XMLParser } from 'fast-xml-parser';
import { barras } from './code128.js';

const parser = new XMLParser({ removeNSPrefix: true, ignoreAttributes: false, attributeNamePrefix: '@', parseTagValue: false });

const lista = (v) => (v === undefined || v === null ? [] : Array.isArray(v) ? v : [v]);
const dinheiro = (v) => Number(v ?? 0).toLocaleString('pt-BR', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
const quantidade = (v) => Number(v ?? 0).toLocaleString('pt-BR', { minimumFractionDigits: 0, maximumFractionDigits: 4 });
const unitario = (v) => Number(v ?? 0).toLocaleString('pt-BR', { minimumFractionDigits: 2, maximumFractionDigits: 10 });
const data = (dh) => (dh ? `${dh.slice(8, 10)}/${dh.slice(5, 7)}/${dh.slice(0, 4)}` : '');
const hora = (dh) => (dh ? dh.slice(11, 19) : '');
const chaveFormatada = (c) => c.replace(/(\d{4})(?=\d)/g, '$1 ');
const cep = (c) => (c ? String(c).replace(/^(\d{5})(\d{3})$/, '$1-$2') : '');
function documento(p) {
  if (p?.CNPJ) return String(p.CNPJ).replace(/^(\d{2})(\d{3})(\d{3})(\d{4})(\d{2})$/, '$1.$2.$3/$4-$5');
  if (p?.CPF) return String(p.CPF).replace(/^(\d{3})(\d{3})(\d{3})(\d{2})$/, '$1.$2.$3-$4');
  return p?.idEstrangeiro ?? '';
}
const MOD_FRETE = {
  0: '0-Por conta do Remetente', 1: '1-Por conta do Destinatário', 2: '2-Por conta de Terceiros',
  3: '3-Próprio Remetente', 4: '4-Próprio Destinatário', 9: '9-Sem Transporte',
};
const T_PAG = {
  '01': 'Dinheiro', '02': 'Cheque', '03': 'Cartão de Crédito', '04': 'Cartão de Débito', '05': 'Crédito Loja', '10': 'Vale Alimentação',
  '11': 'Vale Refeição', '12': 'Vale Presente', '13': 'Vale Combustível', '15': 'Boleto', '16': 'Depósito', '17': 'PIX',
  '18': 'Transferência', '19': 'Fidelidade', '90': 'Sem pagamento', '99': 'Outros',
};

/** Extrai do XML o que o DANFE precisa. */
export function lerNotaParaDanfe(xml) {
  const doc = parser.parse(xml);
  const nfe = doc.nfeProc?.NFe ?? doc.NFe;
  if (!nfe?.infNFe) throw new Error('XML não contém uma NF-e.');
  const inf = nfe.infNFe;
  const prot = doc.nfeProc?.protNFe?.infProt ?? null;
  return {
    chave: inf['@Id'].replace(/^NFe/, ''),
    ide: inf.ide, emit: inf.emit, dest: inf.dest ?? {},
    itens: lista(inf.det).map((d) => {
      const icms = Object.values(d.imposto?.ICMS ?? {})[0] ?? {};
      return { ...d.prod, nItem: d['@nItem'], icms, infAdProd: d.infAdProd };
    }),
    total: inf.total?.ICMSTot ?? {},
    transp: inf.transp ?? {},
    pag: lista(inf.pag?.detPag),
    infAdic: inf.infAdic ?? {},
    protocolo: prot && ['100', '150'].includes(prot.cStat) ? prot : null,
  };
}

// ---------- desenho ----------
const M = 18; // margem
const LARG = 595.28 - 2 * M;

function caixa(doc, x, y, w, h, rotulo, valor = '', o = {}) {
  doc.lineWidth(0.5).rect(x, y, w, h).stroke('#000');
  if (rotulo) doc.font('Helvetica').fontSize(5.2).fillColor('#000').text(rotulo.toUpperCase(), x + 2, y + 2, { width: w - 4, lineBreak: false });
  if (valor !== '' && valor !== undefined && valor !== null) {
    doc.font(o.negrito ? 'Helvetica-Bold' : 'Helvetica').fontSize(o.tamanho ?? 7.5)
      .text(String(valor), x + 3, y + (rotulo ? 9 : 3), { width: w - 6, align: o.alinhar ?? 'left', lineBreak: o.quebrar ?? false, ellipsis: !o.quebrar, height: h - (rotulo ? 10 : 4) });
  }
}

function tituloBloco(doc, y, texto) {
  doc.font('Helvetica-Bold').fontSize(6.5).fillColor('#000').text(texto, M, y, { lineBreak: false });
  return y + 8;
}

function codigoDeBarras(doc, chave, x, y, w, h) {
  const { barras: bs, modulos } = barras(chave);
  const modulo = w / modulos;
  doc.save().fillColor('#000');
  for (const b of bs) doc.rect(x + b.x * modulo, y, b.largura * modulo, h).fill();
  doc.restore();
}

function marcaDagua(doc, texto, cor) {
  doc.save().fillColor(cor).opacity(0.13).font('Helvetica-Bold').fontSize(54)
    .rotate(-35, { origin: [297, 470] }).text(texto, 20, 440, { width: 555, align: 'center' }).restore();
}

function canhoto(doc, n, y) {
  const h = 46;
  const wNum = 95;
  caixa(doc, M, y, LARG - wNum, 22, '', `RECEBEMOS DE ${n.emit.xNome} OS PRODUTOS/SERVIÇOS CONSTANTES DA NOTA FISCAL ELETRÔNICA INDICADA AO LADO. EMISSÃO: ${data(n.ide.dhEmi)} DESTINATÁRIO: ${n.dest.xNome ?? ''} VALOR TOTAL: R$ ${dinheiro(n.total.vNF)}`, { tamanho: 6.2, quebrar: true });
  caixa(doc, M, y + 22, 110, h - 22, 'Data de recebimento');
  caixa(doc, M + 110, y + 22, LARG - wNum - 110, h - 22, 'Identificação e assinatura do recebedor');
  doc.lineWidth(0.5).rect(LARG + M - wNum, y, wNum, h).stroke();
  doc.font('Helvetica-Bold').fontSize(11).text('NF-e', LARG + M - wNum, y + 6, { width: wNum, align: 'center' });
  doc.fontSize(8.5).text(`Nº ${String(n.ide.nNF).padStart(9, '0').replace(/(\d{3})(\d{3})(\d{3})/, '$1.$2.$3')}`, LARG + M - wNum, y + 21, { width: wNum, align: 'center' });
  doc.text(`SÉRIE ${String(n.ide.serie).padStart(3, '0')}`, LARG + M - wNum, y + 32, { width: wNum, align: 'center' });
  const yc = y + h + 5;
  doc.save().dash(3, { space: 2 }).moveTo(M, yc).lineTo(M + LARG, yc).stroke().undash().restore();
  return yc + 6;
}

function cabecalho(doc, n, y, folha) {
  const h = 92;
  const wEmit = 230;
  const wDanfe = 95;
  const xDanfe = M + wEmit;
  const xChave = xDanfe + wDanfe;
  const wChave = LARG - wEmit - wDanfe;

  // Emitente
  doc.lineWidth(0.5).rect(M, y, wEmit, h).stroke();
  const e = n.emit;
  const end = e.enderEmit ?? {};
  doc.font('Helvetica').fontSize(5.2).text('IDENTIFICAÇÃO DO EMITENTE', M + 2, y + 2);
  doc.font('Helvetica-Bold').fontSize(9).text(e.xNome, M + 6, y + 14, { width: wEmit - 12, align: 'center' });
  doc.font('Helvetica').fontSize(7).text(
    [`${end.xLgr ?? ''}, ${end.nro ?? ''}${end.xCpl ? ` - ${end.xCpl}` : ''}`, `${end.xBairro ?? ''} - CEP ${cep(end.CEP)}`,
      `${end.xMun ?? ''} - ${end.UF ?? ''}`, end.fone ? `Fone: ${end.fone}` : ''].filter(Boolean).join('\n'),
    M + 6, doc.y + 4, { width: wEmit - 12, align: 'center' },
  );

  // Quadro DANFE
  doc.rect(xDanfe, y, wDanfe, h).stroke();
  doc.font('Helvetica-Bold').fontSize(13).text('DANFE', xDanfe, y + 5, { width: wDanfe, align: 'center' });
  doc.font('Helvetica').fontSize(6.3).text('Documento Auxiliar da\nNota Fiscal Eletrônica', xDanfe, y + 21, { width: wDanfe, align: 'center' });
  doc.fontSize(7).text('0 - ENTRADA\n1 - SAÍDA', xDanfe + 8, y + 41);
  doc.rect(xDanfe + 62, y + 41, 18, 16).stroke();
  doc.font('Helvetica-Bold').fontSize(11).text(String(n.ide.tpNF), xDanfe + 62, y + 44, { width: 18, align: 'center' });
  doc.fontSize(8).text(`Nº ${String(n.ide.nNF).padStart(9, '0').replace(/(\d{3})(\d{3})(\d{3})/, '$1.$2.$3')}`, xDanfe, y + 62, { width: wDanfe, align: 'center' });
  doc.text(`SÉRIE ${String(n.ide.serie).padStart(3, '0')}`, xDanfe, y + 72, { width: wDanfe, align: 'center' });
  doc.font('Helvetica').fontSize(7).text(`FOLHA ${folha}`, xDanfe, y + 82, { width: wDanfe, align: 'center' });

  // Chave e código de barras
  doc.rect(xChave, y, wChave, h).stroke();
  codigoDeBarras(doc, n.chave, xChave + 8, y + 5, wChave - 16, 30);
  caixa(doc, xChave, y + 40, wChave, 20, 'Chave de acesso', chaveFormatada(n.chave), { negrito: true, alinhar: 'center', tamanho: 7.6 });
  doc.font('Helvetica').fontSize(6.6).text(
    'Consulta de autenticidade no portal nacional da NF-e www.nfe.fazenda.gov.br/portal ou no site da Sefaz Autorizadora',
    xChave + 4, y + 66, { width: wChave - 8, align: 'center' },
  );

  // Natureza e protocolo
  let yy = y + h;
  const p = n.protocolo;
  caixa(doc, M, yy, wEmit + wDanfe, 20, 'Natureza da operação', n.ide.natOp);
  caixa(doc, xChave, yy, wChave, 20, 'Protocolo de autorização de uso',
    p ? `${p.nProt} - ${data(p.dhRecbto)} ${hora(p.dhRecbto)}` : 'NF-e SEM PROTOCOLO DE AUTORIZAÇÃO', { alinhar: 'center', negrito: !p });
  yy += 20;
  const t = LARG / 3;
  caixa(doc, M, yy, t, 20, 'Inscrição estadual', e.IE);
  caixa(doc, M + t, yy, t, 20, 'Inscrição estadual do subst. trib.', e.IEST ?? '');
  caixa(doc, M + 2 * t, yy, t, 20, 'CNPJ / CPF', documento(e));
  return yy + 20 + 4;
}

function blocoDestinatario(doc, n, y) {
  y = tituloBloco(doc, y, 'DESTINATÁRIO / REMETENTE');
  const d = n.dest;
  const e = d.enderDest ?? {};
  const h = 20;
  const c3 = 80;
  caixa(doc, M, y, LARG - 2 * c3 - 50, h, 'Nome / razão social', d.xNome);
  caixa(doc, M + LARG - 2 * c3 - 50, y, c3 + 50, h, 'CNPJ / CPF', documento(d));
  caixa(doc, M + LARG - c3, y, c3, h, 'Data da emissão', data(n.ide.dhEmi), { alinhar: 'center' });
  y += h;
  caixa(doc, M, y, LARG - 3 * c3, h, 'Endereço', `${e.xLgr ?? ''}${e.nro ? `, ${e.nro}` : ''}${e.xCpl ? ` - ${e.xCpl}` : ''}`);
  caixa(doc, M + LARG - 3 * c3, y, c3, h, 'Bairro / distrito', e.xBairro);
  caixa(doc, M + LARG - 2 * c3, y, c3, h, 'CEP', cep(e.CEP), { alinhar: 'center' });
  caixa(doc, M + LARG - c3, y, c3, h, 'Data da saída/entrada', data(n.ide.dhSaiEnt), { alinhar: 'center' });
  y += h;
  caixa(doc, M, y, LARG - 3 * c3 - 30, h, 'Município', e.xMun);
  caixa(doc, M + LARG - 3 * c3 - 30, y, c3, h, 'Fone / fax', e.fone ?? '');
  caixa(doc, M + LARG - 2 * c3 - 30, y, 30, h, 'UF', e.UF, { alinhar: 'center' });
  caixa(doc, M + LARG - 2 * c3, y, c3, h, 'Inscrição estadual', d.IE ?? '');
  caixa(doc, M + LARG - c3, y, c3, h, 'Hora da saída/entrada', hora(n.ide.dhSaiEnt), { alinhar: 'center' });
  return y + h + 4;
}

function blocoPagamento(doc, n, y) {
  if (!n.pag.length) return y;
  y = tituloBloco(doc, y, 'PAGAMENTO');
  const w = LARG / Math.max(n.pag.length, 4);
  n.pag.slice(0, 8).forEach((p, i) => {
    caixa(doc, M + (i % 4) * (LARG / 4), y + Math.floor(i / 4) * 20, LARG / 4, 20, T_PAG[p.tPag] ?? `Forma ${p.tPag}`, `R$ ${dinheiro(p.vPag)}`, { alinhar: 'right' });
  });
  void w;
  return y + Math.ceil(Math.min(n.pag.length, 8) / 4) * 20 + 4;
}

function blocoImposto(doc, n, y) {
  y = tituloBloco(doc, y, 'CÁLCULO DO IMPOSTO');
  const t = n.total;
  const linha1 = [
    ['Base de cálc. do ICMS', t.vBC], ['Valor do ICMS', t.vICMS], ['Base de cálc. ICMS S.T.', t.vBCST], ['Valor do ICMS subst.', t.vST],
    ['V. total tributos', t.vTotTrib ?? 0], ['V. total produtos', t.vProd],
  ];
  const linha2 = [
    ['Valor do frete', t.vFrete], ['Valor do seguro', t.vSeg], ['Desconto', t.vDesc], ['Outras desp. acessórias', t.vOutro],
    ['Valor total IPI', t.vIPI], ['V. total da nota', t.vNF],
  ];
  const w = LARG / 6;
  linha1.forEach(([r, v], i) => caixa(doc, M + i * w, y, w, 20, r, dinheiro(v), { alinhar: 'right' }));
  linha2.forEach(([r, v], i) => caixa(doc, M + i * w, y + 20, w, 20, r, dinheiro(v), { alinhar: 'right', negrito: i === 5 }));
  return y + 40 + 4;
}

function blocoTransporte(doc, n, y) {
  y = tituloBloco(doc, y, 'TRANSPORTADOR / VOLUMES TRANSPORTADOS');
  const tr = n.transp.transporta ?? {};
  caixa(doc, M, y, 230, 20, 'Nome / razão social', tr.xNome ?? '');
  caixa(doc, M + 230, y, 110, 20, 'Frete por conta', MOD_FRETE[n.transp.modFrete] ?? n.transp.modFrete);
  caixa(doc, M + 340, y, 70, 20, 'Placa do veículo', n.transp.veicTransp?.placa ?? '');
  caixa(doc, M + 410, y, 30, 20, 'UF', n.transp.veicTransp?.UF ?? '');
  caixa(doc, M + 440, y, LARG - 440, 20, 'CNPJ / CPF', documento(tr));
  return y + 20 + 4;
}

// Colunas da tabela de produtos: [rótulo, largura, alinhamento, função]
const COLUNAS = [
  ['Código', 44, 'left', (i) => i.cProd],
  ['Descrição do produto / serviço', 150, 'left', (i) => i.xProd + (i.infAdProd ? `\n${i.infAdProd}` : '')],
  ['NCM/SH', 38, 'center', (i) => i.NCM],
  ['CST', 22, 'center', (i) => `${i.icms.orig ?? ''}${i.icms.CST ?? i.icms.CSOSN ?? ''}`],
  ['CFOP', 24, 'center', (i) => i.CFOP],
  ['Un', 20, 'center', (i) => i.uCom],
  ['Quant.', 40, 'right', (i) => quantidade(i.qCom)],
  ['Valor unit.', 44, 'right', (i) => unitario(i.vUnCom)],
  ['Valor total', 46, 'right', (i) => dinheiro(i.vProd)],
  ['B. cálc. ICMS', 40, 'right', (i) => dinheiro(i.icms.vBC ?? 0)],
  ['V. ICMS', 34, 'right', (i) => dinheiro(i.icms.vICMS ?? 0)],
  ['Alíq. ICMS', 0, 'right', (i) => (i.icms.pICMS ? Number(i.icms.pICMS).toLocaleString('pt-BR') : '0')],
];

function cabecalhoProdutos(doc, y) {
  y = tituloBloco(doc, y, 'DADOS DOS PRODUTOS / SERVIÇOS');
  let x = M;
  const usado = COLUNAS.reduce((s, c) => s + c[1], 0);
  COLUNAS.forEach(([rotulo, w], i) => {
    const largura = i === COLUNAS.length - 1 ? LARG - usado : w;
    doc.lineWidth(0.5).rect(x, y, largura, 14).stroke();
    doc.font('Helvetica-Bold').fontSize(5.4).text(rotulo.toUpperCase(), x + 1, y + 4.5, { width: largura - 2, align: 'center', lineBreak: false });
    x += largura;
  });
  return y + 14;
}

function linhaProduto(doc, item, y, apenasMedir = false) {
  doc.font('Helvetica').fontSize(6.5);
  const usado = COLUNAS.reduce((s, c) => s + c[1], 0);
  const alturas = COLUNAS.map(([, w, , f], i) => doc.heightOfString(String(f(item) ?? ''), { width: (i === COLUNAS.length - 1 ? LARG - usado : w) - 4 }));
  const h = Math.max(...alturas) + 5;
  if (apenasMedir) return h;
  let x = M;
  COLUNAS.forEach(([, w, alinhar, f], i) => {
    const largura = i === COLUNAS.length - 1 ? LARG - usado : w;
    doc.text(String(f(item) ?? ''), x + 2, y + 2.5, { width: largura - 4, align: alinhar });
    x += largura;
  });
  doc.lineWidth(0.3).moveTo(M, y + h).lineTo(M + LARG, y + h).stroke('#999').strokeColor('#000');
  return y + h;
}

function blocoAdicionais(doc, n, y, h) {
  y = tituloBloco(doc, y, 'DADOS ADICIONAIS');
  const ad = n.infAdic;
  const texto = [ad.infAdFisco, ad.infCpl].filter(Boolean).join('\n');
  caixa(doc, M, y, LARG * 0.66, h, 'Informações complementares', texto, { quebrar: true, tamanho: 6.8 });
  caixa(doc, M + LARG * 0.66, y, LARG * 0.34, h, 'Reservado ao fisco');
}

// Desenha a nota inteira e devolve quantas folhas usou.
function desenhar(doc, n, totalFolhas, opcoes) {
  const ALTURA_ADICIONAIS = 70;
  const FIM_PAGINA = 842 - M - 12; // reserva a linha de aviso de homologação
  let folha = 1;
  const rotulo = () => `${folha}/${totalFolhas}`;

  let y = canhoto(doc, n, M);
  y = cabecalho(doc, n, y, rotulo());
  y = blocoDestinatario(doc, n, y);
  y = blocoPagamento(doc, n, y);
  y = blocoImposto(doc, n, y);
  y = blocoTransporte(doc, n, y);
  y = cabecalhoProdutos(doc, y);

  const novaFolha = () => {
    doc.addPage();
    folha += 1;
    return cabecalho(doc, n, M, rotulo());
  };

  n.itens.forEach((item, i) => {
    const h = linhaProduto(doc, item, y, true);
    const ultimo = i === n.itens.length - 1;
    // Na última página precisa sobrar espaço para os dados adicionais.
    const limite = FIM_PAGINA - (ultimo ? ALTURA_ADICIONAIS + 12 : 0);
    if (y + h > limite) y = cabecalhoProdutos(doc, novaFolha());
    y = linhaProduto(doc, item, y);
  });
  if (y + ALTURA_ADICIONAIS + 12 > FIM_PAGINA) novaFolha();
  blocoAdicionais(doc, n, FIM_PAGINA - ALTURA_ADICIONAIS - 8, ALTURA_ADICIONAIS);

  // Marcas d'água e aviso de homologação em todas as páginas.
  const intervalo = doc.bufferedPageRange();
  for (let i = intervalo.start; i < intervalo.start + intervalo.count; i++) {
    doc.switchToPage(i);
    if (opcoes.cancelada) marcaDagua(doc, 'CANCELADA', '#c00');
    else if (!n.protocolo) marcaDagua(doc, 'NÃO AUTORIZADA', '#c00');
    if (String(n.ide.tpAmb) === '2') {
      doc.save().fillColor('#c00').font('Helvetica-Bold').fontSize(9)
        .text('EMITIDA EM AMBIENTE DE HOMOLOGAÇÃO - SEM VALOR FISCAL', M, 842 - M - 9, { width: LARG, align: 'center', lineBreak: false }).restore();
    }
  }
  return folha;
}

const novoDocumento = (n) => new PDFDocument({
  size: 'A4', bufferPages: true, autoFirstPage: true,
  margins: { top: M, left: M, right: M, bottom: 0 }, // posicionamento é absoluto; evita páginas automáticas
  info: { Title: `DANFE ${n.chave}`, Producer: 'EmissorNfe' },
});

/**
 * Gera o PDF do DANFE.
 * @param {string} xml NFe assinada ou nfeProc
 * @param {{ cancelada?: boolean }} [opcoes]
 * @returns {Promise<Buffer>}
 */
export function gerarDanfe(xml, opcoes = {}) {
  const n = lerNotaParaDanfe(xml);
  // 1ª passada só para saber o total de folhas ("FOLHA 1/2").
  const rascunho = novoDocumento(n);
  rascunho.on('data', () => {});
  const total = desenhar(rascunho, n, 0, opcoes);
  rascunho.end();

  const doc = novoDocumento(n);
  const partes = [];
  doc.on('data', (p) => partes.push(p));
  const pronto = new Promise((resolve, reject) => {
    doc.on('end', () => resolve(Buffer.concat(partes)));
    doc.on('error', reject);
  });
  desenhar(doc, n, total, opcoes);
  doc.end();
  return pronto;
}
