//! Montagem e leitura das mensagens da SEFAZ (sem rede).

use emissor_nfe::fiscal::assinatura::verificar_assinatura;
use emissor_nfe::fiscal::certificado::certificado_de_teste;
use emissor_nfe::fiscal::config::Config;
use emissor_nfe::fiscal::servicos::*;
use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};

const CHAVE: &str = "52261112345678000195550010000000011482017365"; // fictícia
const DH: &str = "2026-11-03T10:00:00-03:00";

fn envelope(corpo: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><env:Envelope xmlns:env=\"http://www.w3.org/2003/05/soap-envelope\"><env:Header/><env:Body>\
         <nfeResultMsg xmlns=\"http://www.portalfiscal.inf.br/nfe/wsdl/NFeAutorizacao4\">{corpo}</nfeResultMsg></env:Body></env:Envelope>"
    )
}

fn prot_autorizada() -> String {
    format!(
        "<protNFe versao=\"4.00\"><infProt Id=\"NFe152260000000001\"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic><chNFe>{CHAVE}</chNFe>\
         <dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><nProt>152260000000001</nProt><digVal>abc=</digVal><cStat>100</cStat>\
         <xMotivo>Autorizado o uso da NF-e</xMotivo></infProt></protNFe>"
    )
}

fn nfe_assinada() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><NFe xmlns=\"http://www.portalfiscal.inf.br/nfe\"><infNFe Id=\"NFe{CHAVE}\" versao=\"4.00\"></infNFe><Signature xmlns=\"http://www.w3.org/2000/09/xmldsig#\"></Signature></NFe>"
    )
}

#[test]
fn status_monta_e_le() {
    assert_eq!(
        montar_cons_stat_serv(&Config::teste()),
        "<consStatServ versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><cUF>52</cUF><xServ>STATUS</xServ></consStatServ>"
    );
    let resposta = "<?xml version=\"1.0\" encoding=\"utf-8\"?><soap:Envelope xmlns:soap=\"http://www.w3.org/2003/05/soap-envelope\"><soap:Body>\
        <nfeResultMsg xmlns=\"http://www.portalfiscal.inf.br/nfe/wsdl/NFeStatusServico4\">\
        <retConsStatServ versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>\
        <cStat>107</cStat><xMotivo>Servico em Operacao</xMotivo><cUF>52</cUF><dhRecbto>2026-11-03T10:00:00-03:00</dhRecbto><tMed>1</tMed>\
        </retConsStatServ></nfeResultMsg></soap:Body></soap:Envelope>";
    let r = ler_ret_cons_stat_serv(resposta).unwrap();
    assert_eq!(r.cStat.as_deref(), Some("107"));
    assert!(r.emOperacao);
    assert_eq!(r.xMotivo.as_deref(), Some("Servico em Operacao"));
    assert!(ler_ret_cons_stat_serv("<html>erro</html>").unwrap_err().to_string().contains("Resposta sem retConsStatServ"));
}

#[test]
fn envi_nfe_sincrono_sem_declaracao() {
    let env = montar_envi_nfe(&nfe_assinada(), "123456789012345").unwrap();
    assert_eq!(
        env,
        format!(
            "<enviNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><idLote>123456789012345</idLote><indSinc>1</indSinc>{}</enviNFe>",
            nfe_assinada().replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "")
        )
    );
    assert!(montar_envi_nfe("<NFe></NFe>", "1").unwrap_err().to_string().contains("assinado"));
    let id = gerar_id_lote();
    assert!(id.len() == 15 && id.bytes().all(|b| b.is_ascii_digit()), "{id}");
}

#[test]
fn le_rejeicao_no_formato_real_da_sefaz_go() {
    let resp = envelope(&format!(
        "<retEnviNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\" xmlns:ns0=\"http://www.w3.org/2000/09/xmldsig#\">\
         <tpAmb>2</tpAmb><verAplic>GO4.0</verAplic><cStat>104</cStat><xMotivo>Lote processado</xMotivo><cUF>52</cUF>\
         <dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><protNFe versao=\"4.00\"><infProt><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>\
         <chNFe>{CHAVE}</chNFe><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto><cStat>407</cStat>\
         <xMotivo>Rejeição: CPF do Emitente somente no serviço de Nota Fiscal Avulsa no site do Fisco</xMotivo></infProt></protNFe></retEnviNFe>"
    ));
    let r = ler_ret_envi_nfe(&resp).unwrap();
    assert_eq!(r.lote.cStat.as_deref(), Some("104"));
    let p = r.protocolo.unwrap();
    assert_eq!(p.cStat.as_deref(), Some("407"));
    assert!(!p.autorizada);
    assert_eq!(p.chNFe.as_deref(), Some(CHAVE));
    assert!(p.xMotivo.unwrap().contains("CPF do Emitente"));
}

