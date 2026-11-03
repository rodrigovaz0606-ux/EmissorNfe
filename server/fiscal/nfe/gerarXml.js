// Geração do XML da NF-e modelo 55, leiaute 4.00 (ainda sem assinatura).
//
// O XML é montado tag a tag porque a ordem dos elementos é obrigatória no schema.
// Tags com valor undefined/null/'' são omitidas (campos opcionais).
//
// Tributação suportada por enquanto:
//   ICMS: 00 (tributada integralmente), 40/41/50 (isenta/não tributada/suspensão), 51 (diferimento, sem valores)
//   PIS/COFINS: 01/02 (alíquota), 04 a 09 (não tributado)

import { montarChave } from '../chaveAcesso.js';
import { cpfValido, cnpjValido } from './documentos.js';
import {
  texto, escaparXml, soDigitos, paraCentavos, centavos, decimal, unitario, multiplicarEmCentavos, dataHora,
} from './formatos.js';

export const NOME_DEST_HOMOLOGACAO = 'NF-E EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL';

// ---------- helpers de montagem ----------

function tag(nome, valor) {
  if (valor === undefined || valor === null || valor === '') return '';
  return `<${nome}>${escaparXml(valor)}</${nome}>`;
}

// Grupo: só aparece se tiver conteúdo.
function grupo(nome, ...filhos) {
  const conteudo = filhos.join('');
  return conteudo ? `<${nome}>${conteudo}</${nome}>` : '';
}

function exigir(cond, msg) {
  if (!cond) throw new Error(msg);
}

function documento(pessoa, quem) {
  if (pessoa.CNPJ) {
    exigir(cnpjValido(pessoa.CNPJ), `CNPJ do ${quem} inválido: ${pessoa.CNPJ}`);
    return tag('CNPJ', soDigitos(pessoa.CNPJ));
  }
  if (pessoa.CPF) {
    exigir(cpfValido(pessoa.CPF), `CPF do ${quem} inválido: ${pessoa.CPF}`);
    return tag('CPF', soDigitos(pessoa.CPF));
  }
  throw new Error(`Informe CNPJ ou CPF do ${quem}.`);
}

function endereco(nome, e) {
  exigir(e, `Endereço (${nome}) não informado.`);
  // No schema, CEP é obrigatório no endereço do emitente (TEnderEmi) e opcional no do destinatário.
  if (nome === 'enderEmit') exigir(/^\d{8}$/.test(soDigitos(e.CEP) ?? ''), 'CEP do emitente é obrigatório (8 dígitos).');
  return grupo(
    nome,
    tag('xLgr', texto(e.xLgr)),
    tag('nro', texto(e.nro ?? 'SN')),
    tag('xCpl', texto(e.xCpl)),
    tag('xBairro', texto(e.xBairro)),
    tag('cMun', soDigitos(e.cMun)),
    tag('xMun', texto(e.xMun)),
    tag('UF', e.UF),
    tag('CEP', soDigitos(e.CEP)),
    tag('cPais', e.cPais ?? '1058'),
    tag('xPais', e.xPais ?? 'BRASIL'),
    tag('fone', soDigitos(e.fone)),
  );
}

// ---------- blocos ----------

function blocoIde(ide, config, chave) {
  // indIntermed é obrigatório quando a venda não é presencial (indPres 2, 3, 4 ou 9).
  const indIntermed = ide.indIntermed ?? ([2, 3, 4, 9].includes(Number(ide.indPres)) ? 0 : undefined);
  return grupo(
    'ide',
    tag('cUF', config.cUF),
    tag('cNF', chave.cNF),
    tag('natOp', texto(ide.natOp)),
    tag('mod', 55),
    tag('serie', Number(ide.serie)),
    tag('nNF', Number(ide.nNF)),
    tag('dhEmi', ide.dhEmi),
    tag('dhSaiEnt', ide.dhSaiEnt),
    tag('tpNF', ide.tpNF ?? 1),
    tag('idDest', ide.idDest ?? 1),
    tag('cMunFG', soDigitos(ide.cMunFG)),
    tag('tpImp', ide.tpImp ?? 1),
    tag('tpEmis', 1),
    tag('cDV', chave.cDV),
    tag('tpAmb', config.tpAmb),
    tag('finNFe', ide.finNFe ?? 1),
    tag('indFinal', ide.indFinal ?? 0),
    tag('indPres', ide.indPres ?? 1),
    tag('indIntermed', indIntermed),
    tag('procEmi', ide.procEmi ?? 0),
    tag('verProc', texto(ide.verProc ?? 'EmissorNfe 0.1')),
  );
}

