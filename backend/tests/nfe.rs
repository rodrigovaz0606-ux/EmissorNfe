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
    assert_eq!(pegar(&g.xml, "serie").unwrap(), "920");
}

#[test]
fn serie_por_tipo_de_emitente() {
    // CPF fora de 920-969 cairia na rejeição 407 da SEFAZ: barrado antes de enviar.
    let mut d = cenario();
    d.ide.as_mut().unwrap().serie = Some(json!(1));
    let erro = gerar_xml_nfe(&d, &config()).unwrap_err().to_string();
    assert!(erro.contains("920 a 969"), "{erro}");
    for serie in [920, 969] {
        d.ide.as_mut().unwrap().serie = Some(json!(serie));
        let g = gerar_xml_nfe(&d, &config()).unwrap();
        assert_eq!(&g.chave[22..25], format!("{serie:03}"), "série na chave");
        assert_eq!(&g.chave[6..20], "00052998224725", "CPF com zeros à esquerda na chave");
    }
    // CNPJ não pode usar a faixa do CPF
    let mut c = cenario();
    let emit = c.emit.as_mut().unwrap();
    emit.CPF = None;
    emit.CNPJ = Some(json!("12345678000195"));
    c.ide.as_mut().unwrap().serie = Some(json!(920));
    assert!(gerar_xml_nfe(&c, &config()).unwrap_err().to_string().contains("reservadas"));
    c.ide.as_mut().unwrap().serie = Some(json!(1));
    assert!(gerar_xml_nfe(&c, &config()).is_ok());
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
    d.ide.as_mut().unwrap().serie = Some(json!("920"));
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

#[test]
fn codigo_de_beneficio_fiscal() {
    // Goiás exige cBenef em CST com benefício (rejeição 930). Vai em <prod>, antes do CFOP.
    let mut d = cenario();
    d.itens[0].cBenef = Some(json!(" go811001 "));
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(g.xml.contains("<NCM>12019000</NCM><cBenef>GO811001</cBenef><CFOP>5101</CFOP>"), "{}", g.xml);
    d.itens[0].cBenef = Some(json!("SEM CBENEF"));
    assert!(gerar_xml_nfe(&d, &config()).unwrap().xml.contains("<cBenef>SEM CBENEF</cBenef>"));
    for ruim in ["GO81", "GO 81100", "GO81100!"] {
        d.itens[0].cBenef = Some(json!(ruim));
        assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("cBenef"), "{ruim}");
    }
    d.itens[0].cBenef = Some(json!(""));
    assert!(!gerar_xml_nfe(&d, &config()).unwrap().xml.contains("cBenef"));
}

fn com_boi(d: &mut DadosNFe) {
    d.itens[0].NCM = Some(json!("01022990")); // bovino vivo
    d.itens[0].xProd = Some(json!("BOI GORDO"));
    d.itens[0].uCom = Some(json!("CB"));
}

#[test]
fn guia_de_transito_animal() {
    use emissor_nfe::fiscal::nfe::{Agropecuario, GuiaTransito};
    use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};
    // Goiás: boi vivo sem GTA é rejeição 836, barrada antes de enviar.
    let mut d = cenario();
    com_boi(&mut d);
    let erro = gerar_xml_nfe(&d, &config()).unwrap_err().to_string();
    assert!(erro.contains("836") && erro.contains("Item 1"), "{erro}");

    // Com GTA: grupo no fim de infNFe e XML válido no schema oficial (PL_010).
    d.agropecuario = Some(Agropecuario {
        guiaTransito: Some(GuiaTransito { tpGuia: Some(json!("1")), UFGuia: Some(json!("go")), serieGuia: Some(json!("A")), nGuia: Some(json!("123456")) }),
    });
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(
        g.xml.ends_with("<agropecuario><guiaTransito><tpGuia>1</tpGuia><UFGuia>GO</UFGuia><serieGuia>A</serieGuia><nGuia>123456</nGuia></guiaTransito></agropecuario></infNFe></NFe>"),
        "{}",
        &g.xml[g.xml.len() - 300..]
    );
    let assinado = emissor_nfe::fiscal::assinatura::assinar_nfe(&g.xml, &emissor_nfe::fiscal::certificado::certificado_de_teste()).unwrap();
    let xsd = validar_xsd(&assinado, Pacote::Nfe).unwrap();
    assert!(xsd.valido, "{:?}", xsd.erros);

    // Formato da guia
    let mut ruim = d.clone();
    ruim.agropecuario.as_mut().unwrap().guiaTransito.as_mut().unwrap().nGuia = Some(json!("12A"));
    assert!(gerar_xml_nfe(&ruim, &config()).unwrap_err().to_string().contains("Número da guia"));
    ruim.agropecuario.as_mut().unwrap().guiaTransito.as_mut().unwrap().nGuia = Some(json!("1234567890"));
    assert!(gerar_xml_nfe(&ruim, &config()).is_err());

    // CFOP de retorno/transferência dispensa a GTA; devolução (finNFe 4) também.
    let mut retorno = cenario();
    com_boi(&mut retorno);
    retorno.itens[0].CFOP = Some(json!("5949"));
    assert!(gerar_xml_nfe(&retorno, &config()).is_ok());

    // 310: guia de animal numa nota sem animal vivo.
    let mut soja = cenario();
    soja.agropecuario = d.agropecuario.clone();
    assert!(gerar_xml_nfe(&soja, &config()).unwrap_err().to_string().contains("310"));
}

