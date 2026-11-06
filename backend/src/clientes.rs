//! Cadastro de clientes (destinatários) de cada produtor. O formato é o mesmo do
//! destinatário do formulário, para a interface copiar o cliente direto para a nota.

use crate::emissao::{Ativo, Contexto, ErroNegocio};
use crate::fiscal::documentos::{cnpj_valido, cpf_valido};
use anyhow::Result;
use serde_json::{Value, json};

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

/// Confere e normaliza o cliente recebido da interface. Devolve o JSON a gravar.
pub fn validar_cliente(v: &Value) -> Result<Value> {
    if !v.is_object() {
        return Err(negocio("Dados do cliente ausentes."));
    }
    let pj = texto(v, "tipo") == "PJ";
    let doc = digitos(&texto(v, if pj { "CNPJ" } else { "CPF" }));
    if pj && !cnpj_valido(&doc) {
        return Err(negocio("CNPJ do cliente inválido."));
    }
    if !pj && !cpf_valido(&doc) {
        return Err(negocio("CPF do cliente inválido."));
    }
    let x_nome = texto(v, "xNome").to_uppercase();
    if !(2..=60).contains(&x_nome.chars().count()) {
        return Err(negocio("Nome do cliente: de 2 a 60 caracteres."));
    }
    let ind_ie = match texto(v, "indIEDest").as_str() {
        "" => "9".to_string(),
        c @ ("1" | "2" | "9") => c.to_string(),
        c => return Err(negocio(format!("Indicador de IE inválido: {c}"))),
    };
    let ie = digitos(&texto(v, "IE"));
    if ind_ie == "1" && ie.is_empty() {
        return Err(negocio("Cliente contribuinte precisa da inscrição estadual."));
    }

    let vazio = json!({});
    let e = v.get("ender").unwrap_or(&vazio);
    let c_mun = digitos(&texto(e, "cMun"));
    let uf = texto(e, "UF").to_uppercase();
    for (campo, nome) in [("xLgr", "logradouro"), ("xBairro", "bairro"), ("xMun", "município")] {
        if texto(e, campo).is_empty() {
            return Err(negocio(format!("Endereço do cliente: informe o {nome}.")));
        }
    }
    if c_mun.len() != 7 {
        return Err(negocio("Endereço do cliente: código IBGE do município com 7 dígitos."));
    }
    if uf.len() != 2 {
        return Err(negocio("Endereço do cliente: informe a UF."));
    }

    Ok(json!({
        "tipo": if pj { "PJ" } else { "PF" },
        "CPF": if pj { String::new() } else { doc.clone() },
        "CNPJ": if pj { doc } else { String::new() },
        "xNome": x_nome,
        "indIEDest": ind_ie,
        "IE": if ind_ie == "1" { ie } else { String::new() },
        "email": texto(v, "email"),
        "ender": {
            "CEP": digitos(&texto(e, "CEP")),
            "xLgr": texto(e, "xLgr"),
            "nro": texto(e, "nro"),
            "xCpl": texto(e, "xCpl"),
            "xBairro": texto(e, "xBairro"),
            "cMun": c_mun,
            "xMun": texto(e, "xMun"),
            "UF": uf,
            "fone": digitos(&texto(e, "fone")),
        },
    }))
}

/// Cria (id = None) ou altera um cliente. O CPF/CNPJ não pode repetir no mesmo produtor.
pub fn salvar_cliente(id: Option<i64>, dados: &Value, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let mut c = validar_cliente(dados)?;
    let doc = [c["CPF"].as_str(), c["CNPJ"].as_str()].into_iter().flatten().collect::<String>();
    let x_nome = c["xNome"].as_str().unwrap_or_default().to_string();
    let repo = ctx.repo();
    if repo.cliente_por_documento(ativo.id, &doc)?.is_some_and(|o| Some(o) != id) {
        return Err(negocio(format!("Já existe um cliente com o documento {doc}.")));
    }
    let json = c.to_string();
    let id = match id {
        Some(id) if repo.atualizar_cliente(ativo.id, id, &doc, &x_nome, &json)? => id,
        Some(_) => return Err(negocio("Cliente não encontrado.")),
        None => repo.inserir_cliente(ativo.id, &doc, &x_nome, &json)?,
    };
    c["id"] = Value::from(id);
    Ok(c)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn base() -> Value {
        json!({ "tipo": "PF", "CPF": "529.982.247-25", "xNome": "joao", "indIEDest": "9",
                "ender": { "xLgr": "RUA A", "xBairro": "CENTRO", "cMun": "5208707", "xMun": "GOIANIA", "UF": "go", "CEP": "74000-000" } })
    }

    #[test]
    fn normaliza() {
        let c = validar_cliente(&base()).unwrap();
        assert_eq!(c["CPF"], "52998224725");
        assert_eq!(c["xNome"], "JOAO");
        assert_eq!(c["ender"]["UF"], "GO");
        assert_eq!(c["ender"]["CEP"], "74000000");
    }

    #[test]
    fn recusa_invalidos() {
        let erro = |ajuste: fn(&mut Value)| {
            let mut v = base();
            ajuste(&mut v);
            validar_cliente(&v).unwrap_err().to_string()
        };
        assert!(erro(|v| v["CPF"] = json!("111")).contains("CPF"));
        assert!(erro(|v| v["tipo"] = json!("PJ")).contains("CNPJ"));
        assert!(erro(|v| v["xNome"] = json!("")).contains("Nome"));
        assert!(erro(|v| v["indIEDest"] = json!("1")).contains("inscrição"));
        assert!(erro(|v| v["ender"]["cMun"] = json!("12")).contains("IBGE"));
        assert!(erro(|v| v["ender"]["xLgr"] = json!("")).contains("logradouro"));
    }
}
