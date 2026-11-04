//! Assinatura XMLDSig no padrão exigido pela SEFAZ (MOC 4.00):
//!   - assinatura "enveloped" referenciando o elemento pelo atributo Id
//!   - canonicalização C14N 1.0 (sem comentários)
//!   - transforms: enveloped-signature + C14N
//!   - digest SHA-1 e assinatura RSA-SHA1
//!   - KeyInfo com o certificado X509 do emitente
//!   - <Signature> sem prefixo, logo após o elemento assinado
//!
//! O mesmo esquema serve para NF-e (infNFe), eventos (infEvento) e inutilização (infInut).

use super::certificado::Certificado;
use super::xml::{NS_DSIG, c14n, encontrar, filho, ler, texto_de};
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use roxmltree::Node;
use rsa::RsaPublicKey;
use rsa::pkcs1v15::{Signature as AssinaturaRsa, SigningKey, VerifyingKey};
use rsa::pkcs8::DecodePublicKey;
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use serde::Serialize;
use sha1::{Digest, Sha1};
use x509_parser::prelude::*;

const C14N: &str = "http://www.w3.org/TR/2001/REC-xml-c14n-20010315";
const ENVELOPED: &str = "http://www.w3.org/2000/09/xmldsig#enveloped-signature";
const RSA_SHA1: &str = "http://www.w3.org/2000/09/xmldsig#rsa-sha1";
const SHA1: &str = "http://www.w3.org/2000/09/xmldsig#sha1";

fn e_assinatura(n: Node) -> bool {
    n.is_element() && n.tag_name().name() == "Signature" && n.tag_name().namespace() == Some(NS_DSIG)
}

fn sha1_b64(dados: &[u8]) -> String {
    B64.encode(Sha1::digest(dados))
}

pub fn assinar_xml(xml: &str, certificado: &Certificado, elemento: &str) -> Result<String> {
    let doc = ler(xml).context("XML inválido para assinatura")?;
    let alvo = encontrar(doc.root(), elemento).with_context(|| format!("Elemento {elemento} não encontrado"))?;
    let id = alvo.attribute("Id").with_context(|| format!("{elemento} sem atributo Id"))?.to_string();
    let digest = sha1_b64(c14n(alvo, xml, &e_assinatura).as_bytes());
    let fim = alvo.range().end;

    let inicio_sig = format!(
        "<Signature xmlns=\"{NS_DSIG}\"><SignedInfo><CanonicalizationMethod Algorithm=\"{C14N}\"/>\
         <SignatureMethod Algorithm=\"{RSA_SHA1}\"/><Reference URI=\"#{id}\"><Transforms>\
         <Transform Algorithm=\"{ENVELOPED}\"/><Transform Algorithm=\"{C14N}\"/></Transforms>\
         <DigestMethod Algorithm=\"{SHA1}\"/><DigestValue>{digest}</DigestValue></Reference></SignedInfo><SignatureValue>"
    );
    let fim_sig = format!(
        "</SignatureValue><KeyInfo><X509Data><X509Certificate>{}</X509Certificate></X509Data></KeyInfo></Signature>",
        B64.encode(&certificado.cert_der)
    );

    // O SignedInfo é canonicalizado no contexto final (dentro do documento),
    // para herdar corretamente os namespaces em escopo.
    let provisorio = format!("{}{inicio_sig}{fim_sig}{}", &xml[..fim], &xml[fim..]);
    let doc2 = ler(&provisorio)?;
    let signed_info = doc2
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "SignedInfo" && n.parent().is_some_and(e_assinatura))
        .context("SignedInfo não encontrado")?;
    let canonico = c14n(signed_info, &provisorio, &|_| false);
    let assinador = SigningKey::<Sha1>::new(certificado.chave.clone());
    let valor = B64.encode(assinador.sign(canonico.as_bytes()).to_bytes());

    Ok(format!("{}{inicio_sig}{valor}{fim_sig}{}", &xml[..fim], &xml[fim..]))
}

