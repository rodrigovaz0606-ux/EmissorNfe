//! Configuração fiscal: UF, ambiente e endereços dos webservices.
//!
//! TRAVA DE SEGURANÇA: só o ambiente de homologação (tpAmb=2) é permitido.
//! Os endereços de produção não estão cadastrados de propósito; habilitar
//! produção exige mudar este arquivo conscientemente.

use anyhow::{Result, bail};

pub const AMBIENTE_HOMOLOGACAO: u8 = 2;

/// Código IBGE da UF (cUF).
const CODIGOS_UF: &[(&str, u8)] = &[("GO", 52)];

/// Webservices NF-e 4.00 por UF, somente homologação.
const WEBSERVICES_HOMOLOGACAO: &[(&str, &str, &str)] = &[
    ("GO", "NFeStatusServico4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeStatusServico4"),
    ("GO", "NFeAutorizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeAutorizacao4"),
    ("GO", "NFeRetAutorizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4"),
    ("GO", "NFeConsultaProtocolo4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeConsultaProtocolo4"),
    ("GO", "NFeRecepcaoEvento4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeRecepcaoEvento4"),
    ("GO", "NFeInutilizacao4", "https://homolog.sefaz.go.gov.br/nfe/services/NFeInutilizacao4"),
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
}

/// Lê a configuração de uma função de ambiente (normalmente `std::env::var`).
pub fn carregar_config(env: impl Fn(&str) -> Option<String>) -> Result<Config> {
    let ambiente = env("NFE_AMBIENTE").unwrap_or_else(|| "homologacao".into()).to_lowercase();
    if ambiente != "homologacao" {
        bail!("NFE_AMBIENTE=\"{ambiente}\" não é permitido. Esta versão só opera em homologação.");
    }
    let uf = env("NFE_UF").unwrap_or_default().to_uppercase();
    let Some(&(_, c_uf)) = CODIGOS_UF.iter().find(|(u, _)| *u == uf) else {
        let ufs: Vec<_> = CODIGOS_UF.iter().map(|(u, _)| *u).collect();
        bail!("NFE_UF=\"{uf}\" não suportada. UFs mapeadas: {}", ufs.join(", "));
    };
    Ok(Config {
        uf,
        c_uf,
        tp_amb: AMBIENTE_HOMOLOGACAO,
        cert_path: env("CERT_PFX_PATH"),
        cert_senha: env("CERT_PFX_SENHA"),
    })
}

pub fn url_servico(uf: &str, servico: &str) -> Result<&'static str> {
    WEBSERVICES_HOMOLOGACAO
        .iter()
        .find(|(u, s, _)| *u == uf && *s == servico)
        .map(|(_, _, url)| *url)
        .ok_or_else(|| anyhow::anyhow!("Serviço {servico} não mapeado para {uf} (homologação)."))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn env<'a>(pares: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| pares.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn recusa_qualquer_ambiente_que_nao_seja_homologacao() {
        let e = carregar_config(env(&[("NFE_AMBIENTE", "producao"), ("NFE_UF", "GO")])).unwrap_err();
        assert!(e.to_string().contains("só opera em homologação"));
        let c = carregar_config(env(&[("NFE_AMBIENTE", "homologacao"), ("NFE_UF", "go")])).unwrap();
        assert_eq!((c.tp_amb, c.c_uf), (2, 52));
        assert!(carregar_config(env(&[("NFE_UF", "SP")])).is_err());
        assert!(url_servico("GO", "NFeAutorizacao4").unwrap().starts_with("https://homolog."));
    }
}
