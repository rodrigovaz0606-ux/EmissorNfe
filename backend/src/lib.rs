//! Emissor de NF-e modelo 55 direto na SEFAZ (homologação).
pub mod auth;
pub mod clientes;
pub mod cofre;
pub mod contas;
pub mod danfe;
pub mod db;
pub mod emissao;
pub mod exemplo;
pub mod fiscal;
pub mod produtos;
pub mod relatorios;
pub mod servidor;
pub mod simulada;

use std::path::PathBuf;

/// Pasta raiz do projeto: a primeira, subindo a partir da pasta atual, que tem `schemas/`.
/// (O binário roda tanto da raiz quanto de `backend/`.)
pub fn raiz_do_projeto() -> PathBuf {
    let atual = std::env::current_dir().unwrap_or_default();
    atual
        .ancestors()
        .find(|p| p.join("schemas").is_dir())
        .map(PathBuf::from)
        .unwrap_or(atual)
}


/// Lê um arquivo .env com as mesmas regras do `dotenv` do Node: valores sem aspas são
/// literais (barras invertidas e espaços incluídos), aspas externas são removidas,
/// `#` no início da linha (ou após espaço, fora de aspas) é comentário e variáveis
/// já definidas no ambiente não são sobrescritas.
pub fn carregar_env(caminho: &std::path::Path) {
    let Ok(texto) = std::fs::read_to_string(caminho) else { return };
    for (chave, valor) in ler_env(&texto) {
        if std::env::var_os(&chave).is_none() {
            // SAFETY: chamado no início do main, antes de criar outras threads.
            unsafe { std::env::set_var(chave, valor) };
        }
    }
}

pub fn ler_env(texto: &str) -> Vec<(String, String)> {
    let mut saida = Vec::new();
    for linha in texto.trim_start_matches('\u{feff}').lines() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with('#') {
            continue;
        }
        let Some((chave, valor)) = linha.split_once('=') else { continue };
        let chave = chave.trim().trim_start_matches("export ").trim();
        if chave.is_empty() || !chave.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
            continue;
        }
        let v = valor.trim();
        let aspas = v.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'));
        let valor = match aspas {
            Some(q) if v.len() >= 2 && v[1..].contains(q) => {
                let fim = v[1..].find(q).unwrap() + 1;
                let dentro = &v[1..fim];
                if q == '"' { dentro.replace(r"\n", "\n") } else { dentro.to_string() }
            }
            _ => match v.find(" #") {
                Some(i) => v[..i].trim_end().to_string(),
                None => v.to_string(),
            },
        };
        saida.push((chave.to_string(), valor));
    }
    saida
}

#[cfg(test)]
mod testes_env {
    #[test]
    fn le_env_como_o_node() {
        let t = r#"# comentário
CAMINHO=C:\Pasta Com Espaço\a - b.pfx
SENHA="x#y z"
SIMPLES='a\nb'
UF=GO # comentário
vazio=
DUPLAS="a\nb"
"#;
        let v = super::ler_env(t);
        assert_eq!(v[0], ("CAMINHO".into(), r"C:\Pasta Com Espaço\a - b.pfx".into()));
        assert_eq!(v[1], ("SENHA".into(), "x#y z".into()));
        assert_eq!(v[2], ("SIMPLES".into(), r"a\nb".into()));
        assert_eq!(v[3], ("UF".into(), "GO".into()));
        assert_eq!(v[4], ("vazio".into(), "".into()));
        assert_eq!(v[5], ("DUPLAS".into(), "a\nb".into())); // aspas duplas expandem \n, como no Node
    }
}
