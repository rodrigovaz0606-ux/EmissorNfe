// Cenário de teste: produtor rural (pessoa física) vende soja em grão dentro de GO.
// O CPF do emitente vem de fora (do certificado) para não ficar gravado no código.
// IE, endereços e destinatário são FICTÍCIOS.

export function vendaSoja({ cpfEmitente, nomeEmitente, nNF = 1, serie = 1 }) {
  return {
    ide: {
      natOp: 'VENDA DE PRODUCAO DO ESTABELECIMENTO',
      serie,
      nNF,
      tpNF: 1,          // saída
      idDest: 1,        // operação interna
      cMunFG: '5208707', // Goiânia
      finNFe: 1,        // normal
      indFinal: 1,      // destinatário não contribuinte
      indPres: 1,       // presencial
    },
    emit: {
      CPF: cpfEmitente,
      xNome: nomeEmitente,
      ender: {
        xLgr: 'RODOVIA GO-060 KM 10 FAZENDA EXEMPLO',
        nro: 'SN',
        xBairro: 'ZONA RURAL',
        cMun: '5208707',
        xMun: 'GOIANIA',
        UF: 'GO',
        CEP: '74000000',
      },
      IE: '109876547', // fictícia, com dígito verificador válido para GO
      CRT: 3,          // regime normal (produtor rural pessoa física)
    },
    dest: {
      CPF: '12345678909', // fictício (DV válido)
      xNome: 'COMPRADOR TESTE', // em homologação é trocado pelo texto obrigatório
      indIEDest: 9,
      ender: {
        xLgr: 'AVENIDA TESTE',
        nro: '100',
        xBairro: 'CENTRO',
        cMun: '5208707',
        xMun: 'GOIANIA',
        UF: 'GO',
        CEP: '74000000',
      },
    },
    itens: [
      {
        cProd: 'SOJA01',
        xProd: 'SOJA EM GRAOS',
        NCM: '12019000',
        CFOP: '5101',
        uCom: 'SC',
        qCom: 500,
        vUnCom: 132.5,
        icms: { orig: 0, CST: '51' }, // diferimento
        pis: { CST: '08' },
        cofins: { CST: '08' },
      },
    ],
    transp: { modFrete: 9 },
    pag: [{ indPag: 0, tPag: '01' }],
    infAdic: { infCpl: 'NOTA DE TESTE EMITIDA EM HOMOLOGACAO. ICMS DIFERIDO.' },
  };
}
