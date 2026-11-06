//! DANFE (Documento Auxiliar da NF-e) em PDF, retrato A4, a partir do XML (NFe ou nfeProc).

pub mod code128;
mod metricas;
pub mod pdf;

use crate::fiscal::formatos::fixo;
use crate::fiscal::xml::{caminho, filho, filhos, ler, txt};
use anyhow::{Context, Result};
use pdf::{Alinhar, CINZA, Documento, Fonte, LARGURA_A4, PRETO, Txt, VERMELHO};
use roxmltree::Node;

// ---------- formatação ----------

/// Número no formato brasileiro, com casas decimais entre `min` e `max`.
fn br(valor: f64, min: usize, max: usize) -> String {
    let s = fixo(valor, max);
    let (sinal, s) = s.strip_prefix('-').map(|r| ("-", r)).unwrap_or(("", &s));
    let (int, dec) = s.split_once('.').unwrap_or((s, ""));
    let mut dec = dec.trim_end_matches('0').to_string();
    while dec.len() < min {
        dec.push('0');
    }
    let mut grupos = Vec::new();
    let b = int.as_bytes();
    let mut fim = b.len();
    while fim > 3 {
        grupos.push(&int[fim - 3..fim]);
        fim -= 3;
    }
    grupos.push(&int[..fim]);
    grupos.reverse();
    let int = grupos.join(".");
    if dec.is_empty() { format!("{sinal}{int}") } else { format!("{sinal}{int},{dec}") }
}

fn num(v: &str) -> f64 {
    v.trim().parse().unwrap_or(0.0)
}

fn dinheiro(v: &str) -> String {
    br(num(v), 2, 2)
}

fn data(dh: &str) -> String {
    if dh.len() >= 10 { format!("{}/{}/{}", &dh[8..10], &dh[5..7], &dh[0..4]) } else { String::new() }
}

fn hora(dh: &str) -> String {
    dh.get(11..19).unwrap_or("").to_string()
}

fn chave_formatada(c: &str) -> String {
    c.as_bytes().chunks(4).map(|p| String::from_utf8_lossy(p).to_string()).collect::<Vec<_>>().join(" ")
}

fn cep(c: &str) -> String {
    if c.len() == 8 && c.bytes().all(|b| b.is_ascii_digit()) { format!("{}-{}", &c[..5], &c[5..]) } else { c.to_string() }
}

fn numero_nota(n: &str) -> String {
    let p = format!("{:0>9}", n);
    format!("{}.{}.{}", &p[0..3], &p[3..6], &p[6..9])
}

fn modalidade_frete(m: &str) -> String {
    match m {
        "0" => "0-Por conta do Remetente",
        "1" => "1-Por conta do Destinatário",
        "2" => "2-Por conta de Terceiros",
        "3" => "3-Próprio Remetente",
        "4" => "4-Próprio Destinatário",
        "9" => "9-Sem Transporte",
        outro => outro,
    }
    .to_string()
}

fn forma_pagamento(t: &str) -> String {
    match t {
        "01" => "Dinheiro".into(),
        "02" => "Cheque".into(),
        "03" => "Cartão de Crédito".into(),
        "04" => "Cartão de Débito".into(),
        "05" => "Crédito Loja".into(),
        "10" => "Vale Alimentação".into(),
        "11" => "Vale Refeição".into(),
        "12" => "Vale Presente".into(),
        "13" => "Vale Combustível".into(),
        "15" => "Boleto".into(),
        "16" => "Depósito".into(),
        "17" => "PIX".into(),
        "18" => "Transferência".into(),
        "19" => "Fidelidade".into(),
        "90" => "Sem pagamento".into(),
        "99" => "Outros".into(),
        outro => format!("Forma {outro}"),
    }
}

// ---------- leitura do XML ----------

#[derive(Debug, Default, Clone)]
pub struct Endereco {
    pub x_lgr: String,
    pub nro: String,
    pub x_cpl: String,
    pub x_bairro: String,
    pub x_mun: String,
    pub uf: String,
    pub cep: String,
    pub fone: String,
}

#[derive(Debug, Default, Clone)]
pub struct Pessoa {
    pub x_nome: String,
    pub documento: String,
    pub ie: String,
    pub iest: String,
    pub ender: Endereco,
}

