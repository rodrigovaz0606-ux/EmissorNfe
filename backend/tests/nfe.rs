//! Geração do XML da NF-e, assinatura e validação no XSD oficial.

use emissor_nfe::exemplo::venda_soja;
use emissor_nfe::fiscal::assinatura::{assinar_nfe, verificar_assinatura};
use emissor_nfe::fiscal::certificado::certificado_de_teste;
use emissor_nfe::fiscal::chave::validar_chave;
use emissor_nfe::fiscal::config::Config;
use emissor_nfe::fiscal::nfe::{DadosNFe, NOME_DEST_HOMOLOGACAO, gerar_xml_nfe};
use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};
use regex::Regex;
use serde_json::json;

const DH: &str = "2026-11-03T10:00:00-03:00";

/// CPF fictício (DV válido) para não depender do certificado real nos testes.
fn cenario() -> DadosNFe {
    let mut d = venda_soja("52998224725", "PRODUTOR TESTE");
    let ide = d.ide.as_mut().unwrap();
    ide.dhEmi = Some(json!(DH));
    ide.cNF = Some(json!("48201736"));
    d
}

fn config() -> Config {
    Config::teste()
}

fn pegar(xml: &str, tag: &str) -> Option<String> {
    Regex::new(&format!("<{tag}>([^<]*)</{tag}>")).unwrap().captures(xml).map(|c| c[1].to_string())
}

fn recorte(xml: &str, tag: &str) -> String {
    Regex::new(&format!("<{tag}>.*?</{tag}>")).unwrap().find(xml).unwrap().as_str().to_string()
}

#[test]
fn cenario_da_soja_passa_no_xsd_faltando_so_a_assinatura() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    let r = validar_xsd(&g.xml, Pacote::Nfe).unwrap();
    assert_eq!(r.erros.len(), 1, "{:#?}", r.erros);
    assert!(r.erros[0].contains("Elemento 'NFe': conteúdo incompleto. Esperado: ( ds:Signature, infNFeSupl )"), "{}", r.erros[0]);
}

#[test]
fn chave_id_e_ide_coerentes() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    assert!(validar_chave(&g.chave));
    assert!(g.xml.contains(&format!("<infNFe Id=\"NFe{}\" versao=\"4.00\">", g.chave)));
    assert_eq!(pegar(&g.xml, "cNF").unwrap(), "48201736");
    assert_eq!(pegar(&g.xml, "cDV").unwrap(), &g.chave[43..]);
    assert_eq!(pegar(&g.xml, "tpAmb").unwrap(), "2");
    assert_eq!(pegar(&g.xml, "dhEmi").unwrap(), DH);
    assert_eq!(&g.chave[6..20], "00052998224725");
}

#[test]
fn totais_da_soja() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    assert_eq!(g.v_nf, "66250.00");
    assert_eq!(pegar(&g.xml, "vProd").unwrap(), "66250.00");
    assert_eq!(pegar(&g.xml, "vNF").unwrap(), "66250.00");
    assert_eq!(pegar(&g.xml, "vPag").unwrap(), "66250.00");
    assert_eq!(pegar(&g.xml, "qCom").unwrap(), "500.0000");
    assert_eq!(pegar(&g.xml, "vUnCom").unwrap(), "132.50");
    assert_eq!(pegar(&g.xml, "serie").unwrap(), "1");
}

#[test]
fn nome_do_destinatario_em_homologacao() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    assert!(g.xml.contains(&format!("<dest><CPF>12345678909</CPF><xNome>{NOME_DEST_HOMOLOGACAO}</xNome>")));
}

