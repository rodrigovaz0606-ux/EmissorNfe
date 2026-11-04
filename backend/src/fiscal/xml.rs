//! Utilitários de XML: leitura das respostas da SEFAZ e canonicalização C14N 1.0.

use regex::Regex;
use roxmltree::{Document, Node, ParsingOptions};
use std::collections::BTreeMap;

pub const NS_NFE: &str = "http://www.portalfiscal.inf.br/nfe";
pub const NS_DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";

pub fn ler(xml: &str) -> Result<Document<'_>, roxmltree::Error> {
    Document::parse_with_options(xml, ParsingOptions { allow_dtd: false, ..Default::default() })
}

/// Primeiro elemento com esse nome local, em qualquer profundidade
/// (o envelope SOAP varia entre UFs).
pub fn encontrar<'a, 'i>(no: Node<'a, 'i>, nome: &str) -> Option<Node<'a, 'i>> {
    no.descendants().find(|n| n.is_element() && n.tag_name().name() == nome)
}

/// Filho direto (elemento) com esse nome local.
pub fn filho<'a, 'i>(no: Node<'a, 'i>, nome: &str) -> Option<Node<'a, 'i>> {
    no.children().find(|n| n.is_element() && n.tag_name().name() == nome)
}

pub fn filhos<'a, 'i>(no: Node<'a, 'i>, nome: &'a str) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    no.children().filter(move |n| n.is_element() && n.tag_name().name() == nome)
}

/// Texto completo de um elemento (concatena os nós de texto).
pub fn texto_de(no: Node) -> String {
    no.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

/// Texto do filho direto `nome`, se existir.
pub fn txt(no: Node, nome: &str) -> Option<String> {
    filho(no, nome).map(texto_de)
}

/// Texto de um caminho de filhos ("enderEmit/xMun").
pub fn caminho(no: Node, caminho: &str) -> Option<String> {
    let mut atual = no;
    for parte in caminho.split('/') {
        atual = filho(atual, parte)?;
    }
    Some(texto_de(atual))
}

/// Recorta o texto original de um elemento (ex.: protNFe), preservando-o byte a byte.
/// Necessário para montar o nfeProc sem alterar o que a SEFAZ assinou/devolveu.
pub fn recortar_elemento(xml: &str, nome: &str) -> Option<String> {
    let abre = Regex::new(&format!(r"<(\w+:)?{nome}[\s>]")).ok()?;
    let inicio = abre.find(xml)?.start();
    let fecha = Regex::new(&format!(r"</(\w+:)?{nome}>")).ok()?;
    let fim = fecha.find(&xml[inicio..])?;
    Some(xml[inicio..inicio + fim.end()].to_string())
}

/// Remove a declaração `<?xml ...?>` do início.
pub fn sem_declaracao(xml: &str) -> &str {
    let t = xml.trim_start();
    if t.starts_with("<?xml") {
        if let Some(fim) = t.find("?>") {
            return t[fim + 2..].trim_start();
        }
    }
    t
}

// ---------------------------------------------------------------------------
// C14N 1.0 inclusiva, sem comentários (http://www.w3.org/TR/2001/REC-xml-c14n-20010315)
// ---------------------------------------------------------------------------

/// Nome qualificado (com prefixo) exatamente como está no texto original.
fn nome_qualificado<'i>(no: Node<'_, 'i>, fonte: &'i str) -> &'i str {
    let r = no.range();
    let resto = &fonte[r.start + 1..];
    let fim = resto.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(resto.len());
    &resto[..fim]
}

fn escapar_texto(s: &str, saida: &mut String) {
    for c in s.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '\r' => saida.push_str("&#xD;"),
            _ => saida.push(c),
        }
    }
}

fn escapar_atributo(s: &str, saida: &mut String) {
    for c in s.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '"' => saida.push_str("&quot;"),
            '\t' => saida.push_str("&#x9;"),
            '\n' => saida.push_str("&#xA;"),
            '\r' => saida.push_str("&#xD;"),
            _ => saida.push(c),
        }
    }
}

fn namespaces_em_escopo(no: Node) -> BTreeMap<String, String> {
    no.namespaces()
        .filter(|ns| ns.name() != Some("xml"))
        .map(|ns| (ns.name().unwrap_or("").to_string(), ns.uri().to_string()))
        .collect()
}