#[derive(Debug, Default, Clone)]
pub struct ItemDanfe {
    pub c_prod: String,
    pub x_prod: String,
    pub inf_ad_prod: String,
    pub ncm: String,
    pub cst: String,
    pub cfop: String,
    pub u_com: String,
    pub q_com: String,
    pub v_un_com: String,
    pub v_prod: String,
    pub v_bc: String,
    pub v_icms: String,
    pub p_icms: String,
}

#[derive(Debug, Default, Clone)]
pub struct NotaDanfe {
    pub chave: String,
    pub n_nf: String,
    pub serie: String,
    pub tp_nf: String,
    pub nat_op: String,
    pub dh_emi: String,
    pub dh_sai_ent: String,
    pub tp_amb: String,
    pub emit: Pessoa,
    pub dest: Pessoa,
    pub itens: Vec<ItemDanfe>,
    pub total: std::collections::HashMap<String, String>,
    pub mod_frete: String,
    pub transportadora: Pessoa,
    pub placa: String,
    pub uf_veiculo: String,
    pub pag: Vec<(String, String)>,
    pub inf_ad_fisco: String,
    pub inf_cpl: String,
    pub referenciadas: Vec<String>,
    /// (nProt, dhRecbto) quando autorizada.
    pub protocolo: Option<(String, String)>,
}

fn t(no: Node, nome: &str) -> String {
    txt(no, nome).unwrap_or_default()
}

fn documento_formatado(no: Node) -> String {
    if let Some(c) = txt(no, "CNPJ").filter(|c| c.len() == 14) {
        return format!("{}.{}.{}/{}-{}", &c[0..2], &c[2..5], &c[5..8], &c[8..12], &c[12..14]);
    }
    if let Some(c) = txt(no, "CPF").filter(|c| c.len() == 11) {
        return format!("{}.{}.{}-{}", &c[0..3], &c[3..6], &c[6..9], &c[9..11]);
    }
    txt(no, "CNPJ").or_else(|| txt(no, "CPF")).or_else(|| txt(no, "idEstrangeiro")).unwrap_or_default()
}

fn endereco(no: Option<Node>) -> Endereco {
    let Some(e) = no else { return Endereco::default() };
    Endereco {
        x_lgr: t(e, "xLgr"),
        nro: t(e, "nro"),
        x_cpl: t(e, "xCpl"),
        x_bairro: t(e, "xBairro"),
        x_mun: t(e, "xMun"),
        uf: t(e, "UF"),
        cep: t(e, "CEP"),
        fone: t(e, "fone"),
    }
}

fn pessoa(no: Option<Node>, ender: &str) -> Pessoa {
    let Some(p) = no else { return Pessoa::default() };
    Pessoa {
        x_nome: t(p, "xNome"),
        documento: documento_formatado(p),
        ie: t(p, "IE"),
        iest: t(p, "IEST"),
        ender: endereco(filho(p, ender)),
    }
}