function blocoEmit(emit) {
  return grupo(
    'emit',
    documento(emit, 'emitente'),
    tag('xNome', texto(emit.xNome)),
    tag('xFant', texto(emit.xFant)),
    endereco('enderEmit', emit.ender),
    tag('IE', soDigitos(emit.IE)),
    tag('CRT', emit.CRT ?? 3),
  );
}

function blocoDest(dest, config) {
  const xNome = config.tpAmb === 2 ? NOME_DEST_HOMOLOGACAO : texto(dest.xNome);
  const indIEDest = Number(dest.indIEDest ?? (dest.IE ? 1 : 9));
  return grupo(
    'dest',
    documento(dest, 'destinatário'),
    tag('xNome', xNome),
    dest.ender ? endereco('enderDest', dest.ender) : '',
    tag('indIEDest', indIEDest),
    tag('IE', indIEDest === 1 ? soDigitos(dest.IE) : undefined),
    tag('email', texto(dest.email)),
  );
}

// baseICMS: valor do produto − desconto + frete + seguro + outras despesas.
function blocoICMS(item, baseICMS) {
  const { orig = 0, CST } = item.icms ?? {};
  const cst = String(CST ?? '').padStart(2, '0');
  switch (cst) {
    case '00': {
      const pICMS = Number(item.icms.pICMS);
      exigir(Number.isFinite(pICMS), `Item ${item.nItem}: ICMS 00 exige pICMS.`);
      const vBC = baseICMS;
      const vICMS = Math.round((vBC * pICMS) / 100);
      return {
        xml: grupo('ICMS', grupo('ICMS00', tag('orig', orig), tag('CST', '00'), tag('modBC', item.icms.modBC ?? 3),
          tag('vBC', centavos(vBC)), tag('pICMS', decimal(pICMS, 2)), tag('vICMS', centavos(vICMS)))),
        vBC, vICMS,
      };
    }
    case '40': case '41': case '50':
      return { xml: grupo('ICMS', grupo('ICMS40', tag('orig', orig), tag('CST', cst))), vBC: 0, vICMS: 0 };
    case '51':
      return { xml: grupo('ICMS', grupo('ICMS51', tag('orig', orig), tag('CST', '51'))), vBC: 0, vICMS: 0 };
    default:
      throw new Error(`Item ${item.nItem}: CST de ICMS "${CST}" ainda não suportado.`);
  }
}

// PIS e COFINS têm a mesma estrutura; muda só o nome do grupo e das tags.
// base: valor do produto − desconto.
function blocoPisCofins(nome, dados, base, nItem) {
  const cst = String(dados?.CST ?? '').padStart(2, '0');
  const p = nome === 'PIS' ? 'pPIS' : 'pCOFINS';
  const v = nome === 'PIS' ? 'vPIS' : 'vCOFINS';
  if (cst === '01' || cst === '02') {
    const aliq = Number(dados[p]);
    exigir(Number.isFinite(aliq), `Item ${nItem}: ${nome} CST ${cst} exige ${p}.`);
    const valor = Math.round((base * aliq) / 100);
    return {
      xml: grupo(nome, grupo(`${nome}Aliq`, tag('CST', cst), tag('vBC', centavos(base)), tag(p, decimal(aliq, 4)), tag(v, centavos(valor)))),
      valor,
    };
  }
  if (['04', '05', '06', '07', '08', '09'].includes(cst)) {
    return { xml: grupo(nome, grupo(`${nome}NT`, tag('CST', cst))), valor: 0 };
  }
  throw new Error(`Item ${nItem}: CST de ${nome} "${dados?.CST}" ainda não suportado.`);
}

