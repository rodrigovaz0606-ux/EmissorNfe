//! Painel do produtor e exportação para o contador (ZIP com XMLs e resumo em CSV).

use crate::contas::{SessaoAtual, listar_produtores};
use crate::db::{DocumentoExportado, NotaExportada};
use crate::emissao::{Ativo, Contexto, ErroNegocio};
use anyhow::Result;
use chrono::{Datelike, Local, NaiveDate};
use serde_json::{Value, json};
use std::io::{Cursor, Write};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Certificados que vencem em até tantos dias aparecem no painel.
const AVISO_CERTIFICADO_DIAS: i64 = 30;

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

fn data(texto: &str, nome: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(texto, "%Y-%m-%d").map_err(|_| negocio(format!("{nome} inválida: use o formato AAAA-MM-DD.")))
}

/// Primeiro e último dia do mês `meses_atras` antes de `hoje`.
fn limites_do_mes(hoje: NaiveDate, meses_atras: i32) -> (NaiveDate, NaiveDate) {
    let total = hoje.year() * 12 + hoje.month0() as i32 - meses_atras;
    let (ano, mes0) = (total.div_euclid(12), total.rem_euclid(12) as u32);
    let inicio = NaiveDate::from_ymd_opt(ano, mes0 + 1, 1).expect("primeiro dia do mês existe");
    let proximo = if mes0 == 11 { NaiveDate::from_ymd_opt(ano + 1, 1, 1) } else { NaiveDate::from_ymd_opt(ano, mes0 + 2, 1) };
    (inicio, proximo.expect("mês existe").pred_opt().expect("dia anterior existe"))
}

/// Dias até o vencimento (negativo se já venceu). `valido_ate` vem no formato RFC 3339.
fn dias_ate(valido_ate: &str, agora: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    let fim = chrono::DateTime::parse_from_rfc3339(valido_ate).ok()?;
    Some((fim.with_timezone(&chrono::Utc) - agora).num_days())
}

/// Indicadores do mês atual, valor autorizado dos últimos 6 meses e certificados perto de vencer.
pub fn painel(ctx: &Contexto, s: &SessaoAtual, ativo: &Ativo) -> Result<Value> {
    let hoje = Local::now().date_naive();
    let formato = "%Y-%m-%d";
    let (mes, meses) = {
        let repo = ctx.repo();
        let (ini, fim) = limites_do_mes(hoje, 0);
        let mes = repo.resumo_periodo(ativo.id, &ini.format(formato).to_string(), &fim.format(formato).to_string())?;
        let mut meses = vec![];
        for atras in (0..6).rev() {
            let (ini, fim) = limites_do_mes(hoje, atras);
            let r = repo.resumo_periodo(ativo.id, &ini.format(formato).to_string(), &fim.format(formato).to_string())?;
            meses.push(json!({ "mes": ini.format("%Y-%m").to_string(), "valor": r.valor_autorizado, "notas": r.autorizadas }));
        }
        (mes, meses)
    };
    let agora = chrono::Utc::now();
    let mut certificados = vec![];
    for p in listar_produtores(ctx, &s.usuario)? {
        let Some(dias) = p["valido_ate"].as_str().and_then(|v| dias_ate(v, agora)) else { continue };
        if dias <= AVISO_CERTIFICADO_DIAS {
            certificados.push(json!({ "id": p["id"], "titular": p["titular"], "documento": p["documento"], "validoAte": p["valido_ate"], "dias": dias }));
        }
    }
    certificados.sort_by_key(|c| c["dias"].as_i64());
    Ok(json!({ "mes": mes, "meses": meses, "certificados": certificados }))
}

// ---------- exportação ----------

/// Campo de CSV: entre aspas e sem virar fórmula ao abrir no Excel.
fn campo_csv(valor: &str) -> String {
    let seguro = if valor.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{valor}") } else { valor.to_string() };
    format!("\"{}\"", seguro.replace('"', "\"\""))
}