/// Extrai do XML o que o DANFE precisa.
pub fn ler_nota_para_danfe(xml: &str) -> Result<NotaDanfe> {
    let doc = ler(xml).context("XML inválido")?;
    let raiz = doc.root_element();
    let (nfe, proc) = match raiz.tag_name().name() {
        "nfeProc" => (filho(raiz, "NFe"), Some(raiz)),
        "NFe" => (Some(raiz), None),
        _ => (None, None),
    };
    let inf = nfe.and_then(|n| filho(n, "infNFe")).context("XML não contém uma NF-e.")?;
    let ide = filho(inf, "ide").context("NF-e sem ide")?;
    let tot = filho(inf, "total").and_then(|t| filho(t, "ICMSTot"));
    let transp = filho(inf, "transp");
    let inf_prot = proc.and_then(|p| filho(p, "protNFe")).and_then(|p| filho(p, "infProt"));

    Ok(NotaDanfe {
        chave: inf.attribute("Id").unwrap_or("").trim_start_matches("NFe").to_string(),
        n_nf: t(ide, "nNF"),
        serie: t(ide, "serie"),
        tp_nf: t(ide, "tpNF"),
        nat_op: t(ide, "natOp"),
        dh_emi: t(ide, "dhEmi"),
        dh_sai_ent: t(ide, "dhSaiEnt"),
        tp_amb: t(ide, "tpAmb"),
        emit: pessoa(filho(inf, "emit"), "enderEmit"),
        dest: pessoa(filho(inf, "dest"), "enderDest"),
        itens: filhos(inf, "det")
            .map(|d| {
                let icms = filho(d, "imposto").and_then(|i| filho(i, "ICMS")).and_then(|i| i.children().find(|c| c.is_element()));
                let ic = |n: &str| icms.and_then(|i| txt(i, n)).unwrap_or_default();
                let p = |n: &str| caminho(d, &format!("prod/{n}")).unwrap_or_default();
                let cst = if ic("CST").is_empty() { ic("CSOSN") } else { ic("CST") };
                ItemDanfe {
                    c_prod: p("cProd"),
                    x_prod: p("xProd"),
                    inf_ad_prod: t(d, "infAdProd"),
                    ncm: p("NCM"),
                    cst: format!("{}{cst}", ic("orig")),
                    cfop: p("CFOP"),
                    u_com: p("uCom"),
                    q_com: p("qCom"),
                    v_un_com: p("vUnCom"),
                    v_prod: p("vProd"),
                    v_bc: ic("vBC"),
                    v_icms: ic("vICMS"),
                    p_icms: ic("pICMS"),
                }
            })
            .collect(),
        total: tot.map(|t| t.children().filter(|c| c.is_element()).map(|c| (c.tag_name().name().to_string(), crate::fiscal::xml::texto_de(c))).collect()).unwrap_or_default(),
        mod_frete: transp.map(|t| t_(t, "modFrete")).unwrap_or_default(),
        transportadora: pessoa(transp.and_then(|t| filho(t, "transporta")), "-"),
        placa: transp.and_then(|t| caminho(t, "veicTransp/placa")).unwrap_or_default(),
        uf_veiculo: transp.and_then(|t| caminho(t, "veicTransp/UF")).unwrap_or_default(),
        pag: filho(inf, "pag").map(|p| filhos(p, "detPag").map(|d| (t(d, "tPag"), t(d, "vPag"))).collect()).unwrap_or_default(),
        inf_ad_fisco: caminho(inf, "infAdic/infAdFisco").unwrap_or_default(),
        inf_cpl: caminho(inf, "infAdic/infCpl").unwrap_or_default(),
        referenciadas: filhos(ide, "NFref").map(|r| t(r, "refNFe")).collect(),
        protocolo: inf_prot
            .filter(|i| matches!(txt(*i, "cStat").as_deref(), Some("100") | Some("150")))
            .map(|i| (t(i, "nProt"), t(i, "dhRecbto"))),
    })
}

fn t_(no: Node, nome: &str) -> String {
    t(no, nome)
}

// ---------- desenho ----------

const M: f32 = 18.0; // margem
const LARG: f32 = LARGURA_A4 - 2.0 * M;

#[derive(Default, Clone, Copy)]
struct OpcCaixa {
    negrito: bool,
    tamanho: Option<f32>,
    alinhar: Option<Alinhar>,
    quebrar: bool,
}

fn caixa(d: &mut Documento, x: f32, y: f32, w: f32, h: f32, rotulo: &str, valor: &str, o: OpcCaixa) {
    d.retangulo(x, y, w, h, 0.5);
    if !rotulo.is_empty() {
        d.texto(&rotulo.to_uppercase(), x + 2.0, y + 2.0, Txt::new(Fonte::Normal, 5.2).largura(w - 4.0).sem_quebra());
    }
    if !valor.is_empty() {
        let fonte = if o.negrito { Fonte::Negrito } else { Fonte::Normal };
        let mut txt = Txt::new(fonte, o.tamanho.unwrap_or(7.5))
            .largura(w - 6.0)
            .alinhar(o.alinhar.unwrap_or(Alinhar::Esquerda))
            .altura(h - if rotulo.is_empty() { 4.0 } else { 10.0 });
        if !o.quebrar {
            txt = txt.sem_quebra();
        }
        d.texto(valor, x + 3.0, y + if rotulo.is_empty() { 3.0 } else { 9.0 }, txt);
    }
}

fn direita() -> OpcCaixa {
    OpcCaixa { alinhar: Some(Alinhar::Direita), ..Default::default() }
}

