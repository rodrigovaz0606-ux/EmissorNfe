//! Geração do XML da NF-e modelo 55, leiaute 4.00 (ainda sem assinatura).
//!
//! O XML é montado tag a tag porque a ordem dos elementos é obrigatória no schema.
//! Tags sem valor (ausentes ou texto vazio) são omitidas (campos opcionais).
//!
//! Tributação suportada por enquanto:
//!   ICMS: 00 (tributada integralmente), 40/41/50 (isenta/não tributada/suspensão), 51 (diferimento, sem valores)
//!   PIS/COFINS: 01/02 (alíquota), 04 a 09 (não tributado)
//!
//! Os dados chegam do formulário em JSON; números podem vir como número ou texto.

#![allow(non_snake_case)]

use super::chave::{ParametrosChave, montar_chave, validar_chave};
use super::config::Config;
use super::documentos::{cnpj_valido, cpf_valido};
use super::formatos::*;
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const NOME_DEST_HOMOLOGACAO: &str = "NF-E EMITIDA EM AMBIENTE DE HOMOLOGACAO - SEM VALOR FISCAL";
const VER_PROC: &str = "EmissorNfe 0.2";

/// Valor vindo do JSON (texto ou número).
pub type V = Option<Value>;

/// Valor como texto ("" e null contam como ausente).
pub fn s(v: &V) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if t.is_empty() => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(Value::Number(n)) => Some(n.as_f64().map(numero_js).unwrap_or_else(|| n.to_string())),
        Some(outro) => Some(outro.to_string()),
    }
}

/// Valor numérico. Ausente → None; texto inválido → erro.
pub fn n(v: &V) -> Result<Option<f64>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(x)) => Ok(x.as_f64()),
        Some(Value::String(t)) if t.trim().is_empty() => Ok(None),
        Some(Value::String(t)) => match t.trim().replace(',', ".").parse() {
            Ok(x) => Ok(Some(x)),
            Err(_) => bail!("Número inválido: \"{t}\""),
        },
        Some(outro) => bail!("Número inválido: {outro}"),
    }
}

fn centavos_de(v: &V) -> Result<i64> {
    Ok(n(v)?.map(para_centavos).unwrap_or(0))
}

fn digitos(v: &V) -> Option<String> {
    s(v).map(|t| so_digitos(&t)).filter(|t| !t.is_empty())
}

fn limpo(v: &V) -> Option<String> {
    s(v).map(|t| texto(&t)).filter(|t| !t.is_empty())
}

fn pad2(v: &V) -> String {
    format!("{:0>2}", s(v).unwrap_or_default())
}

