//! Cadastro de produtos: dados que se repetem nas notas (código, descrição, NCM, CFOP,
//! unidade, preço e tributação). O formato é o mesmo de um item do formulário da nota,
//! para a interface copiar o produto direto para o item.

use crate::emissao::{Ativo, Contexto, ErroNegocio};
use anyhow::Result;
use serde_json::{Map, Value, json};

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

fn texto(v: &Value, campo: &str) -> String {
    match v.get(campo) {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn digitos(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// Número digitado como na tela: "1.234,56" ou "1234.56".
pub fn ler_numero(s: &str) -> Option<f64> {
    let s = s.trim();
    let normal = if s.contains(',') { s.replace('.', "").replace(',', ".") } else { s.to_string() };
    normal.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn opcao(valor: String, validas: &[&str], padrao: &str, nome: &str) -> Result<String> {
    if valor.is_empty() {
        return Ok(padrao.into());
    }
    if validas.contains(&valor.as_str()) {
        Ok(valor)
    } else {
        Err(negocio(format!("{nome} inválido: {valor}")))
    }
}

fn aliquota(v: &Value, campo: &str, exigir: bool, nome: &str) -> Result<String> {
    let s = texto(v, campo);
    if s.is_empty() {
        return if exigir { Err(negocio(format!("Informe a alíquota de {nome}."))) } else { Ok(s) };
    }
    match ler_numero(&s) {
        Some(n) if (0.0..=100.0).contains(&n) => Ok(s),
        _ => Err(negocio(format!("Alíquota de {nome} inválida: {s}"))),
    }
}

const CST_ICMS: &[&str] = &["00", "40", "41", "50", "51"];
const CST_PIS_COFINS: &[&str] = &["01", "02", "04", "05", "06", "07", "08", "09"];
const ORIGENS: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8"];

/// Confere e normaliza o produto recebido da interface. Devolve o JSON a gravar.
pub fn validar_produto(v: &Value) -> Result<Value> {
    if !v.is_object() {
        return Err(negocio("Dados do produto ausentes."));
    }
    let c_prod = texto(v, "cProd");
    let x_prod = texto(v, "xProd").to_uppercase();
    let ncm = digitos(&texto(v, "NCM"));
    let cfop = digitos(&texto(v, "CFOP"));
    let u_com = texto(v, "uCom").to_uppercase();
    let v_un_com = texto(v, "vUnCom");

    if c_prod.is_empty() || c_prod.chars().count() > 60 {
        return Err(negocio("Código do produto: obrigatório, até 60 caracteres."));
    }
    if x_prod.is_empty() || x_prod.chars().count() > 120 {
        return Err(negocio("Descrição do produto: obrigatória, até 120 caracteres."));
    }
    if ncm.len() != 8 {
        return Err(negocio("NCM deve ter 8 dígitos."));
    }
    if cfop.len() != 4 {
        return Err(negocio("CFOP deve ter 4 dígitos."));
    }
    if u_com.is_empty() || u_com.chars().count() > 6 {
        return Err(negocio("Unidade: obrigatória, até 6 caracteres."));
    }
    if !v_un_com.is_empty() && !ler_numero(&v_un_com).is_some_and(|n| n >= 0.0) {
        return Err(negocio(format!("Preço unitário inválido: {v_un_com}")));
    }

    let vazio = Value::Object(Map::new());
    let icms = v.get("icms").unwrap_or(&vazio);
    let pis = v.get("pis").unwrap_or(&vazio);
    let cofins = v.get("cofins").unwrap_or(&vazio);

    let cst_icms = opcao(texto(icms, "CST"), CST_ICMS, "51", "CST do ICMS")?;
    let cst_pis = opcao(texto(pis, "CST"), CST_PIS_COFINS, "08", "CST do PIS")?;
    let cst_cofins = opcao(texto(cofins, "CST"), CST_PIS_COFINS, "08", "CST da COFINS")?;
    let tributado = |cst: &str| cst == "01" || cst == "02";

    Ok(json!({
        "cProd": c_prod,
        "xProd": x_prod,
        "NCM": ncm,
        "CFOP": cfop,
        "uCom": u_com,
        "vUnCom": v_un_com,
        "icms": {
            "orig": opcao(texto(icms, "orig"), ORIGENS, "0", "Origem da mercadoria")?,
            "pICMS": aliquota(icms, "pICMS", cst_icms == "00", "ICMS")?,
            "CST": cst_icms,
        },
        "pis": { "pPIS": aliquota(pis, "pPIS", tributado(&cst_pis), "PIS")?, "CST": cst_pis },
        "cofins": { "pCOFINS": aliquota(cofins, "pCOFINS", tributado(&cst_cofins), "COFINS")?, "CST": cst_cofins },
    }))
}

/// Cria (id = None) ou altera um produto. O código não pode repetir.
pub fn salvar_produto(id: Option<i64>, dados: &Value, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let mut p = validar_produto(dados)?;
    let c_prod = p["cProd"].as_str().unwrap_or_default().to_string();
    let x_prod = p["xProd"].as_str().unwrap_or_default().to_string();
    let repo = ctx.repo();
    if let Some(outro) = repo.produto_por_codigo(ativo.id, &c_prod)?.filter(|o| Some(*o) != id) {
        return Err(negocio(format!("Já existe um produto com o código {c_prod} (cadastro nº {outro}).")));
    }
    let json = p.to_string();
    let id = match id {
        Some(id) if repo.atualizar_produto(ativo.id, id, &c_prod, &x_prod, &json)? => id,
        Some(_) => return Err(negocio("Produto não encontrado.")),
        None => repo.inserir_produto(ativo.id, &c_prod, &x_prod, &json)?,
    };
    p["id"] = Value::from(id);
    Ok(p)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn soja() -> Value {
        json!({ "cProd": " SOJA01 ", "xProd": "soja em graos", "NCM": "1201.90.00", "CFOP": "5101", "uCom": "sc", "vUnCom": "132,50" })
    }

    #[test]
    fn normaliza_e_aplica_padroes() {
        let p = validar_produto(&soja()).unwrap();
        assert_eq!(p["cProd"], "SOJA01");
        assert_eq!(p["xProd"], "SOJA EM GRAOS");
        assert_eq!(p["NCM"], "12019000");
        assert_eq!(p["uCom"], "SC");
        assert_eq!(p["icms"], json!({ "orig": "0", "CST": "51", "pICMS": "" }));
        assert_eq!(p["pis"]["CST"], "08");
    }

    #[test]
    fn recusa_dados_invalidos() {
        let erro = |ajuste: fn(&mut Value)| {
            let mut v = soja();
            ajuste(&mut v);
            validar_produto(&v).unwrap_err().to_string()
        };
        assert!(erro(|v| v["cProd"] = json!("")).contains("Código"));
        assert!(erro(|v| v["NCM"] = json!("1201")).contains("NCM"));
        assert!(erro(|v| v["CFOP"] = json!("51")).contains("CFOP"));
        assert!(erro(|v| v["vUnCom"] = json!("abc")).contains("Preço"));
        assert!(erro(|v| v["icms"] = json!({ "CST": "00" })).contains("alíquota de ICMS"));
        assert!(erro(|v| v["icms"] = json!({ "CST": "99" })).contains("CST do ICMS"));
        assert!(erro(|v| v["pis"] = json!({ "CST": "01", "pPIS": "150" })).contains("PIS"));
    }

    #[test]
    fn numeros_como_na_tela() {
        assert_eq!(ler_numero("1.234,56"), Some(1234.56));
        assert_eq!(ler_numero("58.4"), Some(58.4));
        assert_eq!(ler_numero("x"), None);
    }
}