#[test]
fn le_autorizacao_e_monta_nfe_proc_com_protocolo_original() {
    let resp = envelope(&format!(
        "<retEnviNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><verAplic>GO4.0</verAplic>\
         <cStat>104</cStat><xMotivo>Lote processado</xMotivo><cUF>52</cUF><dhRecbto>2026-11-03T13:16:25-03:00</dhRecbto>{}</retEnviNFe>",
        prot_autorizada()
    ));
    let p = ler_ret_envi_nfe(&resp).unwrap().protocolo.unwrap();
    assert!(p.autorizada);
    assert_eq!(p.nProt.as_deref(), Some("152260000000001"));
    assert_eq!(p.xml.as_deref(), Some(prot_autorizada().as_str()));
    let proc = montar_nfe_proc(&nfe_assinada(), p.xml.as_deref().unwrap());
    assert!(proc.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?><nfeProc versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><NFe "));
    assert!(proc.ends_with(&format!("</NFe>{}</nfeProc>", prot_autorizada())));
}

#[test]
fn lote_assincrono_recibo_e_consulta() {
    let recebido = ler_ret_envi_nfe(&envelope(
        "<retEnviNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><cStat>103</cStat>\
         <xMotivo>Lote recebido com sucesso</xMotivo><infRec><nRec>521000000000001</nRec><tMed>1</tMed></infRec></retEnviNFe>",
    ))
    .unwrap();
    assert_eq!(recebido.recibo.as_deref(), Some("521000000000001"));
    assert!(recebido.protocolo.is_none());
    let consulta = ler_ret_cons_reci_nfe(&envelope(&format!(
        "<retConsReciNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><nRec>521000000000001</nRec>\
         <cStat>104</cStat><xMotivo>Lote processado</xMotivo>{}</retConsReciNFe>",
        prot_autorizada()
    )))
    .unwrap();
    assert!(consulta.protocolo.unwrap().autorizada);
}

fn dados_evento() -> DadosEvento {
    DadosEvento { chave: CHAVE.into(), documento: "12345678000195".into(), n_seq_evento: 1, dh_evento: Some(DH.into()), ..Default::default() }
}

#[test]
fn cancelamento_monta_assina_e_passa_no_xsd() {
    let d = DadosEvento { n_prot: Some("152260000000001".into()), x_just: Some("Erro na digitacao do valor".into()), ..dados_evento() };
    let p = preparar_evento(TipoEvento::Cancelamento, &d, &Config::teste(), &certificado_de_teste()).unwrap();
    assert!(p.evento.contains(&format!("<infEvento Id=\"ID110111{CHAVE}01\">")));
    assert!(verificar_assinatura(&p.evento).valida);
    assert!(verificar_assinatura(&p.env_evento).valida); // continua válida dentro do lote
    let r = validar_xsd(&p.env_evento, Pacote::Cancelamento).unwrap();
    assert!(r.valido, "{:#?}", r.erros);
}

#[test]
fn cce_monta_assina_e_passa_no_xsd() {
    let d = DadosEvento {
        documento: "52998224725".into(),
        n_seq_evento: 2,
        x_correcao: Some("Corrigir o endereco do destinatario para Rua 10".into()),
        ..dados_evento()
    };
    let p = preparar_evento(TipoEvento::Cce, &d, &Config::teste(), &certificado_de_teste()).unwrap();
    assert!(p.env_evento.contains("<CPF>52998224725</CPF>"));
    assert!(p.env_evento.contains(&format!("Id=\"ID110110{CHAVE}02\"")));
    assert!(p.env_evento.contains(&format!("<xCondUso>{COND_USO_CCE}</xCondUso>")));
    let r = validar_xsd(&p.env_evento, Pacote::Cce).unwrap();
    assert!(r.valido, "{:#?}", r.erros);
}

#[test]
fn eventos_validacoes_de_entrada() {
    let c = Config::teste();
    let erro = |t, d: DadosEvento, c: &Config| montar_evento(t, &d, c).unwrap_err().to_string();
    let canc = |prot: &str, just: &str| DadosEvento { n_prot: Some(prot.into()), x_just: Some(just.into()), ..dados_evento() };
    let cce = |txt: &str| DadosEvento { x_correcao: Some(txt.into()), ..dados_evento() };
    assert!(erro(TipoEvento::Cancelamento, canc("1", "justificativa longa ok"), &c).contains("protocolo"));
    assert!(erro(TipoEvento::Cancelamento, canc("152260000000001", "curta"), &c).contains("15 a 255"));
    assert!(erro(TipoEvento::Cce, cce("curta"), &c).contains("15 a 1000"));
    assert!(erro(TipoEvento::Cce, DadosEvento { chave: "123".into(), ..cce("texto suficiente aqui") }, &c).contains("Chave"));
    assert!(erro(TipoEvento::Cce, DadosEvento { n_seq_evento: 21, ..cce("texto suficiente aqui") }, &c).contains("nSeqEvento"));
    let mut producao = Config::teste();
    producao.tp_amb = 1;
    assert!(erro(TipoEvento::Cce, cce("texto suficiente aqui"), &producao).contains("homologação"));
}

#[test]
fn le_retorno_de_evento_e_monta_proc() {
    let resp = format!(
        "<soap:Envelope xmlns:soap=\"http://www.w3.org/2003/05/soap-envelope\"><soap:Body><nfeResultMsg>\
         <retEnvEvento versao=\"1.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><idLote>1</idLote><tpAmb>2</tpAmb><verAplic>GO</verAplic>\
         <cOrgao>52</cOrgao><cStat>128</cStat><xMotivo>Lote de Evento Processado</xMotivo>\
         <retEvento versao=\"1.00\"><infEvento><tpAmb>2</tpAmb><verAplic>GO</verAplic><cOrgao>52</cOrgao><cStat>135</cStat>\
         <xMotivo>Evento registrado e vinculado a NF-e</xMotivo><chNFe>{CHAVE}</chNFe><tpEvento>110111</tpEvento>\
         <xEvento>Cancelamento</xEvento><nSeqEvento>1</nSeqEvento><dhRegEvento>2026-11-03T10:00:01-03:00</dhRegEvento>\
         <nProt>152260000000099</nProt></infEvento></retEvento></retEnvEvento></nfeResultMsg></soap:Body></soap:Envelope>"
    );
    let r = ler_ret_env_evento(&resp).unwrap();
    assert_eq!(r.lote.cStat.as_deref(), Some("128"));
    let e = r.evento.unwrap();
    assert_eq!(e.cStat.as_deref(), Some("135"));
    assert!(e.registrado);
    assert_eq!(e.nProt.as_deref(), Some("152260000000099"));
    let proc = montar_proc_evento(
        "<evento versao=\"1.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><infEvento/></evento>",
        &r.xml_ret_evento.unwrap(),
    );
    assert!(proc.contains(
        "<procEventoNFe versao=\"1.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><evento versao=\"1.00\"><infEvento/></evento><retEvento versao=\"1.00\">"
    ));
}

#[test]
fn inutilizacao_monta_assina_xsd_e_exige_cnpj() {
    let d = DadosInutilizacao {
        cnpj: "12.345.678/0001-95".into(),
        serie: 1,
        n_nf_ini: 10,
        n_nf_fin: 12,
        x_just: "Falha no sistema pulou a numeracao".into(),
        ano: Some(2026),
    };
    let xml = preparar_inutilizacao(&d, &Config::teste(), &certificado_de_teste()).unwrap();
    assert!(xml.contains("<infInut Id=\"ID52261234567800019555001000000010000000012\">"));
    assert!(verificar_assinatura(&xml).valida);
    let r = validar_xsd(&xml, Pacote::Inutilizacao).unwrap();
    assert!(r.valido, "{:#?}", r.erros);
    let cpf = DadosInutilizacao { cnpj: "52998224725".into(), ..d.clone() };
    assert!(montar_inut_nfe(&cpf, &Config::teste()).unwrap_err().to_string().contains("CNPJ"));
    let faixa = DadosInutilizacao { n_nf_fin: 5, ..d.clone() };
    assert!(montar_inut_nfe(&faixa, &Config::teste()).unwrap_err().to_string().contains("Faixa"));

    let ret = ler_ret_inut_nfe(
        "<x><retInutNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><infInut><tpAmb>2</tpAmb><cStat>102</cStat>\
         <xMotivo>Inutilizacao de numero homologado</xMotivo><nProt>152260000000123</nProt></infInut></retInutNFe></x>",
    )
    .unwrap();
    assert!(ret.homologada);
    let proc = montar_proc_inut(&xml, ret.xml_ret.as_deref().unwrap());
    assert!(proc.contains("<ProcInutNFe versao=\"4.00\""));
    assert!(proc.ends_with("</retInutNFe></ProcInutNFe>"));
}

#[test]
fn consulta_de_situacao_monta_e_le() {
    assert_eq!(
        montar_cons_sit_nfe(CHAVE, 2).unwrap(),
        format!("<consSitNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><xServ>CONSULTAR</xServ><chNFe>{CHAVE}</chNFe></consSitNFe>")
    );
    let r = ler_ret_cons_sit_nfe(&format!(
        "<x><retConsSitNFe versao=\"4.00\" xmlns=\"http://www.portalfiscal.inf.br/nfe\"><tpAmb>2</tpAmb><cStat>217</cStat>\
         <xMotivo>Rejeicao: NF-e nao consta na base de dados da SEFAZ</xMotivo><cUF>52</cUF><chNFe>{CHAVE}</chNFe></retConsSitNFe></x>"
    ))
    .unwrap();
    assert_eq!(r.cStat.as_deref(), Some("217"));
    assert!(r.protocolo.is_none());
    assert!(montar_cons_sit_nfe("123", 2).is_err());
}
