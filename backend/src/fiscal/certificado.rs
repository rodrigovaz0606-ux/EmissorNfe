//! Leitura do certificado A1 (.pfx), em Rust puro.
//! Aceita .pfx modernos (AES) e os legados (3DES/RC2) que a ICP-Brasil ainda usa.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use super::pkcs12::{self, SenhaIncorreta};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::traits::PublicKeyParts;
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::Serialize;
use std::path::Path;
use x509_parser::prelude::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoCertificado {
    pub titular: String,
    pub documento: Option<String>,
    pub tipo: String,
    pub emissor: String,
    pub valido_de: String,
    pub valido_ate: String,
    pub vigente: bool,
}

#[derive(Clone)]
pub struct Certificado {
    pub chave: RsaPrivateKey,
    pub cert_der: Vec<u8>,
    pub cadeia_der: Vec<Vec<u8>>,
    /// .pfx original e senha: usados na conexão TLS com a SEFAZ.
    pub pfx: Vec<u8>,
    pub senha: String,
    pub info: InfoCertificado,
}

impl std::fmt::Debug for Certificado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Nunca imprime chave nem senha.
        f.debug_struct("Certificado").field("info", &self.info).finish_non_exhaustive()
    }
}

pub fn ler_certificado(caminho: Option<&str>, senha: Option<&str>) -> Result<Certificado> {
    let Some(caminho) = caminho.filter(|c| !c.is_empty()) else { bail!("CERT_PFX_PATH não definido no .env") };
    if !Path::new(caminho).exists() {
        bail!("Certificado não encontrado: {caminho}");
    }
    let pfx = std::fs::read(caminho).with_context(|| format!("Erro ao ler {caminho}"))?;
    ler_pfx(&pfx, senha.unwrap_or(""))
}

pub fn ler_pfx(pfx: &[u8], senha: &str) -> Result<Certificado> {
    let conteudo = pkcs12::ler_pfx(pfx, senha).map_err(|e| {
        if e.downcast_ref::<SenhaIncorreta>().is_some() {
            anyhow::anyhow!("Não foi possível abrir o .pfx: senha incorreta.")
        } else {
            anyhow::anyhow!("Não foi possível abrir o .pfx: {e:#}")
        }
    })?;
    let Some(der_chave) = conteudo.chaves.first() else { bail!("O .pfx não contém chave privada.") };
    let chave = RsaPrivateKey::from_pkcs8_der(der_chave).context("Chave privada não é RSA ou está corrompida.")?;

    // O certificado do titular é o que tem a mesma chave pública da chave privada.
    let modulo = chave.n().clone();
    let mesmo_modulo = |der: &[u8]| -> bool {
        X509Certificate::from_der(der)
            .ok()
            .and_then(|(_, c)| RsaPublicKey::from_public_key_der(c.public_key().raw).ok())
            .is_some_and(|p| *p.n() == modulo)
    };
    let certs = conteudo.certificados;
    let Some(pos) = certs.iter().position(|d| mesmo_modulo(d)) else {
        bail!("Certificado correspondente à chave privada não encontrado no .pfx.");
    };
    let cert_der = certs[pos].clone();
    let cadeia_der = certs.iter().enumerate().filter(|(i, _)| *i != pos).map(|(_, d)| d.clone()).collect();
    let info = descrever_certificado(&cert_der)?;
    Ok(Certificado { chave, cert_der, cadeia_der, pfx: pfx.to_vec(), senha: senha.to_string(), info })
}

fn cn(nome: &X509Name) -> String {
    nome.iter_common_name().next().and_then(|c| c.as_str().ok()).unwrap_or("").to_string()
}

fn data_iso(t: ASN1Time) -> (String, DateTime<Utc>) {
    let dt = DateTime::<Utc>::from_timestamp(t.timestamp(), 0).unwrap_or_default();
    (dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(), dt)
}

/// Titular e documento vêm do CN no padrão ICP-Brasil: "NOME:DOCUMENTO".
pub fn descrever_certificado(der: &[u8]) -> Result<InfoCertificado> {
    let (_, c) = X509Certificate::from_der(der).context("Certificado X.509 inválido.")?;
    let cn_titular = cn(c.subject());
    let (titular, documento) = match cn_titular.split_once(':') {
        Some((n, d)) => (n.to_string(), Some(d.to_string())),
        None => (cn_titular.clone(), None),
    };
    let tipo = match documento.as_deref().map(str::len) {
        Some(14) => "e-CNPJ",
        Some(11) => "e-CPF",
        _ => "desconhecido",
    };
    let (valido_de, de) = data_iso(c.validity().not_before);
    let (valido_ate, ate) = data_iso(c.validity().not_after);
    let agora = Utc::now();
    Ok(InfoCertificado {
        titular,
        documento,
        tipo: tipo.into(),
        emissor: cn(c.issuer()),
        valido_de,
        valido_ate,
        vigente: agora >= de && agora <= ate,
    })
}

/// Certificado fictício usado nos testes automatizados (não é de ninguém).
pub fn certificado_de_teste() -> Certificado {
    let pfx = include_bytes!("../../tests/fixtures/teste.pfx");
    ler_pfx(pfx, "teste").expect("certificado de teste")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_pfx_moderno_e_legado() {
        let c = certificado_de_teste();
        assert_eq!(c.info.titular, "EMPRESA TESTE");
        assert_eq!(c.info.documento.as_deref(), Some("12345678000195"));
        assert_eq!(c.info.tipo, "e-CNPJ");
        assert!(c.info.vigente);
        assert!(c.info.valido_ate.ends_with('Z'));

        let legado = ler_pfx(include_bytes!("../../tests/fixtures/teste-legado.pfx"), "teste").unwrap();
        assert_eq!(legado.cert_der, c.cert_der);
        let erro = ler_pfx(include_bytes!("../../tests/fixtures/teste.pfx"), "errada").unwrap_err();
        assert!(erro.to_string().contains("senha incorreta"), "{erro}");
    }

    #[test]
    fn debug_nao_vaza_senha() {
        let c = certificado_de_teste();
        assert!(!format!("{c:?}").contains("teste\""));
    }
}