function blocoDet(item, nItem) {
  const it = { ...item, nItem };
  exigir(it.xProd, `Item ${nItem}: xProd obrigatório.`);
  exigir(/^\d{8}$/.test(soDigitos(it.NCM) ?? ''), `Item ${nItem}: NCM deve ter 8 dígitos.`);
  exigir(/^\d{4}$/.test(soDigitos(it.CFOP) ?? ''), `Item ${nItem}: CFOP deve ter 4 dígitos.`);
  exigir(Number(it.qCom) > 0, `Item ${nItem}: qCom deve ser maior que zero.`);

  const vProdC = multiplicarEmCentavos(it.qCom, it.vUnCom);
  const vFreteC = paraCentavos(it.vFrete);
  const vSegC = paraCentavos(it.vSeg);
  const vDescC = paraCentavos(it.vDesc);
  const vOutroC = paraCentavos(it.vOutro);
  const cEAN = it.cEAN ?? 'SEM GTIN';
  const opcional = (c) => (c ? centavos(c) : undefined);

  const prod = grupo(
    'prod',
    tag('cProd', texto(it.cProd)),
    tag('cEAN', cEAN),
    tag('xProd', texto(it.xProd)),
    tag('NCM', soDigitos(it.NCM)),
    tag('CEST', soDigitos(it.CEST)),
    tag('cBenef', it.cBenef),
    tag('CFOP', soDigitos(it.CFOP)),
    tag('uCom', texto(it.uCom)),
    tag('qCom', decimal(it.qCom, 4)),
    tag('vUnCom', unitario(it.vUnCom)),
    tag('vProd', centavos(vProdC)),
    tag('cEANTrib', it.cEANTrib ?? cEAN),
    tag('uTrib', texto(it.uTrib ?? it.uCom)),
    tag('qTrib', decimal(it.qTrib ?? it.qCom, 4)),
    tag('vUnTrib', unitario(it.vUnTrib ?? it.vUnCom)),
    tag('vFrete', opcional(vFreteC)),
    tag('vSeg', opcional(vSegC)),
    tag('vDesc', opcional(vDescC)),
    tag('vOutro', opcional(vOutroC)),
    tag('indTot', 1),
  );

  exigir(vDescC <= vProdC, `Item ${nItem}: desconto maior que o valor do produto.`);
  const baseICMS = vProdC - vDescC + vFreteC + vSegC + vOutroC;
  const basePisCofins = vProdC - vDescC;
  const icms = blocoICMS(it, baseICMS);
  const pis = blocoPisCofins('PIS', it.pis, basePisCofins, nItem);
  const cofins = blocoPisCofins('COFINS', it.cofins, basePisCofins, nItem);

  const xml =
    `<det nItem="${nItem}">` +
    prod +
    grupo('imposto', icms.xml, pis.xml, cofins.xml) +
    tag('infAdProd', texto(it.infAdProd)) +
    '</det>';

  return {
    xml,
    totais: {
      vProd: vProdC, vFrete: vFreteC, vSeg: vSegC, vDesc: vDescC, vOutro: vOutroC,
      vBC: icms.vBC, vICMS: icms.vICMS, vPIS: pis.valor, vCOFINS: cofins.valor,
    },
  };
}

function blocoTotal(t) {
  const vNF = t.vProd - t.vDesc + t.vFrete + t.vSeg + t.vOutro;
  const c = (v) => centavos(v);
  return {
    vNF,
    xml: grupo('total', grupo(
      'ICMSTot',
      tag('vBC', c(t.vBC)), tag('vICMS', c(t.vICMS)), tag('vICMSDeson', c(0)),
      tag('vFCP', c(0)), tag('vBCST', c(0)), tag('vST', c(0)), tag('vFCPST', c(0)), tag('vFCPSTRet', c(0)),
      tag('vProd', c(t.vProd)), tag('vFrete', c(t.vFrete)), tag('vSeg', c(t.vSeg)), tag('vDesc', c(t.vDesc)),
      tag('vII', c(0)), tag('vIPI', c(0)), tag('vIPIDevol', c(0)),
      tag('vPIS', c(t.vPIS)), tag('vCOFINS', c(t.vCOFINS)), tag('vOutro', c(t.vOutro)),
      tag('vNF', c(vNF)),
    )),
  };
}