fn centro() -> OpcCaixa {
    OpcCaixa { alinhar: Some(Alinhar::Centro), ..Default::default() }
}

fn titulo_bloco(d: &mut Documento, y: f32, texto: &str) -> f32 {
    d.texto(texto, M, y, Txt::new(Fonte::Negrito, 6.5).sem_quebra());
    y + 8.0
}

fn codigo_de_barras(d: &mut Documento, chave: &str, x: f32, y: f32, w: f32, h: f32) {
    let Ok((barras, modulos)) = code128::barras(chave) else { return };
    let modulo = w / modulos as f32;
    for (bx, bl) in barras {
        d.preencher(x + bx as f32 * modulo, y, bl as f32 * modulo, h, PRETO);
    }
}

fn marca_dagua(d: &mut Documento, texto: &str) {
    d.texto_girado(texto, 20.0, 440.0, Txt::new(Fonte::Negrito, 54.0).largura(555.0).centro().cor(VERMELHO), 35.0, (297.0, 470.0), true);
}

fn canhoto(d: &mut Documento, n: &NotaDanfe, y: f32) -> f32 {
    let h = 46.0;
    let w_num = 95.0;
    let recebemos = format!(
        "RECEBEMOS DE {} OS PRODUTOS/SERVIÇOS CONSTANTES DA NOTA FISCAL ELETRÔNICA INDICADA AO LADO. EMISSÃO: {} DESTINATÁRIO: {} VALOR TOTAL: R$ {}",
        n.emit.x_nome,
        data(&n.dh_emi),
        n.dest.x_nome,
        dinheiro(n.total.get("vNF").map(String::as_str).unwrap_or("0"))
    );
    caixa(d, M, y, LARG - w_num, 22.0, "", &recebemos, OpcCaixa { tamanho: Some(6.2), quebrar: true, ..Default::default() });
    caixa(d, M, y + 22.0, 110.0, h - 22.0, "Data de recebimento", "", OpcCaixa::default());
    caixa(d, M + 110.0, y + 22.0, LARG - w_num - 110.0, h - 22.0, "Identificação e assinatura do recebedor", "", OpcCaixa::default());
    let x = LARG + M - w_num;
    d.retangulo(x, y, w_num, h, 0.5);
    d.texto("NF-e", x, y + 6.0, Txt::new(Fonte::Negrito, 11.0).largura(w_num).centro());
    d.texto(&format!("Nº {}", numero_nota(&n.n_nf)), x, y + 21.0, Txt::new(Fonte::Negrito, 8.5).largura(w_num).centro());
    d.texto(&format!("SÉRIE {:0>3}", n.serie), x, y + 32.0, Txt::new(Fonte::Negrito, 8.5).largura(w_num).centro());
    let yc = y + h + 5.0;
    d.linha(M, yc, M + LARG, yc, 0.5, PRETO, Some((3.0, 2.0)));
    yc + 6.0
}