/// CSV para o Excel em português: separador ";", vírgula decimal e BOM para os acentos.
fn resumo_csv(notas: &[NotaExportada]) -> String {
    let mut csv = String::from("\u{feff}Número;Série;Chave de acesso;Data;Situação;Destinatário;CPF/CNPJ;Valor;Protocolo\r\n");
    for n in notas {
        let linha = [
            campo_csv(&n.n_nf.to_string()),
            campo_csv(&n.serie.to_string()),
            campo_csv(&n.chave),
            campo_csv(&n.criada_em),
            campo_csv(&n.status),
            campo_csv(n.dest_nome.as_deref().unwrap_or("")),
            campo_csv(n.dest_doc.as_deref().unwrap_or("")),
            campo_csv(&n.v_nf.replace('.', ",")),
            campo_csv(n.n_prot.as_deref().unwrap_or("")),
        ];
        csv.push_str(&linha.join(";"));
        csv.push_str("\r\n");
    }
    csv
}

/// XMLs do período (notas com valor fiscal, eventos e inutilizações) e um resumo.csv.
pub fn exportar_zip(ctx: &Contexto, ativo: &Ativo, de: &str, ate: &str) -> Result<Vec<u8>> {
    let (ini, fim) = (data(de, "Data inicial")?, data(ate, "Data final")?);
    if fim < ini {
        return Err(negocio("A data final é anterior à inicial."));
    }
    let (notas, eventos, inutilizacoes): (Vec<NotaExportada>, Vec<DocumentoExportado>, Vec<DocumentoExportado>) = {
        let repo = ctx.repo();
        (repo.notas_para_exportar(ativo.id, de, ate)?, repo.eventos_para_exportar(ativo.id, de, ate)?, repo.inutilizacoes_para_exportar(ativo.id, de, ate)?)
    };
    if notas.is_empty() && eventos.is_empty() && inutilizacoes.is_empty() {
        return Err(negocio("Nenhum documento com valor fiscal nesse período."));
    }

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opcoes = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let mut adicionar = |nome: String, conteudo: &[u8]| -> Result<()> {
        zip.start_file(nome, opcoes)?;
        zip.write_all(conteudo)?;
        Ok(())
    };
    adicionar("resumo.csv".into(), resumo_csv(&notas).as_bytes())?;
    for n in &notas {
        let (sufixo, xml) = match &n.xml_proc {
            Some(x) => ("procNFe", x),
            None => ("nfe", &n.xml_nfe),
        };
        adicionar(format!("notas/{}-{sufixo}.xml", n.chave), xml.as_bytes())?;
    }
    for e in &eventos {
        adicionar(format!("eventos/{}", e.nome), e.xml.as_bytes())?;
    }
    for i in &inutilizacoes {
        adicionar(format!("inutilizacoes/{}", i.nome), i.xml.as_bytes())?;
    }
    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn limites_de_mes_atravessam_o_ano() {
        let d = |a, m, dia| NaiveDate::from_ymd_opt(a, m, dia).unwrap();
        assert_eq!(limites_do_mes(d(2026, 3, 31), 0), (d(2026, 3, 1), d(2026, 3, 31)));
        assert_eq!(limites_do_mes(d(2026, 3, 31), 1), (d(2026, 2, 1), d(2026, 2, 28)));
        assert_eq!(limites_do_mes(d(2026, 1, 15), 2), (d(2025, 11, 1), d(2025, 11, 30)));
        assert_eq!(limites_do_mes(d(2026, 7, 1), 7), (d(2025, 12, 1), d(2025, 12, 31)));
    }

    #[test]
    fn csv_nao_vira_formula_e_escapa_aspas() {
        assert_eq!(campo_csv("=SOMA(A1)"), "\"'=SOMA(A1)\"");
        assert_eq!(campo_csv("-1"), "\"'-1\"");
        assert_eq!(campo_csv("A \"B\""), "\"A \"\"B\"\"\"");
        assert_eq!(campo_csv("COMPRADOR"), "\"COMPRADOR\"");
    }

    #[test]
    fn dias_ate_o_vencimento() {
        let agora = chrono::DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(dias_ate("2026-11-11T00:00:00Z", agora), Some(10));
        assert_eq!(dias_ate("2026-10-22T00:00:00Z", agora), Some(-10));
        assert_eq!(dias_ate("lixo", agora), None);
    }
}