// ---------- estrutura dos dados ----------

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Endereco {
    #[serde(default)]
    pub xLgr: V,
    #[serde(default)]
    pub nro: V,
    #[serde(default)]
    pub xCpl: V,
    #[serde(default)]
    pub xBairro: V,
    #[serde(default)]
    pub cMun: V,
    #[serde(default)]
    pub xMun: V,
    #[serde(default)]
    pub UF: V,
    #[serde(default)]
    pub CEP: V,
    #[serde(default)]
    pub cPais: V,
    #[serde(default)]
    pub xPais: V,
    #[serde(default)]
    pub fone: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Pessoa {
    #[serde(default)]
    pub CNPJ: V,
    #[serde(default)]
    pub CPF: V,
    #[serde(default)]
    pub xNome: V,
    #[serde(default)]
    pub xFant: V,
    #[serde(default)]
    pub ender: Option<Endereco>,
    #[serde(default)]
    pub IE: V,
    #[serde(default)]
    pub CRT: V,
    #[serde(default)]
    pub indIEDest: V,
    #[serde(default)]
    pub email: V,
}

impl Pessoa {
    /// CNPJ ou CPF, só os dígitos.
    pub fn documento(&self) -> Option<String> {
        digitos(&self.CNPJ).or_else(|| digitos(&self.CPF))
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Ide {
    #[serde(default)]
    pub natOp: V,
    #[serde(default)]
    pub serie: V,
    #[serde(default)]
    pub nNF: V,
    #[serde(default)]
    pub dhEmi: V,
    #[serde(default)]
    pub dhSaiEnt: V,
    #[serde(default)]
    pub tpNF: V,
    #[serde(default)]
    pub idDest: V,
    #[serde(default)]
    pub cMunFG: V,
    #[serde(default)]
    pub tpImp: V,
    #[serde(default)]
    pub finNFe: V,
    #[serde(default)]
    pub indFinal: V,
    #[serde(default)]
    pub indPres: V,
    #[serde(default)]
    pub indIntermed: V,
    #[serde(default)]
    pub procEmi: V,
    #[serde(default)]
    pub verProc: V,
    #[serde(default)]
    pub cNF: V,
    /// Chaves de NF-e referenciadas (devolução, complementar, ajuste).
    #[serde(default)]
    pub refNFe: Vec<Value>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Icms {
    #[serde(default)]
    pub orig: V,
    #[serde(default)]
    pub CST: V,
    #[serde(default)]
    pub modBC: V,
    #[serde(default)]
    pub pICMS: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct PisCofins {
    #[serde(default)]
    pub CST: V,
    #[serde(default)]
    pub pPIS: V,
    #[serde(default)]
    pub pCOFINS: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Item {
    #[serde(default)]
    pub cProd: V,
    #[serde(default)]
    pub cEAN: V,
    #[serde(default)]
    pub xProd: V,
    #[serde(default)]
    pub NCM: V,
    #[serde(default)]
    pub CEST: V,
    #[serde(default)]
    pub cBenef: V,
    #[serde(default)]
    pub CFOP: V,
    #[serde(default)]
    pub uCom: V,
    #[serde(default)]
    pub qCom: V,
    #[serde(default)]
    pub vUnCom: V,
    #[serde(default)]
    pub cEANTrib: V,
    #[serde(default)]
    pub uTrib: V,
    #[serde(default)]
    pub qTrib: V,
    #[serde(default)]
    pub vUnTrib: V,
    #[serde(default)]
    pub vFrete: V,
    #[serde(default)]
    pub vSeg: V,
    #[serde(default)]
    pub vDesc: V,
    #[serde(default)]
    pub vOutro: V,
    #[serde(default)]
    pub infAdProd: V,
    #[serde(default)]
    pub icms: Option<Icms>,
    #[serde(default)]
    pub pis: Option<PisCofins>,
    #[serde(default)]
    pub cofins: Option<PisCofins>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Transp {
    #[serde(default)]
    pub modFrete: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Pag {
    #[serde(default)]
    pub indPag: V,
    #[serde(default)]
    pub tPag: V,
    #[serde(default)]
    pub xPag: V,
    #[serde(default)]
    pub vPag: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct InfAdic {
    #[serde(default)]
    pub infAdFisco: V,
    #[serde(default)]
    pub infCpl: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RespTec {
    #[serde(default)]
    pub CNPJ: V,
    #[serde(default)]
    pub xContato: V,
    #[serde(default)]
    pub email: V,
    #[serde(default)]
    pub fone: V,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DadosNFe {
    pub ide: Option<Ide>,
    pub emit: Option<Pessoa>,
    pub dest: Option<Pessoa>,
    #[serde(default)]
    pub itens: Vec<Item>,
    #[serde(default)]
    pub transp: Option<Transp>,
    #[serde(default)]
    pub pag: Option<Vec<Pag>>,
    #[serde(default)]
    pub infAdic: Option<InfAdic>,
    #[serde(default)]
    pub respTec: Option<RespTec>,
}

// ---------- helpers de montagem ----------

fn tag(nome: &str, valor: Option<impl AsRef<str>>) -> String {
    match valor {
        Some(v) if !v.as_ref().is_empty() => format!("<{nome}>{}</{nome}>", escapar_xml(v.as_ref())),
        _ => String::new(),
    }
}

fn t(nome: &str, valor: &str) -> String {
    tag(nome, Some(valor))
}

/// Grupo: só aparece se tiver conteúdo.
fn grupo(nome: &str, filhos: &[String]) -> String {
    let conteudo = filhos.concat();
    if conteudo.is_empty() { conteudo } else { format!("<{nome}>{conteudo}</{nome}>") }
}

/// Valor informado ou o padrão.
fn ou(v: &V, padrao: &str) -> String {
    s(v).unwrap_or_else(|| padrao.to_string())
}

fn documento(p: &Pessoa, quem: &str) -> Result<String> {
    if let Some(cnpj) = s(&p.CNPJ) {
        exigir(cnpj_valido(&cnpj), format!("CNPJ do {quem} inválido: {cnpj}"))?;
        return Ok(t("CNPJ", &so_digitos(&cnpj)));
    }
    if let Some(cpf) = s(&p.CPF) {
        exigir(cpf_valido(&cpf), format!("CPF do {quem} inválido: {cpf}"))?;
        return Ok(t("CPF", &so_digitos(&cpf)));
    }
    bail!("Informe CNPJ ou CPF do {quem}.")
}

fn endereco(nome: &str, e: Option<&Endereco>) -> Result<String> {
    let Some(e) = e else { bail!("Endereço ({nome}) não informado.") };
    // No schema, CEP é obrigatório no endereço do emitente (TEnderEmi) e opcional no do destinatário.
    if nome == "enderEmit" {
        exigir(digitos(&e.CEP).is_some_and(|c| c.len() == 8), "CEP do emitente é obrigatório (8 dígitos).")?;
    }
    let nro = match &e.nro {
        None | Some(Value::Null) => Some("SN".to_string()),
        v => limpo(v),
    };
    Ok(grupo(
        nome,
        &[
            tag("xLgr", limpo(&e.xLgr)),
            tag("nro", nro),
            tag("xCpl", limpo(&e.xCpl)),
            tag("xBairro", limpo(&e.xBairro)),
            tag("cMun", digitos(&e.cMun)),
            tag("xMun", limpo(&e.xMun)),
            tag("UF", s(&e.UF)),
            tag("CEP", digitos(&e.CEP)),
            t("cPais", &ou(&e.cPais, "1058")),
            t("xPais", &ou(&e.xPais, "BRASIL")),
            tag("fone", digitos(&e.fone)),
        ],
    ))
}

// ---------- blocos ----------

fn bloco_ide(ide: &Ide, dh_emi: &str, config: &Config, c_nf: &str, c_dv: u32) -> Result<String> {
    // indIntermed é obrigatório quando a venda não é presencial (indPres 2, 3, 4 ou 9).
    let ind_pres = n(&ide.indPres)?.unwrap_or(1.0);
    let ind_intermed = s(&ide.indIntermed).or_else(|| [2.0, 3.0, 4.0, 9.0].contains(&ind_pres).then(|| "0".into()));
    let inteiro = |v: &V| -> Result<Option<String>> { Ok(n(v)?.map(numero_js)) };
    let fin_nfe = ou(&ide.finNFe, "1");
    let mut referencias = vec![];
    for r in &ide.refNFe {
        let chave = s(&Some(r.clone())).map(|c| so_digitos(&c)).unwrap_or_default();
        if !validar_chave(&chave) {
            bail!("Chave da NF-e referenciada inválida: {chave}");
        }
        referencias.push(grupo("NFref", &[t("refNFe", &chave)]));
    }
    if referencias.is_empty() && matches!(fin_nfe.as_str(), "2" | "4") {
        bail!("Nota complementar ou de devolução precisa da chave da NF-e referenciada.");
    }
    if referencias.len() > 500 {
        bail!("Uma NF-e aceita no máximo 500 documentos referenciados.");
    }
    let mut campos = vec![
            t("cUF", &config.c_uf.to_string()),
            t("cNF", c_nf),
            tag("natOp", limpo(&ide.natOp)),
            t("mod", "55"),
            tag("serie", inteiro(&ide.serie)?),
            tag("nNF", inteiro(&ide.nNF)?),
            t("dhEmi", dh_emi),
            tag("dhSaiEnt", s(&ide.dhSaiEnt)),
            t("tpNF", &ou(&ide.tpNF, "1")),
            t("idDest", &ou(&ide.idDest, "1")),
            tag("cMunFG", digitos(&ide.cMunFG)),
            t("tpImp", &ou(&ide.tpImp, "1")),
            t("tpEmis", "1"),
            t("cDV", &c_dv.to_string()),
            t("tpAmb", &config.tp_amb.to_string()),
            t("finNFe", &ou(&ide.finNFe, "1")),
            t("indFinal", &ou(&ide.indFinal, "0")),
            t("indPres", &ou(&ide.indPres, "1")),
            tag("indIntermed", ind_intermed),
            t("procEmi", &ou(&ide.procEmi, "0")),
            t("verProc", &limpo(&ide.verProc).unwrap_or_else(|| VER_PROC.into())),
    ];
    campos.extend(referencias);
    Ok(grupo("ide", &campos))
}

fn bloco_emit(emit: &Pessoa) -> Result<String> {
    Ok(grupo(
        "emit",
        &[
            documento(emit, "emitente")?,
            tag("xNome", limpo(&emit.xNome)),
            tag("xFant", limpo(&emit.xFant)),
            endereco("enderEmit", emit.ender.as_ref())?,
            tag("IE", digitos(&emit.IE)),
            t("CRT", &ou(&emit.CRT, "3")),
        ],
    ))
}

fn bloco_dest(dest: &Pessoa, config: &Config) -> Result<String> {
    let x_nome = if config.tp_amb == 2 { Some(NOME_DEST_HOMOLOGACAO.to_string()) } else { limpo(&dest.xNome) };
    let ind_ie_dest = match n(&dest.indIEDest)? {
        Some(v) => v as i64,
        None if s(&dest.IE).is_some() => 1,
        None => 9,
    };
    let ender = match &dest.ender {
        Some(e) => endereco("enderDest", Some(e))?,
        None => String::new(),
    };
    Ok(grupo(
        "dest",
        &[
            documento(dest, "destinatário")?,
            tag("xNome", x_nome),
            ender,
            t("indIEDest", &ind_ie_dest.to_string()),
            tag("IE", if ind_ie_dest == 1 { digitos(&dest.IE) } else { None }),
            tag("email", limpo(&dest.email)),
        ],
    ))
}

struct Imposto {
    xml: String,
    v_bc: i64,
    valor: i64,
}

/// base: valor do produto − desconto + frete + seguro + outras despesas.
fn bloco_icms(icms: &Icms, n_item: usize, base: i64) -> Result<Imposto> {
    let orig = ou(&icms.orig, "0");
    let cst = pad2(&icms.CST);
    match cst.as_str() {
        "00" => {
            let Some(p) = n(&icms.pICMS)?.filter(|p| p.is_finite()) else { bail!("Item {n_item}: ICMS 00 exige pICMS.") };
            let v_icms = arredondar(base as f64 * p / 100.0);
            let xml = grupo(
                "ICMS",
                &[grupo(
                    "ICMS00",
                    &[
                        t("orig", &orig),
                        t("CST", "00"),
                        t("modBC", &ou(&icms.modBC, "3")),
                        t("vBC", &centavos(base)),
                        t("pICMS", &fixo(p, 2)),
                        t("vICMS", &centavos(v_icms)),
                    ],
                )],
            );
            Ok(Imposto { xml, v_bc: base, valor: v_icms })
        }
        "40" | "41" | "50" => Ok(Imposto {
            xml: grupo("ICMS", &[grupo("ICMS40", &[t("orig", &orig), t("CST", &cst)])]),
            v_bc: 0,
            valor: 0,
        }),
        "51" => Ok(Imposto {
            xml: grupo("ICMS", &[grupo("ICMS51", &[t("orig", &orig), t("CST", "51")])]),
            v_bc: 0,
            valor: 0,
        }),
        _ => bail!("Item {n_item}: CST de ICMS \"{}\" ainda não suportado.", s(&icms.CST).unwrap_or_default()),
    }
}

/// PIS e COFINS têm a mesma estrutura; muda só o nome do grupo e das tags.
/// base: valor do produto − desconto.
fn bloco_pis_cofins(nome: &str, dados: &PisCofins, base: i64, n_item: usize) -> Result<Imposto> {
    let cst = pad2(&dados.CST);
    let (tag_p, tag_v, aliquota) = if nome == "PIS" { ("pPIS", "vPIS", &dados.pPIS) } else { ("pCOFINS", "vCOFINS", &dados.pCOFINS) };
    match cst.as_str() {
        "01" | "02" => {
            let Some(p) = n(aliquota)?.filter(|p| p.is_finite()) else { bail!("Item {n_item}: {nome} CST {cst} exige {tag_p}.") };
            let valor = arredondar(base as f64 * p / 100.0);
            let xml = grupo(
                nome,
                &[grupo(
                    &format!("{nome}Aliq"),
                    &[t("CST", &cst), t("vBC", &centavos(base)), t(tag_p, &fixo(p, 4)), t(tag_v, &centavos(valor))],
                )],
            );
            Ok(Imposto { xml, v_bc: base, valor })
        }
        "04" | "05" | "06" | "07" | "08" | "09" => Ok(Imposto {
            xml: grupo(nome, &[grupo(&format!("{nome}NT"), &[t("CST", &cst)])]),
            v_bc: 0,
            valor: 0,
        }),
        _ => bail!("Item {n_item}: CST de {nome} \"{}\" ainda não suportado.", s(&dados.CST).unwrap_or_default()),
    }
}

#[derive(Default, Clone, Copy)]
struct Totais {
    v_prod: i64,
    v_frete: i64,
    v_seg: i64,
    v_desc: i64,
    v_outro: i64,
    v_bc: i64,
    v_icms: i64,
    v_pis: i64,
    v_cofins: i64,
}

fn bloco_det(it: &Item, n_item: usize) -> Result<(String, Totais)> {
    exigir(limpo(&it.xProd).is_some(), format!("Item {n_item}: xProd obrigatório."))?;
    exigir(digitos(&it.NCM).is_some_and(|d| d.len() == 8), format!("Item {n_item}: NCM deve ter 8 dígitos."))?;
    exigir(digitos(&it.CFOP).is_some_and(|d| d.len() == 4), format!("Item {n_item}: CFOP deve ter 4 dígitos."))?;
    let q_com = n(&it.qCom)?.unwrap_or(0.0);
    exigir(q_com > 0.0, format!("Item {n_item}: qCom deve ser maior que zero."))?;
    let v_un = n(&it.vUnCom)?.unwrap_or(f64::NAN);
    exigir(v_un.is_finite(), format!("Item {n_item}: informe o valor unitário."))?;

    let v_prod = multiplicar_em_centavos(q_com, v_un);
    let v_frete = centavos_de(&it.vFrete)?;
    let v_seg = centavos_de(&it.vSeg)?;
    let v_desc = centavos_de(&it.vDesc)?;
    let v_outro = centavos_de(&it.vOutro)?;
    let c_ean = s(&it.cEAN).unwrap_or_else(|| "SEM GTIN".into());
    let opcional = |c: i64| (c != 0).then(|| centavos(c));
    let q_trib = n(&it.qTrib)?.unwrap_or(q_com);
    let v_un_trib = n(&it.vUnTrib)?.unwrap_or(v_un);

    let prod = grupo(
        "prod",
        &[
            tag("cProd", limpo(&it.cProd)),
            t("cEAN", &c_ean),
            tag("xProd", limpo(&it.xProd)),
            tag("NCM", digitos(&it.NCM)),
            tag("CEST", digitos(&it.CEST)),
            tag("cBenef", s(&it.cBenef)),
            tag("CFOP", digitos(&it.CFOP)),
            tag("uCom", limpo(&it.uCom)),
            t("qCom", &fixo(q_com, 4)),
            t("vUnCom", &unitario(v_un)),
            t("vProd", &centavos(v_prod)),
            t("cEANTrib", &s(&it.cEANTrib).unwrap_or_else(|| c_ean.clone())),
            tag("uTrib", limpo(&it.uTrib).or_else(|| limpo(&it.uCom))),
            t("qTrib", &fixo(q_trib, 4)),
            t("vUnTrib", &unitario(v_un_trib)),
            tag("vFrete", opcional(v_frete)),
            tag("vSeg", opcional(v_seg)),
            tag("vDesc", opcional(v_desc)),
            tag("vOutro", opcional(v_outro)),
            t("indTot", "1"),
        ],
    );

    exigir(v_desc <= v_prod, format!("Item {n_item}: desconto maior que o valor do produto."))?;
    let base_icms = v_prod - v_desc + v_frete + v_seg + v_outro;
    let base_pis_cofins = v_prod - v_desc;
    let icms = bloco_icms(&it.icms.clone().unwrap_or_default(), n_item, base_icms)?;
    let pis = bloco_pis_cofins("PIS", &it.pis.clone().unwrap_or_default(), base_pis_cofins, n_item)?;
    let cofins = bloco_pis_cofins("COFINS", &it.cofins.clone().unwrap_or_default(), base_pis_cofins, n_item)?;

    let xml = format!(
        "<det nItem=\"{n_item}\">{prod}{}{}</det>",
        grupo("imposto", &[icms.xml, pis.xml, cofins.xml]),
        tag("infAdProd", limpo(&it.infAdProd)),
    );
    let _ = (pis.v_bc, cofins.v_bc);
    Ok((
        xml,
        Totais {
            v_prod,
            v_frete,
            v_seg,
            v_desc,
            v_outro,
            v_bc: icms.v_bc,
            v_icms: icms.valor,
            v_pis: pis.valor,
            v_cofins: cofins.valor,
        },
    ))
}

fn bloco_total(t_: &Totais) -> (i64, String) {
    let v_nf = t_.v_prod - t_.v_desc + t_.v_frete + t_.v_seg + t_.v_outro;
    let c = |v: i64| centavos(v);
    let xml = grupo(
        "total",
        &[grupo(
            "ICMSTot",
            &[
                t("vBC", &c(t_.v_bc)),
                t("vICMS", &c(t_.v_icms)),
                t("vICMSDeson", &c(0)),
                t("vFCP", &c(0)),
                t("vBCST", &c(0)),
                t("vST", &c(0)),
                t("vFCPST", &c(0)),
                t("vFCPSTRet", &c(0)),
                t("vProd", &c(t_.v_prod)),
                t("vFrete", &c(t_.v_frete)),
                t("vSeg", &c(t_.v_seg)),
                t("vDesc", &c(t_.v_desc)),
                t("vII", &c(0)),
                t("vIPI", &c(0)),
                t("vIPIDevol", &c(0)),
                t("vPIS", &c(t_.v_pis)),
                t("vCOFINS", &c(t_.v_cofins)),
                t("vOutro", &c(t_.v_outro)),
                t("vNF", &c(v_nf)),
            ],
        )],
    );
    (v_nf, xml)
}

fn bloco_pag(pagamentos: Option<&Vec<Pag>>, v_nf: i64) -> Result<String> {
    let padrao = vec![Pag { tPag: Some(Value::String("90".into())), ..Default::default() }];
    let lista = match pagamentos {
        Some(l) if !l.is_empty() => l,
        _ => &padrao,
    };
    let mut detalhes = vec![];
    for p in lista {
        let t_pag = pad2(&p.tPag);
        // tPag 90 = sem pagamento: vPag deve ser zero. Pagamento único sem valor = valor da nota.
        let v_pag = if t_pag == "90" {
            Some(0)
        } else if p.vPag.as_ref().is_some_and(|v| !v.is_null()) {
            Some(centavos_de(&p.vPag)?)
        } else if lista.len() == 1 {
            Some(v_nf)
        } else {
            None
        };
        let Some(v_pag) = v_pag else { bail!("Com mais de uma forma de pagamento, informe vPag em cada uma.") };
        detalhes.push(grupo(
            "detPag",
            &[
                tag("indPag", s(&p.indPag)),
                t("tPag", &t_pag),
                tag("xPag", if t_pag == "99" { limpo(&p.xPag) } else { None }),
                t("vPag", &centavos(v_pag)),
            ],
        ));
    }
    Ok(grupo("pag", &detalhes))
}

#[derive(Debug, Clone)]
pub struct NFeGerada {
    pub xml: String,
    pub chave: String,
    pub v_nf: String,
}

/// Monta o XML da NF-e (sem assinatura).
pub fn gerar_xml_nfe(dados: &DadosNFe, config: &Config) -> Result<NFeGerada> {
    exigir(config.tp_amb == 2, "Somente homologação (tpAmb=2) é permitida nesta versão.")?;
    let (Some(ide), Some(emit), Some(dest)) = (&dados.ide, &dados.emit, &dados.dest) else {
        bail!("Informe ide, emit e dest.");
    };
    exigir(!dados.itens.is_empty() && dados.itens.len() <= 990, "A nota precisa ter de 1 a 990 itens.")?;

    let dh_emi = s(&ide.dhEmi).unwrap_or_else(agora);
    let doc_emit = s(&emit.CNPJ).or_else(|| s(&emit.CPF)).unwrap_or_default();
    let chave = montar_chave(&ParametrosChave {
        c_uf: &config.c_uf.to_string(),
        dh_emi: &dh_emi,
        documento: &doc_emit,
        serie: &s(&ide.serie).unwrap_or_default(),
        n_nf: &s(&ide.nNF).unwrap_or_default(),
        c_nf: s(&ide.cNF).as_deref(),
    })?;

    let mut dets = String::new();
    let mut tot = Totais::default();
    for (i, item) in dados.itens.iter().enumerate() {
        let (xml, t_) = bloco_det(item, i + 1)?;
        dets.push_str(&xml);
        tot.v_prod += t_.v_prod;
        tot.v_frete += t_.v_frete;
        tot.v_seg += t_.v_seg;
        tot.v_desc += t_.v_desc;
        tot.v_outro += t_.v_outro;
        tot.v_bc += t_.v_bc;
        tot.v_icms += t_.v_icms;
        tot.v_pis += t_.v_pis;
        tot.v_cofins += t_.v_cofins;
    }
    let (v_nf, total) = bloco_total(&tot);
    exigir(v_nf >= 0, "Valor total da nota ficou negativo.")?;

    let transp = grupo("transp", &[t("modFrete", &dados.transp.as_ref().map(|t_| ou(&t_.modFrete, "9")).unwrap_or_else(|| "9".into()))]);
    let inf_adic = dados.infAdic.clone().unwrap_or_default();
    let resp_tec = match &dados.respTec {
        Some(r) => grupo(
            "infRespTec",
            &[tag("CNPJ", digitos(&r.CNPJ)), tag("xContato", limpo(&r.xContato)), tag("email", limpo(&r.email)), tag("fone", digitos(&r.fone))],
        ),
        None => String::new(),
    };

    let inf_nfe = format!(
        "<infNFe Id=\"NFe{}\" versao=\"4.00\">{}{}{}{dets}{total}{transp}{}{}{resp_tec}</infNFe>",
        chave.chave,
        bloco_ide(ide, &dh_emi, config, &chave.c_nf, chave.c_dv)?,
        bloco_emit(emit)?,
        bloco_dest(dest, config)?,
        bloco_pag(dados.pag.as_ref(), v_nf)?,
        grupo("infAdic", &[tag("infAdFisco", limpo(&inf_adic.infAdFisco)), tag("infCpl", limpo(&inf_adic.infCpl))]),
    );

    Ok(NFeGerada {
        xml: format!("<NFe xmlns=\"http://www.portalfiscal.inf.br/nfe\">{inf_nfe}</NFe>"),
        chave: chave.chave,
        v_nf: centavos(v_nf),
    })
}