fn cabecalho(d: &mut Documento, n: &NotaDanfe, y: f32, folha: &str) -> f32 {
    let h = 92.0;
    let w_emit = 230.0;
    let w_danfe = 95.0;
    let x_danfe = M + w_emit;
    let x_chave = x_danfe + w_danfe;
    let w_chave = LARG - w_emit - w_danfe;

    // Emitente
    d.retangulo(M, y, w_emit, h, 0.5);
    let e = &n.emit;
    let end = &e.ender;
    d.texto("IDENTIFICAÇÃO DO EMITENTE", M + 2.0, y + 2.0, Txt::new(Fonte::Normal, 5.2));
    let y_nome = d.texto(&e.x_nome, M + 6.0, y + 14.0, Txt::new(Fonte::Negrito, 9.0).largura(w_emit - 12.0).centro());
    let mut linhas = vec![
        format!("{}, {}{}", end.x_lgr, end.nro, if end.x_cpl.is_empty() { String::new() } else { format!(" - {}", end.x_cpl) }),
        format!("{} - CEP {}", end.x_bairro, cep(&end.cep)),
        format!("{} - {}", end.x_mun, end.uf),
    ];
    if !end.fone.is_empty() {
        linhas.push(format!("Fone: {}", end.fone));
    }
    d.texto(&linhas.join("\n"), M + 6.0, y_nome + 4.0, Txt::new(Fonte::Normal, 7.0).largura(w_emit - 12.0).centro());

    // Quadro DANFE
    d.retangulo(x_danfe, y, w_danfe, h, 0.5);
    d.texto("DANFE", x_danfe, y + 5.0, Txt::new(Fonte::Negrito, 13.0).largura(w_danfe).centro());
    d.texto("Documento Auxiliar da\nNota Fiscal Eletrônica", x_danfe, y + 21.0, Txt::new(Fonte::Normal, 6.3).largura(w_danfe).centro());
    d.texto("0 - ENTRADA\n1 - SAÍDA", x_danfe + 8.0, y + 41.0, Txt::new(Fonte::Normal, 7.0));
    d.retangulo(x_danfe + 62.0, y + 41.0, 18.0, 16.0, 0.5);
    d.texto(&n.tp_nf, x_danfe + 62.0, y + 44.0, Txt::new(Fonte::Negrito, 11.0).largura(18.0).centro());
    d.texto(&format!("Nº {}", numero_nota(&n.n_nf)), x_danfe, y + 62.0, Txt::new(Fonte::Negrito, 8.0).largura(w_danfe).centro());
    d.texto(&format!("SÉRIE {:0>3}", n.serie), x_danfe, y + 72.0, Txt::new(Fonte::Negrito, 8.0).largura(w_danfe).centro());
    d.texto(&format!("FOLHA {folha}"), x_danfe, y + 82.0, Txt::new(Fonte::Normal, 7.0).largura(w_danfe).centro());

    // Chave e código de barras
    d.retangulo(x_chave, y, w_chave, h, 0.5);
    codigo_de_barras(d, &n.chave, x_chave + 8.0, y + 5.0, w_chave - 16.0, 30.0);
    caixa(d, x_chave, y + 40.0, w_chave, 20.0, "Chave de acesso", &chave_formatada(&n.chave), OpcCaixa { negrito: true, tamanho: Some(7.6), ..centro() });
    d.texto(
        "Consulta de autenticidade no portal nacional da NF-e www.nfe.fazenda.gov.br/portal ou no site da Sefaz Autorizadora",
        x_chave + 4.0,
        y + 66.0,
        Txt::new(Fonte::Normal, 6.6).largura(w_chave - 8.0).centro(),
    );

    // Natureza e protocolo
    let mut yy = y + h;
    caixa(d, M, yy, w_emit + w_danfe, 20.0, "Natureza da operação", &n.nat_op, OpcCaixa::default());
    let (prot, negrito) = match &n.protocolo {
        Some((n_prot, dh)) => (format!("{n_prot} - {} {}", data(dh), hora(dh)), false),
        None => ("NF-e SEM PROTOCOLO DE AUTORIZAÇÃO".into(), true),
    };
    caixa(d, x_chave, yy, w_chave, 20.0, "Protocolo de autorização de uso", &prot, OpcCaixa { negrito, ..centro() });
    yy += 20.0;
    let t3 = LARG / 3.0;
    caixa(d, M, yy, t3, 20.0, "Inscrição estadual", &e.ie, OpcCaixa::default());
    caixa(d, M + t3, yy, t3, 20.0, "Inscrição estadual do subst. trib.", &e.iest, OpcCaixa::default());
    caixa(d, M + 2.0 * t3, yy, t3, 20.0, "CNPJ / CPF", &e.documento, OpcCaixa::default());
    yy + 20.0 + 4.0
}

