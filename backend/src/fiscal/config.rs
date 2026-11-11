//! Configuração fiscal: UF, ambiente e endereços dos webservices.
//!
//! O padrão é HOMOLOGAÇÃO (tpAmb=2, sem valor fiscal). Produção (tpAmb=1) só é usada quando
//! o .env diz explicitamente `NFE_AMBIENTE=producao`: notas emitidas assim têm valor fiscal.
//! Em produção a conexão confere o certificado do servidor da SEFAZ (ver `soap.rs`).

use anyhow::{Result, bail};

pub const AMBIENTE_PRODUCAO: u8 = 1;
pub const AMBIENTE_HOMOLOGACAO: u8 = 2;

/// Código IBGE da UF (cUF).
const CODIGOS_UF: &[(&str, u8)] = &[("GO", 52)];

/// Webservices NF-e 4.00: (UF, tpAmb, serviço, URL). Os de produção são os publicados no
/// Portal Nacional da NF-e (relação de webservices) para a SEFAZ-GO.
const WEBSERVICES: &[(&str, u8, &str, &str)] = &[
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeStatusServico4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeStatusServico4"),
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeAutorizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeAutorizacao4"),
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeRetAutorizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4"),
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeConsultaProtocolo4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeConsultaProtocolo4"),
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeRecepcaoEvento4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeRecepcaoEvento4"),
    ("GO", AMBIENTE_HOMOLOGACAO, "NFeInutilizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeInutilizacao4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeStatusServico4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeStatusServico4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeAutorizacao4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeAutorizacao4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeRetAutorizacao4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeConsultaProtocolo4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeConsultaProtocolo4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeRecepcaoEvento4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeRecepcaoEvento4"),
    ("GO", AMBIENTE_PRODUCAO, "NFeInutilizacao4", "https://nfe.sefaz.go.gov.br/nfe/services/NFeInutilizacao4"),
];

#[derive(Debug, Clone)]
pub struct Config {
    pub uf: String,
    pub c_uf: u8,
    pub tp_amb: u8,
    pub cert_path: Option<String>,
    pub cert_senha: Option<String>,
}

impl Config {
    /// Configuração de homologação para testes (sem certificado).
    pub fn teste() -> Self {
        Config { uf: "GO".into(), c_uf: 52, tp_amb: AMBIENTE_HOMOLOGACAO, cert_path: None, cert_senha: None }
    }

    pub fn producao(&self) -> bool {
        self.tp_amb == AMBIENTE_PRODUCAO
    }

    /// "producao" ou "homologacao" (para a interface e as mensagens).
    pub fn nome_ambiente(&self) -> &'static str {
        if self.producao() { "producao" } else { "homologacao" }
    }
}

/// Lê a configuração de uma função de ambiente (normalmente `std::env::var`).
pub fn carregar_config(env: impl Fn(&str) -> Option<String>) -> Result<Config> {
    let ambiente = env("NFE_AMBIENTE").unwrap_or_else(|| "homologacao".into()).trim().to_lowercase();
    let tp_amb = match ambiente.as_str() {
        "homologacao" | "homologação" => AMBIENTE_HOMOLOGACAO,
        "producao" | "produção" => AMBIENTE_PRODUCAO,
        outro => bail!("NFE_AMBIENTE=\"{outro}\" inválido: use \"homologacao\" ou \"producao\"."),
    };
    let uf = env("NFE_UF").unwrap_or_default().to_uppercase();
    let Some(&(_, c_uf)) = CODIGOS_UF.iter().find(|(u, _)| *u == uf) else {
        let ufs: Vec<_> = CODIGOS_UF.iter().map(|(u, _)| *u).collect();
        bail!("NFE_UF=\"{uf}\" não suportada. UFs mapeadas: {}", ufs.join(", "));
    };
    Ok(Config { uf, c_uf, tp_amb, cert_path: env("CERT_PFX_PATH"), cert_senha: env("CERT_PFX_SENHA") })
}

pub fn url_servico(config: &Config, servico: &str) -> Result<&'static str> {
    WEBSERVICES
        .iter()
        .find(|(u, amb, s, _)| *u == config.uf && *amb == config.tp_amb && *s == servico)
        .map(|(_, _, _, url)| *url)
        .ok_or_else(|| anyhow::anyhow!("Serviço {servico} não mapeado para {} ({}).", config.uf, config.nome_ambiente()))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn env<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pares.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn homologacao_e_o_padrao_e_producao_so_quando_pedida() {
        let padrao = carregar_config(env(&[("NFE_UF", "go")])).unwrap();
        assert_eq!((padrao.tp_amb, padrao.c_uf, padrao.producao()), (2, 52, false));
        assert!(url_servico(&padrao, "NFeAutorizacao4").unwrap().starts_with("https://homolog.sefaz.go.gov.br/"));

        let prod = carregar_config(env(&[("NFE_AMBIENTE", "Producao"), ("NFE_UF", "GO")])).unwrap();
        assert_eq!((prod.tp_amb, prod.producao(), prod.nome_ambiente()), (1, true, "producao"));
        assert_eq!(url_servico(&prod, "NFeAutorizacao4").unwrap(), "https://nfe.sefaz.go.gov.br/nfe/services/NFeAutorizacao4");
        for s in ["NFeStatusServico4", "NFeRetAutorizacao4", "NFeConsultaProtocolo4", "NFeRecepcaoEvento4", "NFeInutilizacao4"] {
            assert!(url_servico(&prod, s).unwrap().starts_with("https://nfe.sefaz.go.gov.br/nfe/services/"), "{s}");
        }

        assert!(carregar_config(env(&[("NFE_AMBIENTE", "prod"), ("NFE_UF", "GO")])).unwrap_err().to_string().contains("inválido"));
        assert!(carregar_config(env(&[("NFE_UF", "SP")])).is_err());
    }
}