function blocoPag(pagamentos, vNF) {
  const lista = pagamentos?.length ? pagamentos : [{ tPag: '90' }];
  const detalhes = lista.map((p) => {
    const tPag = String(p.tPag).padStart(2, '0');
    // tPag 90 = sem pagamento: vPag deve ser zero. Pagamento único sem valor = valor da nota.
    const vPag = tPag === '90' ? 0 : p.vPag !== undefined ? paraCentavos(p.vPag) : lista.length === 1 ? vNF : undefined;
    exigir(vPag !== undefined, 'Com mais de uma forma de pagamento, informe vPag em cada uma.');
    return grupo('detPag', tag('indPag', p.indPag), tag('tPag', tPag), tag('xPag', tPag === '99' ? texto(p.xPag) : undefined), tag('vPag', centavos(vPag)));
  });
  return grupo('pag', ...detalhes);
}

/**
 * Monta o XML da NF-e (sem assinatura).
 * @param {object} dados  { ide, emit, dest, itens, transp, pag, infAdic, respTec }
 * @param {object} config { cUF, tpAmb }
 * @returns {{ xml: string, chave: string, vNF: string }}
 */
export function gerarXmlNFe(dados, config) {
  exigir(config?.tpAmb === 2, 'Somente homologação (tpAmb=2) é permitida nesta versão.');
  const { ide, emit, dest, itens } = dados;
  exigir(ide && emit && dest, 'Informe ide, emit e dest.');
  exigir(Array.isArray(itens) && itens.length > 0 && itens.length <= 990, 'A nota precisa ter de 1 a 990 itens.');

  const dhEmi = ide.dhEmi ?? dataHora();
  const chave = montarChave({
    cUF: config.cUF, dhEmi, documento: emit.CNPJ ?? emit.CPF, serie: ide.serie, nNF: ide.nNF, cNF: ide.cNF,
  });

  const dets = itens.map((item, i) => blocoDet(item, i + 1));
  const somar = (campo) => dets.reduce((s, d) => s + d.totais[campo], 0);
  const totais = Object.fromEntries(
    ['vProd', 'vFrete', 'vSeg', 'vDesc', 'vOutro', 'vBC', 'vICMS', 'vPIS', 'vCOFINS'].map((k) => [k, somar(k)]),
  );
  const total = blocoTotal(totais);
  exigir(total.vNF >= 0, 'Valor total da nota ficou negativo.');

  const respTec = dados.respTec;
  const infNFe =
    `<infNFe Id="NFe${chave.chave}" versao="4.00">` +
    blocoIde({ ...ide, dhEmi }, config, chave) +
    blocoEmit(emit) +
    blocoDest(dest, config) +
    dets.map((d) => d.xml).join('') +
    total.xml +
    grupo('transp', tag('modFrete', dados.transp?.modFrete ?? 9)) +
    blocoPag(dados.pag, total.vNF) +
    grupo('infAdic', tag('infAdFisco', texto(dados.infAdic?.infAdFisco)), tag('infCpl', texto(dados.infAdic?.infCpl))) +
    (respTec
      ? grupo('infRespTec', tag('CNPJ', soDigitos(respTec.CNPJ)), tag('xContato', texto(respTec.xContato)),
        tag('email', texto(respTec.email)), tag('fone', soDigitos(respTec.fone)))
      : '') +
    '</infNFe>';

  return {
    xml: `<NFe xmlns="http://www.portalfiscal.inf.br/nfe">${infNFe}</NFe>`,
    chave: chave.chave,
    vNF: centavos(total.vNF),
  };
}
