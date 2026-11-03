// Configuração fiscal: UF, ambiente e endereços dos webservices.
//
// TRAVA DE SEGURANÇA: só o ambiente de homologação (tpAmb=2) é permitido.
// Os endereços de produção não estão cadastrados de propósito; habilitar
// produção exige mudar este arquivo conscientemente.

export const AMBIENTE_HOMOLOGACAO = 2;

// Código IBGE da UF (cUF)
export const CODIGOS_UF = {
  GO: 52,
};

// Webservices NF-e 4.00 por UF, somente homologação.
const WEBSERVICES_HOMOLOGACAO = {
  GO: {
    NFeStatusServico4: 'https://homolog.sefaz.go.gov.br/nfe/services/NFeStatusServico4',
    NFeAutorizacao4: 'https://homolog.sefaz.go.gov.br/nfe/services/NFeAutorizacao4',
    NFeRetAutorizacao4: 'https://homolog.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4',
  },
};

export function carregarConfig(env = process.env) {
  const ambiente = (env.NFE_AMBIENTE || 'homologacao').toLowerCase();
  if (ambiente !== 'homologacao') {
    throw new Error(
      `NFE_AMBIENTE="${ambiente}" não é permitido. Esta versão só opera em homologação.`,
    );
  }

  const uf = (env.NFE_UF || '').toUpperCase();
  if (!CODIGOS_UF[uf]) {
    throw new Error(`NFE_UF="${uf}" não suportada. UFs mapeadas: ${Object.keys(CODIGOS_UF).join(', ')}`);
  }

  return {
    uf,
    cUF: CODIGOS_UF[uf],
    tpAmb: AMBIENTE_HOMOLOGACAO,
    certPath: env.CERT_PFX_PATH,
    certSenha: env.CERT_PFX_SENHA,
  };
}

export function urlServico(uf, servico) {
  const url = WEBSERVICES_HOMOLOGACAO[uf]?.[servico];
  if (!url) throw new Error(`Serviço ${servico} não mapeado para ${uf} (homologação).`);
  return url;
}
