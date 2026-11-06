// Tabelas do leiaute da NF-e usadas nos selects.

export const UFS = ['AC', 'AL', 'AM', 'AP', 'BA', 'CE', 'DF', 'ES', 'GO', 'MA', 'MG', 'MS', 'MT', 'PA', 'PB', 'PE', 'PI', 'PR', 'RJ', 'RN', 'RO', 'RR', 'RS', 'SC', 'SE', 'SP', 'TO'];

export const NATUREZAS = [
  'VENDA DE PRODUCAO DO ESTABELECIMENTO',
  'VENDA DE MERCADORIA ADQUIRIDA DE TERCEIROS',
  'REMESSA PARA DEPOSITO FECHADO OU ARMAZEM GERAL',
  'REMESSA PARA INDUSTRIALIZACAO',
  'DEVOLUCAO DE COMPRA',
  'DEVOLUCAO DE VENDA',
  'COMPRA PARA PRODUCAO RURAL',
  'OUTRA SAIDA NAO ESPECIFICADA',
];

export const CFOPS = [
  ['5101', 'Venda de produção do estabelecimento'],
  ['5102', 'Venda de mercadoria adquirida de terceiros'],
  ['5201', 'Devolução de compra para produção rural ou industrialização'],
  ['5901', 'Remessa para industrialização'],
  ['5905', 'Remessa para depósito fechado ou armazém geral'],
  ['5949', 'Outra saída não especificada'],
  ['6101', 'Venda de produção (interestadual)'],
  ['6102', 'Venda de mercadoria de terceiros (interestadual)'],
  ['6201', 'Devolução de compra (interestadual)'],
  ['6901', 'Remessa para industrialização (interestadual)'],
  ['6905', 'Remessa para depósito (interestadual)'],
  ['6949', 'Outra saída (interestadual)'],
  ['1101', 'Compra para produção rural ou industrialização'],
  ['2101', 'Compra para produção rural (interestadual)'],
  ['1201', 'Entrada: devolução de venda de produção própria'],
  ['2201', 'Entrada: devolução de venda (interestadual)'],
];

// Modelos de operação: preenchem natureza, tipo, finalidade, CFOP (interno, interestadual) e pagamento.
// São um ponto de partida: o CFOP e a tributação de cada operação devem ser conferidos com o contador.
export const TIPOS_NOTA = [
  { id: 'venda_producao', rotulo: 'Venda de produção própria', natOp: 'VENDA DE PRODUCAO DO ESTABELECIMENTO', tpNF: '1', finNFe: '1', cfop: ['5101', '6101'], venda: true },
  { id: 'venda_terceiros', rotulo: 'Venda de mercadoria de terceiros', natOp: 'VENDA DE MERCADORIA ADQUIRIDA DE TERCEIROS', tpNF: '1', finNFe: '1', cfop: ['5102', '6102'], venda: true },
  { id: 'remessa_deposito', rotulo: 'Remessa para depósito ou armazém', natOp: 'REMESSA PARA DEPOSITO FECHADO OU ARMAZEM GERAL', tpNF: '1', finNFe: '1', cfop: ['5905', '6905'], semPagamento: true },
  { id: 'remessa_industrializacao', rotulo: 'Remessa para industrialização', natOp: 'REMESSA PARA INDUSTRIALIZACAO', tpNF: '1', finNFe: '1', cfop: ['5901', '6901'], semPagamento: true },
  { id: 'devolucao_compra', rotulo: 'Devolução de compra (saída)', natOp: 'DEVOLUCAO DE COMPRA', tpNF: '1', finNFe: '4', cfop: ['5201', '6201'], semPagamento: true },
  { id: 'entrada_compra', rotulo: 'Entrada: compra para produção rural', natOp: 'COMPRA PARA PRODUCAO RURAL', tpNF: '0', finNFe: '1', cfop: ['1101', '2101'] },
  { id: 'entrada_devolucao', rotulo: 'Entrada: devolução de venda', natOp: 'DEVOLUCAO DE VENDA', tpNF: '0', finNFe: '4', cfop: ['1201', '2201'], semPagamento: true },
];

export const UNIDADES = ['UN', 'KG', 'SC', 'TON', 'CX', 'L', 'M', 'M3', 'CB'];

export const TP_NF = [['1', 'Saída'], ['0', 'Entrada']];
export const FIN_NFE = [['1', 'Normal'], ['2', 'Complementar'], ['3', 'Ajuste'], ['4', 'Devolução']];
export const IND_FINAL = [['1', 'Sim, consumidor final'], ['0', 'Não (revenda/industrialização)']];
export const IND_PRES = [
  ['1', 'Presencial'], ['2', 'Pela internet'], ['3', 'Teleatendimento'], ['4', 'Entrega em domicílio'], ['9', 'Outros (não presencial)'],
];
export const CRT = [['3', 'Regime normal'], ['1', 'Simples Nacional'], ['2', 'Simples — excesso de sublimite'], ['4', 'MEI']];
export const IND_IE_DEST = [['1', 'Contribuinte do ICMS'], ['2', 'Isento de inscrição'], ['9', 'Não contribuinte']];

export const ORIGENS = [
  ['0', '0 — Nacional'], ['1', '1 — Estrangeira, importação direta'], ['2', '2 — Estrangeira, mercado interno'],
];
export const CST_ICMS = [
  ['00', '00 — Tributada integralmente'],
  ['40', '40 — Isenta'],
  ['41', '41 — Não tributada'],
  ['50', '50 — Suspensão'],
  ['51', '51 — Diferimento'],
];
export const CST_PIS_COFINS = [
  ['01', '01 — Tributável (alíquota básica)'],
  ['02', '02 — Tributável (alíquota diferenciada)'],
  ['04', '04 — Monofásica, alíquota zero'],
  ['05', '05 — Substituição tributária'],
  ['06', '06 — Alíquota zero'],
  ['07', '07 — Isenta'],
  ['08', '08 — Sem incidência'],
  ['09', '09 — Suspensão'],
];

export const MOD_FRETE = [
  ['9', 'Sem frete'], ['0', 'Por conta do emitente (CIF)'], ['1', 'Por conta do destinatário (FOB)'],
  ['2', 'Por conta de terceiros'], ['3', 'Transporte próprio do emitente'], ['4', 'Transporte próprio do destinatário'],
];
export const T_PAG = [
  ['01', 'Dinheiro'], ['02', 'Cheque'], ['03', 'Cartão de crédito'], ['04', 'Cartão de débito'], ['15', 'Boleto'],
  ['16', 'Depósito bancário'], ['17', 'PIX'], ['18', 'Transferência bancária'], ['90', 'Sem pagamento'],
];