fn bloco_destinatario(d: &mut Documento, n: &NotaDanfe, y: f32) -> f32 {
    let mut y = titulo_bloco(d, y, "DESTINATÁRIO / REMETENTE");
    let p = &n.dest;
    let e = &p.ender;
    let h = 20.0;
    let c3 = 80.0;
    let o = OpcCaixa::default();
    caixa(d, M, y, LARG - 2.0 * c3 - 50.0, h, "Nome / razão social", &p.x_nome, o);
    caixa(d, M + LARG - 2.0 * c3 - 50.0, y, c3 + 50.0, h, "CNPJ / CPF", &p.documento, o);
    caixa(d, M + LARG - c3, y, c3, h, "Data da emissão", &data(&n.dh_emi), centro());
    y += h;
    let mut endereco = e.x_lgr.clone();
    if !e.nro.is_empty() {
        endereco += &format!(", {}", e.nro);
    }
    if !e.x_cpl.is_empty() {
        endereco += &format!(" - {}", e.x_cpl);
    }
    caixa(d, M, y, LARG - 3.0 * c3, h, "Endereço", &endereco, o);
    caixa(d, M + LARG - 3.0 * c3, y, c3, h, "Bairro / distrito", &e.x_bairro, o);
    caixa(d, M + LARG - 2.0 * c3, y, c3, h, "CEP", &cep(&e.cep), centro());
    caixa(d, M + LARG - c3, y, c3, h, "Data da saída/entrada", &data(&n.dh_sai_ent), centro());
    y += h;
    caixa(d, M, y, LARG - 3.0 * c3 - 30.0, h, "Município", &e.x_mun, o);
    caixa(d, M + LARG - 3.0 * c3 - 30.0, y, c3, h, "Fone / fax", &e.fone, o);
    caixa(d, M + LARG - 2.0 * c3 - 30.0, y, 30.0, h, "UF", &e.uf, centro());
    caixa(d, M + LARG - 2.0 * c3, y, c3, h, "Inscrição estadual", &p.ie, o);
    caixa(d, M + LARG - c3, y, c3, h, "Hora da saída/entrada", &hora(&n.dh_sai_ent), centro());
    y + h + 4.0
}

fn bloco_pagamento(d: &mut Documento, n: &NotaDanfe, y: f32) -> f32 {
    if n.pag.is_empty() {
        return y;
    }
    let y = titulo_bloco(d, y, "PAGAMENTO");
    let w = LARG / 4.0;
    for (i, (t_pag, v_pag)) in n.pag.iter().take(8).enumerate() {
        let x = M + (i % 4) as f32 * w;
        let yy = y + (i / 4) as f32 * 20.0;
        caixa(d, x, yy, w, 20.0, &forma_pagamento(t_pag), &format!("R$ {}", dinheiro(v_pag)), direita());
    }
    y + n.pag.len().min(8).div_ceil(4) as f32 * 20.0 + 4.0
}

fn bloco_imposto(d: &mut Documento, n: &NotaDanfe, y: f32) -> f32 {
    let y = titulo_bloco(d, y, "CÁLCULO DO IMPOSTO");
    let v = |k: &str| dinheiro(n.total.get(k).map(String::as_str).unwrap_or("0"));
    let linha1 = [
        ("Base de cálc. do ICMS", "vBC"),
        ("Valor do ICMS", "vICMS"),
        ("Base de cálc. ICMS S.T.", "vBCST"),
        ("Valor do ICMS subst.", "vST"),
        ("V. total tributos", "vTotTrib"),
        ("V. total produtos", "vProd"),
    ];
    let linha2 = [
        ("Valor do frete", "vFrete"),
        ("Valor do seguro", "vSeg"),
        ("Desconto", "vDesc"),
        ("Outras desp. acessórias", "vOutro"),
        ("Valor total IPI", "vIPI"),
        ("V. total da nota", "vNF"),
    ];
    let w = LARG / 6.0;
    for (i, (r, k)) in linha1.iter().enumerate() {
        caixa(d, M + i as f32 * w, y, w, 20.0, r, &v(k), direita());
    }
    for (i, (r, k)) in linha2.iter().enumerate() {
        caixa(d, M + i as f32 * w, y + 20.0, w, 20.0, r, &v(k), OpcCaixa { negrito: i == 5, ..direita() });
    }
    y + 40.0 + 4.0
}

fn bloco_transporte(d: &mut Documento, n: &NotaDanfe, y: f32) -> f32 {
    let y = titulo_bloco(d, y, "TRANSPORTADOR / VOLUMES TRANSPORTADOS");
    let o = OpcCaixa::default();
    let tr = &n.transportadora;
    caixa(d, M, y, 230.0, 20.0, "Nome / razão social", &tr.x_nome, o);
    caixa(d, M + 230.0, y, 110.0, 20.0, "Frete por conta", &modalidade_frete(&n.mod_frete), o);
    caixa(d, M + 340.0, y, 70.0, 20.0, "Placa do veículo", &n.placa, o);
    caixa(d, M + 410.0, y, 30.0, 20.0, "UF", &n.uf_veiculo, o);
    caixa(d, M + 440.0, y, LARG - 440.0, 20.0, "CNPJ / CPF", &tr.documento, o);
    y + 20.0 + 4.0
}