#[test]
fn icms_00_e_pis_cofins_por_aliquota() {
    let mut d = cenario();
    let base = d.itens[0].clone();
    let mut a = base.clone();
    a.icms = serde_json::from_value(json!({ "CST": "00", "pICMS": 12 })).unwrap();
    a.pis = serde_json::from_value(json!({ "CST": "01", "pPIS": 1.65 })).unwrap();
    a.cofins = serde_json::from_value(json!({ "CST": "01", "pCOFINS": 7.6 })).unwrap();
    let mut b = base;
    b.cProd = Some(json!("MILHO01"));
    b.xProd = Some(json!("MILHO"));
    b.NCM = Some(json!("10059010"));
    b.qCom = Some(json!(3));
    b.vUnCom = Some(json!(0.335));
    b.vDesc = Some(json!(0.01));
    d.itens = vec![a, b];
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    let tot = recorte(&g.xml, "ICMSTot");
    assert_eq!(pegar(&tot, "vBC").unwrap(), "66250.00");
    assert_eq!(pegar(&tot, "vICMS").unwrap(), "7950.00"); // 12%
    assert_eq!(pegar(&tot, "vPIS").unwrap(), "1093.13"); // 1,65% de 66.250 = 1.093,125 → 1.093,13
    assert_eq!(pegar(&tot, "vCOFINS").unwrap(), "5035.00");
    assert_eq!(pegar(&tot, "vProd").unwrap(), "66251.01"); // 3 × 0,335 = 1,005 → 1,01
    assert_eq!(pegar(&tot, "vDesc").unwrap(), "0.01");
    assert_eq!(g.v_nf, "66251.00");
    assert!(g.xml.contains("<pPIS>1.6500</pPIS>"));
    assert_eq!(validar_xsd(&g.xml, Pacote::Nfe).unwrap().erros.len(), 1);
}

#[test]
fn textos_limpos_e_escapados() {
    let mut d = cenario();
    d.itens[0].xProd = Some(json!("  SOJA   \"A&B\" <TIPO 1>  "));
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(g.xml.contains("<xProd>SOJA &quot;A&amp;B&quot; &lt;TIPO 1&gt;</xProd>"));
}

#[test]
fn validacoes_de_entrada() {
    let erro = |d: &DadosNFe, c: &Config| gerar_xml_nfe(d, c).unwrap_err().to_string();
    let mut producao = config();
    producao.tp_amb = 1;
    assert!(erro(&cenario(), &producao).contains("homologação"));

    let mut d = cenario();
    d.itens.clear();
    assert!(erro(&d, &config()).contains("1 a 990 itens"));

    let mut d = cenario();
    d.emit.as_mut().unwrap().CPF = Some(json!("11111111111"));
    assert!(erro(&d, &config()).contains("CPF do emitente inválido"));

    let mut d = cenario();
    d.itens[0].icms = serde_json::from_value(json!({ "CST": "60" })).unwrap();
    assert!(erro(&d, &config()).contains("CST de ICMS \"60\" ainda não suportado"));

    let mut d = cenario();
    d.itens[0].NCM = Some(json!("12"));
    assert!(erro(&d, &config()).contains("NCM"));

    let mut d = cenario();
    d.itens[0].qCom = Some(json!("abc"));
    assert!(erro(&d, &config()).contains("Número inválido"));
}

#[test]
fn bases_de_calculo() {
    // ICMS = produto − desconto + frete; PIS/COFINS = produto − desconto
    let mut d = cenario();
    let mut it = d.itens[0].clone();
    it.qCom = Some(json!(200));
    it.vUnCom = Some(json!(58.4));
    it.vDesc = Some(json!(120));
    it.vFrete = Some(json!(50));
    it.icms = serde_json::from_value(json!({ "CST": "00", "pICMS": 12 })).unwrap();
    it.pis = serde_json::from_value(json!({ "CST": "01", "pPIS": 1.65 })).unwrap();
    it.cofins = serde_json::from_value(json!({ "CST": "01", "pCOFINS": 7.6 })).unwrap();
    d.itens = vec![it];
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    let icms = recorte(&g.xml, "ICMS00");
    assert_eq!(pegar(&icms, "vBC").unwrap(), "11610.00"); // 11.680 − 120 + 50
    assert_eq!(pegar(&icms, "vICMS").unwrap(), "1393.20");
    assert_eq!(pegar(&recorte(&g.xml, "PISAliq"), "vBC").unwrap(), "11560.00"); // 11.680 − 120
    assert_eq!(pegar(&recorte(&g.xml, "PISAliq"), "vPIS").unwrap(), "190.74");
    assert_eq!(pegar(&recorte(&g.xml, "COFINSAliq"), "vCOFINS").unwrap(), "878.56");
    assert_eq!(g.v_nf, "11610.00");
    d.itens[0].vDesc = Some(json!(99999));
    assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("desconto maior"));
}

