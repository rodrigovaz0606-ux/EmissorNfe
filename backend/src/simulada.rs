//! SEFAZ SIMULADA — só para testes de interface (NFE_SEFAZ_SIMULADA=1).
//! Nada é enviado à SEFAZ; as respostas imitam o formato real com o texto "SIMULADO".

use crate::emissao::Sefaz;
use crate::fiscal::certificado::Certificado;
use crate::fiscal::config::Config;
use crate::fiscal::formatos::agora;
use crate::fiscal::servicos::*;
use anyhow::{Context, Result};
use base64::Engine;
use regex::Regex;
use sha1::{Digest, Sha1};

fn protocolo() -> String {
    let ms = chrono::Utc::now().timestamp_millis().to_string();
    format!("152{}", &ms[ms.len().saturating_sub(12)..])
}

fn envelope(corpo: &str) -> String {
    format!("<soap:Envelope xmlns:soap=\"http://www.w3.org/2003/05/soap-envelope\"><soap:Body>{corpo}</soap:Body></soap:Envelope>")
}

fn capturar(re: &str, texto: &str) -> Result<String> {
    Ok(Regex::new(re)?.captures(texto).context("XML inesperado na SEFAZ simulada")?[1].to_string())
}

pub struct SefazSimulada;

impl Sefaz for SefazSimulada {
    fn simulada(&self) -> bool {
        true
    }

    fn status(&self, _: &Config, _: &Certificado) -> Result<RetStatus> {
        Ok(RetStatus {
            tpAmb: Some("2".into()),
            verAplic: Some("SIMULADOR".into()),
            cStat: Some("107".into()),
            xMotivo: Some("Servico em operacao (SIMULADO)".into()),
            cUF: Some("52".into()),
            dhRecbto: Some(agora()),
            tMed: Some("0".into()),
            emOperacao: true,
        })
    }

    fn autorizar(&self, _: &Config, _: &Certificado, xml: &str) -> Result<RetAutorizacao> {
        let chave = capturar(r#"Id="NFe(\d{44})""#, xml)?;
        let n_prot = protocolo();
        let dh = agora();
        let dig_val = base64::engine::general_purpose::STANDARD.encode(Sha1::digest(xml.as_bytes()));
        let motivo = "Autorizado o uso da NF-e (SIMULADO)";
        let prot = format!(
            "<protNFe versao=\"4.00\"><infProt><tpAmb>2</tpAmb><verAplic>SIMULADOR</verAplic><chNFe>{chave}</chNFe><dhRecbto>{dh}</dhRecbto>\
             <nProt>{n_prot}</nProt><digVal>{dig_val}</digVal><cStat>100</cStat><xMotivo>{motivo}</xMotivo></infProt></protNFe>"
        );
        Ok(RetAutorizacao {
            lote: Lote { cStat: Some("104".into()), xMotivo: Some("Lote processado (SIMULADO)".into()), dhRecbto: Some(dh.clone()), verAplic: None },
            recibo: None,
            protocolo: Some(Protocolo {
                cStat: Some("100".into()),
                xMotivo: Some(motivo.into()),
                chNFe: Some(chave),
                nProt: Some(n_prot),
                dhRecbto: Some(dh),
                digVal: Some(dig_val),
                autorizada: true,
                xml: Some(prot.clone()),
            }),
            enviado: String::new(),
            resposta_soap: envelope(&format!(
                "<retEnviNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><cStat>104</cStat>\
                 <xMotivo>Lote processado (SIMULADO)</xMotivo>{prot}</retEnviNFe>"
            )),
        })
    }

    fn evento(&self, _: &Config, _: &Certificado, env: &str) -> Result<RetEvento> {
        let ch = capturar(r"<chNFe>(\d{44})</chNFe>", env)?;
        let tp = capturar(r"<tpEvento>(\d+)</tpEvento>", env)?;
        let seq = capturar(r"<nSeqEvento>(\d+)</nSeqEvento>", env)?;
        let n_prot = protocolo();
        let dh = agora();
        let motivo = "Evento registrado e vinculado a NF-e (SIMULADO)";
        let ret = format!(
            "<retEvento versao=\"1.00\"><infEvento><tpAmb>2</tpAmb><verAplic>SIMULADOR</verAplic><cOrgao>52</cOrgao><cStat>135</cStat>\
             <xMotivo>{motivo}</xMotivo><chNFe>{ch}</chNFe><tpEvento>{tp}</tpEvento><nSeqEvento>{seq}</nSeqEvento>\
             <dhRegEvento>{dh}</dhRegEvento><nProt>{n_prot}</nProt></infEvento></retEvento>"
        );
        Ok(RetEvento {
            lote: LoteEvento { cStat: Some("128".into()), xMotivo: Some("Lote de evento processado (SIMULADO)".into()) },
            evento: Some(InfRetEvento {
                cStat: Some("135".into()),
                xMotivo: Some(motivo.into()),
                chNFe: Some(ch),
                tpEvento: Some(tp),
                nSeqEvento: Some(seq),
                dhRegEvento: Some(dh),
                nProt: Some(n_prot),
                registrado: true,
            }),
            xml_ret_evento: Some(ret.clone()),
            resposta_soap: envelope(&ret),
        })
    }

    fn inutilizar(&self, _: &Config, _: &Certificado, _: &str) -> Result<RetInut> {
        let n_prot = protocolo();
        let motivo = "Inutilizacao homologada (SIMULADO)";
        let ret = format!(
            "<retInutNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><infInut><tpAmb>2</tpAmb><cStat>102</cStat>\
             <xMotivo>{motivo}</xMotivo><nProt>{n_prot}</nProt></infInut></retInutNFe>"
        );
        Ok(RetInut {
            c_stat: Some("102".into()),
            x_motivo: Some(motivo.into()),
            n_prot: Some(n_prot),
            dh_recbto: None,
            homologada: true,
            xml_ret: Some(ret.clone()),
            resposta_soap: envelope(&ret),
        })
    }

    fn consultar(&self, _: &Config, _: &Certificado, chave: &str) -> Result<RetConsSit> {
        Ok(RetConsSit {
            cStat: Some("100".into()),
            xMotivo: Some("Autorizado o uso da NF-e (SIMULADO)".into()),
            chNFe: Some(chave.into()),
            protocolo: None,
            eventos: vec![],
        })
    }
}