/// Comparação com a libxml2 (ver ferramentas/comparar_xsd.py). Só roda com COMPARAR_XSD_DIR definido:
/// 1ª fase grava amostras assinadas (certificado fictício); 2ª fase valida cada .xml da pasta.
#[test]
fn comparacao_xsd_com_libxml2() {
    use emissor_nfe::fiscal::assinatura::assinar_nfe;
    use emissor_nfe::fiscal::certificado::certificado_de_teste;
    use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};
    let Ok(pasta) = std::env::var("COMPARAR_XSD_DIR") else { return };
    let pasta = std::path::PathBuf::from(pasta);
    let cert = certificado_de_teste();
    if std::env::var("COMPARAR_XSD_FASE").as_deref() == Ok("gerar") {
        let mut amostras: Vec<(&str, DadosNFe)> = vec![("soja_cpf", cenario())];
        let mut boi = cenario();
        com_boi(&mut boi);
        boi.agropecuario = Some(serde_json::from_value(json!({ "guiaTransito": { "tpGuia": "1", "UFGuia": "GO", "serieGuia": "A", "nGuia": "123456" } })).unwrap());
        amostras.push(("boi_gta", boi));
        let mut benef = cenario();
        benef.itens[0].cBenef = Some(json!("GO811001"));
        amostras.push(("cbenef", benef));
        let mut dev = cenario();
        let ide = dev.ide.as_mut().unwrap();
        ide.finNFe = Some(json!(4));
        ide.refNFe = vec![json!("52261112345678000195550010000000011482017365")];
        amostras.push(("devolucao", dev));
        let mut integral = cenario();
        com_classificacao(&mut integral, "000001");
        amostras.push(("ibscbs_integral", integral));
        let mut reducao = cenario();
        com_classificacao(&mut reducao, "200036");
        amostras.push(("ibscbs_reducao", reducao));
        for (nome, d) in amostras {
            let g = gerar_xml_nfe(&d, &config()).unwrap();
            std::fs::write(pasta.join(format!("base_{nome}.xml")), assinar_nfe(&g.xml, &cert).unwrap()).unwrap();
        }
        return;
    }
    let mut saida = String::new();
    let mut nomes: Vec<_> = std::fs::read_dir(&pasta).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "xml")).collect();
    nomes.sort();
    for p in nomes {
        let xml = std::fs::read_to_string(&p).unwrap();
        let valido = validar_xsd(&xml, Pacote::Nfe).map(|r| r.valido).unwrap_or(false);
        saida.push_str(&format!("{}\t{}\n", p.file_name().unwrap().to_string_lossy(), valido));
    }
    std::fs::write(pasta.join("emissor.tsv"), saida).unwrap();
}

fn com_classificacao(d: &mut DadosNFe, c: &str) {
    d.itens[0].ibscbs = Some(emissor_nfe::fiscal::nfe::IbsCbs { cClassTrib: Some(json!(c)) });
}

fn validar_assinado(xml: &str) {
    use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};
    let assinado = emissor_nfe::fiscal::assinatura::assinar_nfe(xml, &emissor_nfe::fiscal::certificado::certificado_de_teste()).unwrap();
    let r = validar_xsd(&assinado, Pacote::Nfe).unwrap();
    assert!(r.valido, "{:?}", r.erros);
}