/// Forma canônica do elemento `no` e de todo o seu conteúdo.
/// `excluir` remove elementos do conjunto (transformação enveloped-signature).
pub fn c14n(no: Node, fonte: &str, excluir: &dyn Fn(Node) -> bool) -> String {
    let mut saida = String::new();
    elemento_canonico(no, fonte, None, excluir, &mut saida);
    saida
}

fn elemento_canonico(
    no: Node,
    fonte: &str,
    escopo_pai: Option<&BTreeMap<String, String>>,
    excluir: &dyn Fn(Node) -> bool,
    saida: &mut String,
) {
    let escopo = namespaces_em_escopo(no);
    let nome = nome_qualificado(no, fonte);
    saida.push('<');
    saida.push_str(nome);

    // Declarações de namespace: no elemento raiz do recorte, todas as que estão em
    // escopo; nos demais, só as que mudam em relação ao pai. Ordenadas pelo prefixo.
    let mut declarar: Vec<(&str, &str)> = Vec::new();
    match escopo_pai {
        None => {
            for (p, u) in &escopo {
                if !(p.is_empty() && u.is_empty()) {
                    declarar.push((p, u));
                }
            }
        }
        Some(pai) => {
            for (p, u) in &escopo {
                if pai.get(p) != Some(u) {
                    declarar.push((p, u));
                }
            }
            if !escopo.contains_key("") && pai.get("").is_some_and(|u| !u.is_empty()) {
                declarar.push(("", ""));
            }
        }
    }
    declarar.sort();
    for (p, u) in declarar {
        if p.is_empty() {
            saida.push_str(" xmlns=\"");
        } else {
            saida.push_str(" xmlns:");
            saida.push_str(p);
            saida.push_str("=\"");
        }
        escapar_atributo(u, saida);
        saida.push('"');
    }

    // Atributos ordenados por (namespace, nome local); sem namespace vêm primeiro.
    let mut atributos: Vec<_> = no.attributes().collect();
    atributos.sort_by(|a, b| (a.namespace().unwrap_or(""), a.name()).cmp(&(b.namespace().unwrap_or(""), b.name())));
    for a in atributos {
        saida.push(' ');
        saida.push_str(&fonte[a.range_qname()]);
        saida.push_str("=\"");
        escapar_atributo(a.value(), saida);
        saida.push('"');
    }
    saida.push('>');

    for filho in no.children() {
        if filho.is_element() {
            if !excluir(filho) {
                elemento_canonico(filho, fonte, Some(&escopo), excluir, saida);
            }
        } else if filho.is_text() {
            escapar_texto(filho.text().unwrap_or(""), saida);
        }
    }
    saida.push_str("</");
    saida.push_str(nome);
    saida.push('>');
}

#[cfg(test)]
mod testes {
    use super::*;

    fn canon(xml: &str, nome: &str) -> String {
        let doc = ler(xml).unwrap();
        c14n(encontrar(doc.root(), nome).unwrap(), xml, &|_| false)
    }

    #[test]
    fn herda_namespace_e_ordena_atributos() {
        let xml = r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00" Id="NFe1"><a/><b x="1">t&amp;&lt;&gt;</b></infNFe></NFe>"#;
        assert_eq!(
            canon(xml, "infNFe"),
            r#"<infNFe xmlns="http://www.portalfiscal.inf.br/nfe" Id="NFe1" versao="4.00"><a></a><b x="1">t&amp;&lt;&gt;</b></infNFe>"#
        );
    }

    #[test]
    fn namespaces_prefixados_e_redundantes() {
        let xml = r#"<s:E xmlns:s="urn:s" xmlns:z="urn:z"><x xmlns="urn:d"><y xmlns="urn:d" a="&quot;"/><z:w xmlns="">a&#xD;b</z:w></x></s:E>"#;
        assert_eq!(
            canon(xml, "x"),
            "<x xmlns=\"urn:d\" xmlns:s=\"urn:s\" xmlns:z=\"urn:z\"><y a=\"&quot;\"></y><z:w xmlns=\"\">a&#xD;b</z:w></x>"
        );
    }

    #[test]
    fn recorta_elemento_original() {
        let xml = "<a><ns2:protNFe versao=\"4.00\"><x/></ns2:protNFe></a>";
        assert_eq!(recortar_elemento(xml, "protNFe").unwrap(), "<ns2:protNFe versao=\"4.00\"><x/></ns2:protNFe>");
        assert_eq!(recortar_elemento(xml, "nada"), None);
    }
}
