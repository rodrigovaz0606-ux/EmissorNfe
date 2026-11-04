//! DANFE em PDF.

use emissor_nfe::danfe::{contar_paginas, gerar_danfe, ler_nota_para_danfe};
use emissor_nfe::exemplo::venda_soja;
use emissor_nfe::fiscal::config::Config;
use emissor_nfe::fiscal::nfe::{DadosNFe, gerar_xml_nfe};
use serde_json::json;

fn xml_exemplo(ajuste: impl FnOnce(&mut DadosNFe)) -> (String, String) {
    let mut d = venda_soja("52998224725", "PRODUTOR TESTE");
    ajuste(&mut d);
    let g = gerar_xml_nfe(&d, &Config::teste()).unwrap();
    (g.xml, g.chave)
}

#[test]
fn gera_pdf_de_uma_pagina_para_a_nota_de_exemplo() {
    let (xml, chave) = xml_exemplo(|_| {});
    let n = ler_nota_para_danfe(&xml).unwrap();
    assert_eq!(n.chave, chave);
    assert!(n.protocolo.is_none());
    assert_eq!(n.emit.documento, "529.982.247-25");
    assert_eq!(n.itens.len(), 1);
    assert_eq!(n.itens[0].cst, "051");
    let pdf = gerar_danfe(&xml, false).unwrap();
    assert_eq!(&pdf[..5], b"%PDF-");
    assert_eq!(contar_paginas(&pdf), 1);
    let texto = String::from_utf8_lossy(&pdf);
    assert!(texto.contains("FOLHA 1/1"), "falta FOLHA 1/1");
    assert!(texto.contains("/GS1 gs"), "falta a marca d'água (texto transparente)");
}

#[test]
fn quebra_em_varias_folhas_com_muitos_itens() {
    let (xml, _) = xml_exemplo(|d| {
        let base = d.itens[0].clone();
        d.itens = (0..120)
            .map(|i| {
                let mut it = base.clone();
                it.cProd = Some(json!(format!("P{i}")));
                it.xProd = Some(json!(format!("PRODUTO {i}")));
                it.qCom = Some(json!(1));
                it.vUnCom = Some(json!(1));
                it
            })
            .collect();
    });
    let pdf = gerar_danfe(&xml, false).unwrap();
    let paginas = contar_paginas(&pdf);
    assert!(paginas >= 3, "páginas: {paginas}");
    let texto = String::from_utf8_lossy(&pdf);
    assert!(texto.contains(&format!("FOLHA {paginas}/{paginas}")));
}

#[test]
fn le_protocolo_do_nfe_proc_e_marca_cancelada() {
    let (xml, chave) = xml_exemplo(|_| {});
    let proc = format!(
        "<nfeProc versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\">{xml}<protNFe versao=\"4.00\"><infProt><tpAmb>2</tpAmb>\
         <chNFe>{chave}</chNFe><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><nProt>152260000000001</nProt><cStat>100</cStat>\
         <xMotivo>Autorizado o uso da NF-e</xMotivo></infProt></protNFe></nfeProc>"
    );
    let n = ler_nota_para_danfe(&proc).unwrap();
    assert_eq!(n.protocolo.as_ref().unwrap().0, "152260000000001");
    let pdf = gerar_danfe(&proc, true).unwrap();
    let texto = String::from_utf8_lossy(&pdf);
    assert!(texto.contains("CANCELADA"));
    assert!(texto.contains("152260000000001 - 03/11/2026 13:16:25"));
    assert!(ler_nota_para_danfe("<x/>").is_err());
}