type Coluna = (&'static str, f32, Alinhar, fn(&ItemDanfe) -> String);

/// Colunas da tabela de produtos: rótulo, largura (0 = resto), alinhamento e valor.
const COLUNAS: [Coluna; 12] = [
    ("Código", 44.0, Alinhar::Esquerda, |i| i.c_prod.clone()),
    ("Descrição do produto / serviço", 150.0, Alinhar::Esquerda, |i| {
        if i.inf_ad_prod.is_empty() { i.x_prod.clone() } else { format!("{}\n{}", i.x_prod, i.inf_ad_prod) }
    }),
    ("NCM/SH", 38.0, Alinhar::Centro, |i| i.ncm.clone()),
    ("CST", 22.0, Alinhar::Centro, |i| i.cst.clone()),
    ("CFOP", 24.0, Alinhar::Centro, |i| i.cfop.clone()),
    ("Un", 20.0, Alinhar::Centro, |i| i.u_com.clone()),
    ("Quant.", 40.0, Alinhar::Direita, |i| br(num(&i.q_com), 0, 4)),
    ("Valor unit.", 44.0, Alinhar::Direita, |i| br(num(&i.v_un_com), 2, 10)),
    ("Valor total", 46.0, Alinhar::Direita, |i| dinheiro(&i.v_prod)),
    ("B. cálc. ICMS", 40.0, Alinhar::Direita, |i| dinheiro(&i.v_bc)),
    ("V. ICMS", 34.0, Alinhar::Direita, |i| dinheiro(&i.v_icms)),
    ("Alíq. ICMS", 0.0, Alinhar::Direita, |i| if i.p_icms.is_empty() { "0".into() } else { br(num(&i.p_icms), 0, 3) }),
];

fn larguras_colunas() -> Vec<f32> {
    let usado: f32 = COLUNAS.iter().map(|c| c.1).sum();
    COLUNAS.iter().map(|c| if c.1 == 0.0 { LARG - usado } else { c.1 }).collect()
}

fn cabecalho_produtos(d: &mut Documento, y: f32) -> f32 {
    let y = titulo_bloco(d, y, "DADOS DOS PRODUTOS / SERVIÇOS");
    let mut x = M;
    for (c, w) in COLUNAS.iter().zip(larguras_colunas()) {
        d.retangulo(x, y, w, 14.0, 0.5);
        d.texto(&c.0.to_uppercase(), x + 1.0, y + 4.5, Txt::new(Fonte::Negrito, 5.4).largura(w - 2.0).centro().sem_quebra());
        x += w;
    }
    y + 14.0
}

fn altura_linha_produto(d: &Documento, item: &ItemDanfe) -> f32 {
    COLUNAS
        .iter()
        .zip(larguras_colunas())
        .map(|(c, w)| d.altura_texto(&(c.3)(item), &Txt::new(Fonte::Normal, 6.5).largura(w - 4.0)))
        .fold(0.0, f32::max)
        + 5.0
}

fn linha_produto(d: &mut Documento, item: &ItemDanfe, y: f32) -> f32 {
    let h = altura_linha_produto(d, item);
    let mut x = M;
    for (c, w) in COLUNAS.iter().zip(larguras_colunas()) {
        d.texto(&(c.3)(item), x + 2.0, y + 2.5, Txt::new(Fonte::Normal, 6.5).largura(w - 4.0).alinhar(c.2));
        x += w;
    }
    d.linha(M, y + h, M + LARG, y + h, 0.3, CINZA, None);
    y + h
}

fn bloco_adicionais(d: &mut Documento, n: &NotaDanfe, y: f32, h: f32) {
    let y = titulo_bloco(d, y, "DADOS ADICIONAIS");
    let referenciadas = n.referenciadas.iter().map(|c| format!("NF-e referenciada: {c}")).collect::<Vec<_>>().join("\n");
    let texto = [referenciadas.as_str(), n.inf_ad_fisco.as_str(), n.inf_cpl.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    caixa(d, M, y, LARG * 0.66, h, "Informações complementares", &texto, OpcCaixa { quebrar: true, tamanho: Some(6.8), ..Default::default() });
    caixa(d, M + LARG * 0.66, y, LARG * 0.34, h, "Reservado ao fisco", "", OpcCaixa::default());
}

/// Desenha a nota inteira; `total_folhas` aparece em "FOLHA x/n".
fn desenhar(n: &NotaDanfe, total_folhas: usize, cancelada: bool) -> Documento {
    const ALTURA_ADICIONAIS: f32 = 70.0;
    let fim_pagina = 842.0 - M - 12.0; // reserva a linha de aviso de homologação
    let mut d = Documento::new(&format!("DANFE {}", n.chave));
    let mut folha = 1;
    let rotulo = |f: usize| format!("{f}/{total_folhas}");

    let mut y = canhoto(&mut d, n, M);
    y = cabecalho(&mut d, n, y, &rotulo(folha));
    y = bloco_destinatario(&mut d, n, y);
    y = bloco_pagamento(&mut d, n, y);
    y = bloco_imposto(&mut d, n, y);
    y = bloco_transporte(&mut d, n, y);
    y = cabecalho_produtos(&mut d, y);

    for (i, item) in n.itens.iter().enumerate() {
        let h = altura_linha_produto(&d, item);
        let ultimo = i == n.itens.len() - 1;
        // Na última página precisa sobrar espaço para os dados adicionais.
        let limite = fim_pagina - if ultimo { ALTURA_ADICIONAIS + 12.0 } else { 0.0 };
        if y + h > limite {
            d.nova_pagina();
            folha += 1;
            let yc = cabecalho(&mut d, n, M, &rotulo(folha));
            y = cabecalho_produtos(&mut d, yc);
        }
        y = linha_produto(&mut d, item, y);
    }
    if y + ALTURA_ADICIONAIS + 12.0 > fim_pagina {
        d.nova_pagina();
        folha += 1;
        cabecalho(&mut d, n, M, &rotulo(folha));
    }
    bloco_adicionais(&mut d, n, fim_pagina - ALTURA_ADICIONAIS - 8.0, ALTURA_ADICIONAIS);

    // Marcas d'água e aviso de homologação em todas as páginas.
    for p in 0..d.total_paginas() {
        d.ir_para_pagina(p);
        if cancelada {
            marca_dagua(&mut d, "CANCELADA");
        } else if n.protocolo.is_none() {
            marca_dagua(&mut d, "NÃO AUTORIZADA");
        }
        if n.tp_amb == "2" {
            d.texto(
                "EMITIDA EM AMBIENTE DE HOMOLOGAÇÃO - SEM VALOR FISCAL",
                M,
                842.0 - M - 9.0,
                Txt::new(Fonte::Negrito, 9.0).largura(LARG).centro().sem_quebra().cor(VERMELHO),
            );
        }
    }
    d
}

/// PDF do DANFE a partir da NFe assinada ou do nfeProc.
pub fn gerar_danfe(xml: &str, cancelada: bool) -> Result<Vec<u8>> {
    let n = ler_nota_para_danfe(xml)?;
    // 1ª passada só para saber o total de folhas ("FOLHA 1/2").
    let total = desenhar(&n, 0, cancelada).total_paginas();
    Ok(desenhar(&n, total, cancelada).finalizar())
}

/// Quantidade de páginas de um PDF gerado aqui (para testes).
pub fn contar_paginas(pdf: &[u8]) -> usize {
    let s = String::from_utf8_lossy(pdf);
    s.matches("/Type /Page").count() - s.matches("/Type /Pages").count()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn numeros_no_formato_brasileiro() {
        assert_eq!(br(66250.0, 2, 2), "66.250,00");
        assert_eq!(br(1234567.891, 2, 2), "1.234.567,89");
        assert_eq!(br(500.0, 0, 4), "500");
        assert_eq!(br(0.335, 2, 10), "0,335");
        assert_eq!(br(132.5, 2, 10), "132,50");
        assert_eq!(br(12.0, 0, 3), "12");
        assert_eq!(br(-5.5, 2, 2), "-5,50");
        assert_eq!(numero_nota("1"), "000.000.001");
        assert_eq!(chave_formatada("12345678"), "1234 5678");
        assert_eq!(cep("74000000"), "74000-000");
    }
}