#[test]
fn ibs_cbs_produtor_rural_nao_contribuinte() {
    // CST 410 proíbe o grupo de valores (ind_gIBSCBS = 0): só CST e cClassTrib. Total com base zero.
    let g = gerar_xml_nfe(&cenario(), &config()).unwrap();
    assert!(g.xml.contains("</COFINS><IBSCBS><CST>410</CST><cClassTrib>410014</cClassTrib></IBSCBS></imposto>"), "{}", g.xml);
    assert!(g.xml.contains("</ICMSTot><IBSCBSTot><vBCIBSCBS>0.00</vBCIBSCBS></IBSCBSTot></total>"));
    validar_assinado(&g.xml);
}

#[test]
fn ibs_cbs_tributacao_integral_2026() {
    // Soja: 500 × 132,50 = 66.250,00; ICMS 51 e PIS/COFINS 08 sem valor → base 66.250,00.
    // IBS UF 0,10% = 66,25 · IBS município 0% · CBS 0,90% = 596,25 (regras UB18, UB37, UB56).
    let mut d = cenario();
    com_classificacao(&mut d, "000001");
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(g.xml.contains(
        "<IBSCBS><CST>000</CST><cClassTrib>000001</cClassTrib><gIBSCBS><vBC>66250.00</vBC><gIBSUF><pIBSUF>0.1000</pIBSUF><vIBSUF>66.25</vIBSUF></gIBSUF><gIBSMun><pIBSMun>0.0000</pIBSMun><vIBSMun>0.00</vIBSMun></gIBSMun><vIBS>66.25</vIBS><gCBS><pCBS>0.9000</pCBS><vCBS>596.25</vCBS></gCBS></gIBSCBS></IBSCBS>"
    ), "{}", g.xml);
    assert!(g.xml.contains("<IBSCBSTot><vBCIBSCBS>66250.00</vBCIBSCBS><gIBS><gIBSUF><vDif>0.00</vDif><vDevTrib>0.00</vDevTrib><vIBSUF>66.25</vIBSUF></gIBSUF>"));
    assert!(g.xml.contains("<vIBS>66.25</vIBS><vCredPres>0.00</vCredPres><vCredPresCondSus>0.00</vCredPresCondSus></gIBS><gCBS><vDif>0.00</vDif><vDevTrib>0.00</vDevTrib><vCBS>596.25</vCBS>"));
    validar_assinado(&g.xml);

    // A base desconta ICMS, PIS e COFINS (regra UB16): ICMS 00 a 12% sobre 66.250 = 7.950 → base 58.300.
    d.itens[0].icms = Some(serde_json::from_value(json!({ "orig": 0, "CST": "00", "pICMS": 12 })).unwrap());
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(g.xml.contains("<vBC>58300.00</vBC><gIBSUF><pIBSUF>0.1000</pIBSUF><vIBSUF>58.30</vIBSUF>"), "{}", g.xml);
}

#[test]
fn ibs_cbs_agropecuario_in_natura_com_reducao() {
    // 60% de redução: alíquota efetiva UF 0,1 × 0,4 = 0,04%; CBS 0,9 × 0,4 = 0,36% (regra UB28).
    let mut d = cenario();
    com_classificacao(&mut d, "200036");
    let g = gerar_xml_nfe(&d, &config()).unwrap();
    assert!(g.xml.contains("<gIBSUF><pIBSUF>0.1000</pIBSUF><gRed><pRedAliq>60.0000</pRedAliq><pAliqEfet>0.0400</pAliqEfet></gRed><vIBSUF>26.50</vIBSUF></gIBSUF>"), "{}", g.xml);
    assert!(g.xml.contains("<gCBS><pCBS>0.9000</pCBS><gRed><pRedAliq>60.0000</pRedAliq><pAliqEfet>0.3600</pAliqEfet></gRed><vCBS>238.50</vCBS></gCBS>"));
    validar_assinado(&g.xml);
}

#[test]
fn ibs_cbs_recusas() {
    let mut d = cenario();
    com_classificacao(&mut d, "999999");
    assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("não suportada"));
    d.itens[0].ibscbs = None;
    assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("cClassTrib"));
    // 2027: a CBS ainda aguarda legislação; não inventa alíquota.
    let mut d = cenario();
    com_classificacao(&mut d, "000001");
    d.ide.as_mut().unwrap().dhEmi = Some(json!("2027-01-05T10:00:00-03:00"));
    assert!(gerar_xml_nfe(&d, &config()).unwrap_err().to_string().contains("2027"));
    // ...mas o produtor não contribuinte (sem valores) continua emitindo.
    com_classificacao(&mut d, "410014");
    assert!(gerar_xml_nfe(&d, &config()).is_ok());
}