#[test]
fn cep_do_emitente_obrigatorio_no_destinatario_opcional() {
    let mut d = cenario();
    d.dest.as_mut().unwrap().ender.as_mut().unwrap().CEP = None;
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert_eq!(validar_xsd(&g.xml, Pacote::Nfe).unwrap().erros.len(), 1); // só a falta da assinatura

    let mut d = cenario();
    d.emit.as_mut().unwrap().ender.as_mut().unwrap().CEP = None;
    assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("CEP do emitente"));
}

#[test]
fn valores_como_texto_do_formulario() {
    // O formulário pode mandar números como texto, com vírgula.
    let mut d = cenario();
    d.itens[0].qCom = Some(json!("500"));
    d.itens[0].vUnCom = Some(json!("132,50"));
    d.ide.as_mut().unwrap().serie = Some(json!("1"));
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert_eq!(g.v_nf, "66250.00");
}

// ---------- assinatura ----------

#[test]
fn xml_assinado_passa_no_xsd_e_tem_a_estrutura_da_sefaz() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    let assinado = assinar_nfe(&g.xml, &certificado_de_teste()).unwrap();
    let r = validar_xsd(&assinado, Pacote::Nfe).unwrap();
    assert!(r.valido, "{:#?}", r.erros);

    assert!(assinado.contains("</infNFe><Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"><SignedInfo>"));
    assert!(assinado.contains(&format!("<Reference URI=\"#NFe{}\">", g.chave)));
    assert!(assinado.contains("<CanonicalizationMethod Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315\"/>"));
    assert!(assinado.contains("<SignatureMethod Algorithm=\"http://www.w3.org/2000/09/xmldsig#rsa-sha1\"/>"));
    assert!(assinado.contains(
        "<Transform Algorithm=\"http://www.w3.org/2000/09/xmldsig#enveloped-signature\"/><Transform Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315\"/>"
    ));
    assert!(assinado.contains("<DigestMethod Algorithm=\"http://www.w3.org/2000/09/xmldsig#sha1\"/>"));
    assert!(Regex::new(r"<KeyInfo><X509Data><X509Certificate>[A-Za-z0-9+/=]+</X509Certificate></X509Data></KeyInfo></Signature></NFe>$").unwrap().is_match(&assinado));
    assert!(!assinado.contains("ds:Signature")); // SEFAZ rejeita prefixo

    assert!(verificar_assinatura(&assinado).valida);
    let adulterado = assinado.replace("<vNF>66250.00</vNF>", "<vNF>1.00</vNF>");
    assert_ne!(adulterado, assinado);
    assert!(!verificar_assinatura(&adulterado).valida);
}

#[test]
fn xsd_aponta_campo_errado() {
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    let assinado = assinar_nfe(&g.xml, &certificado_de_teste()).unwrap();
    // Troca o CFOP por um valor fora do padrão do schema (sem reassinar: o XSD não olha a assinatura).
    let r = validar_xsd(&assinado.replace("<CFOP>5101</CFOP>", "<CFOP>9101</CFOP>"), Pacote::Nfe).unwrap();
    assert_eq!(r.erros.len(), 1, "{:#?}", r.erros);
    assert!(r.erros[0].starts_with("Elemento 'CFOP': o valor '9101' não atende ao padrão"), "{}", r.erros[0]);

    let r = validar_xsd(&assinado.replace("<xBairro>CENTRO</xBairro>", ""), Pacote::Nfe).unwrap();
    assert!(r.erros[0].contains("Elemento 'cMun': não esperado aqui. Esperado: ( xBairro, xCpl )"), "{:#?}", r.erros);
}