pub fn assinar_nfe(xml: &str, certificado: &Certificado) -> Result<String> {
    assinar_xml(xml, certificado, "infNFe")
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Verificacao {
    pub valida: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motivo: Option<String>,
}

/// Confere uma assinatura usando o certificado embutido no próprio XML (KeyInfo).
pub fn verificar_assinatura(xml: &str) -> Verificacao {
    match verificar(xml) {
        Ok(()) => Verificacao { valida: true, motivo: None },
        Err(e) => Verificacao { valida: false, motivo: Some(e.to_string()) },
    }
}

fn limpar_b64(s: &str) -> Result<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    Ok(B64.decode(s)?)
}

fn verificar(xml: &str) -> Result<()> {
    let doc = ler(xml)?;
    let sig = doc.descendants().find(|n| e_assinatura(*n)).context("Signature não encontrada")?;
    let si = filho(sig, "SignedInfo").context("SignedInfo não encontrado")?;
    fn algoritmo<'a>(no: Option<Node<'a, '_>>) -> &'a str {
        no.and_then(|n| n.attribute("Algorithm")).unwrap_or("")
    }
    if algoritmo(filho(si, "CanonicalizationMethod")) != C14N || algoritmo(filho(si, "SignatureMethod")) != RSA_SHA1 {
        bail!("Algoritmos de assinatura não suportados");
    }
    let refe = filho(si, "Reference").context("Reference não encontrado")?;
    if algoritmo(filho(refe, "DigestMethod")) != SHA1 {
        bail!("Algoritmo de digest não suportado");
    }
    let uri = refe.attribute("URI").unwrap_or("").trim_start_matches('#');
    let alvo = doc
        .descendants()
        .find(|n| n.is_element() && n.attribute("Id") == Some(uri))
        .with_context(|| format!("Elemento referenciado (#{uri}) não encontrado"))?;
    let digest_xml = filho(refe, "DigestValue").map(texto_de).unwrap_or_default();
    let digest = sha1_b64(c14n(alvo, xml, &e_assinatura).as_bytes());
    if digest_xml.trim() != digest {
        bail!("DigestValue não confere (conteúdo alterado)");
    }

    let cert_b64 = encontrar(sig, "X509Certificate").map(texto_de).context("X509Certificate não encontrado")?;
    let cert_der = limpar_b64(&cert_b64)?;
    let (_, cert) = X509Certificate::from_der(&cert_der).map_err(|_| anyhow::anyhow!("X509Certificate inválido"))?;
    let publica = RsaPublicKey::from_public_key_der(cert.public_key().raw).context("Chave pública não é RSA")?;
    let valor = limpar_b64(&filho(sig, "SignatureValue").map(texto_de).unwrap_or_default())?;
    let assinatura = AssinaturaRsa::try_from(valor.as_slice()).context("SignatureValue inválido")?;
    let canonico = c14n(si, xml, &|_| false);
    VerifyingKey::<Sha1>::new(publica)
        .verify(canonico.as_bytes(), &assinatura)
        .map_err(|_| anyhow::anyhow!("SignatureValue não confere"))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::fiscal::certificado::certificado_de_teste;

    const XML: &str = "<NFe xmlns=\"http://www.portalfiscal.inf.br/nfe\"><infNFe Id=\"NFe123\" versao=\"4.00\"><vNF>10.00</vNF></infNFe></NFe>";

    #[test]
    fn assina_e_confere() {
        let assinado = assinar_nfe(XML, &certificado_de_teste()).unwrap();
        assert!(assinado.contains("</infNFe><Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><SignedInfo>"));
        assert!(assinado.ends_with("</X509Certificate></X509Data></KeyInfo></Signature></NFe>"));
        assert_eq!(verificar_assinatura(&assinado), Verificacao { valida: true, motivo: None });

        let adulterado = assinado.replace("<vNF>10.00</vNF>", "<vNF>1.00</vNF>");
        let v = verificar_assinatura(&adulterado);
        assert!(!v.valida);
        assert!(v.motivo.unwrap().contains("DigestValue"));
    }

    #[test]
    fn continua_valida_dentro_de_outro_documento() {
        let assinado = assinar_nfe(XML, &certificado_de_teste()).unwrap();
        let lote = format!("<enviNFe xmlns=\"http://www.portalfiscal.inf.br/nfe\" versao=\"4.00\"><idLote>1</idLote>{assinado}</enviNFe>");
        assert!(verificar_assinatura(&lote).valida);
    }
}
