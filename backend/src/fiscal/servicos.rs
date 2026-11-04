//! Serviços da SEFAZ (NF-e 4.00): status, autorização, eventos, consulta e inutilização.
//!
//! Cada serviço tem três partes: montar o XML de pedido, ler o XML de resposta
//! (funções puras, testáveis sem rede) e enviar (SOAP com o certificado).

#![allow(non_snake_case)]

use super::assinatura::assinar_xml;
use super::certificado::Certificado;
use super::chave::validar_chave;
use super::config::{Config, url_servico};
use super::formatos::{agora, escapar_xml, so_digitos, texto};
use super::soap::{Requisicao, enviar_soap, erro_rede};
use super::xml::{NS_NFE, encontrar, filho, filhos, ler, recortar_elemento, sem_declaracao, txt};
use super::xsd::Pacote;
use anyhow::{Result, bail};
use rand::Rng;
use roxmltree::Node;
use serde::Serialize;
use std::time::Duration;

fn enviar(config: &Config, cert: &Certificado, servico: &str, metodo: &str, xml: &str, segundos: u64) -> Result<String> {
    enviar_soap(&Requisicao {
        url: url_servico(&config.uf, servico)?,
        servico,
        metodo,
        xml_dados: xml,
        certificado: cert,
        tempo_limite: Duration::from_secs(segundos),
    })
}

fn inicio(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Localiza o elemento de retorno dentro do envelope SOAP e aplica `f`.
fn ler_retorno<T>(resposta: &str, nome: &str, f: impl FnOnce(Node) -> T) -> Result<T> {
    let doc = ler(resposta).map_err(|e| erro_rede(format!("Resposta da SEFAZ não é XML válido ({e}):\n{}", inicio(resposta, 1500))))?;
    match encontrar(doc.root(), nome) {
        Some(no) => Ok(f(no)),
        None => Err(erro_rede(format!("Resposta sem {nome}:\n{}", inicio(resposta, 1500)))),
    }
}

/// Identificador de lote com 15 dígitos.
pub fn gerar_id_lote() -> String {
    let ms = chrono::Utc::now().timestamp_millis().to_string();
    format!("{}{:03}", &ms[ms.len().saturating_sub(12)..], rand::thread_rng().gen_range(0..1000))
}

// ===========================================================================
// NFeStatusServico4
// ===========================================================================

#[derive(Debug, Serialize)]
pub struct RetStatus {
    pub tpAmb: Option<String>,
    pub verAplic: Option<String>,
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub cUF: Option<String>,
    pub dhRecbto: Option<String>,
    pub tMed: Option<String>,
    pub emOperacao: bool,
}

pub fn montar_cons_stat_serv(config: &Config) -> String {
    format!(
        "<consStatServ versao=\"4.00\" xmlns=\"{NS_NFE}\"><tpAmb>{}</tpAmb><cUF>{}</cUF><xServ>STATUS</xServ></consStatServ>",
        config.tp_amb, config.c_uf
    )
}

pub fn ler_ret_cons_stat_serv(resposta: &str) -> Result<RetStatus> {
    ler_retorno(resposta, "retConsStatServ", |r| RetStatus {
        tpAmb: txt(r, "tpAmb"),
        verAplic: txt(r, "verAplic"),
        cStat: txt(r, "cStat"),
        xMotivo: txt(r, "xMotivo"),
        cUF: txt(r, "cUF"),
        dhRecbto: txt(r, "dhRecbto"),
        tMed: txt(r, "tMed"),
        emOperacao: txt(r, "cStat").as_deref() == Some("107"),
    })
}

pub fn consultar_status_servico(config: &Config, cert: &Certificado) -> Result<RetStatus> {
    let r = enviar(config, cert, "NFeStatusServico4", "nfeStatusServicoNF", &montar_cons_stat_serv(config), 30)?;
    ler_ret_cons_stat_serv(&r)
}

// ===========================================================================
// NFeAutorizacao4 (+ NFeRetAutorizacao4)
//   104 lote processado (resultado da nota em protNFe/infProt)
//   100 autorizado o uso da NF-e      150 autorizado fora de prazo
//   103 lote recebido (assíncrono)    105 lote em processamento
// ===========================================================================

pub const AUTORIZADA: [&str; 2] = ["100", "150"];

#[derive(Debug, Clone, Serialize, Default)]
pub struct Lote {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub dhRecbto: Option<String>,
    pub verAplic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Protocolo {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub chNFe: Option<String>,
    pub nProt: Option<String>,
    pub dhRecbto: Option<String>,
    pub digVal: Option<String>,
    pub autorizada: bool,
    /// Texto original do protNFe, para o nfeProc.
    #[serde(skip)]
    pub xml: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RetAutorizacao {
    pub lote: Lote,
    pub recibo: Option<String>,
    pub protocolo: Option<Protocolo>,
    #[serde(skip)]
    pub enviado: String,
    #[serde(skip)]
    pub resposta_soap: String,
}

/// O XML da NF-e entra exatamente como foi assinado (sem declaração <?xml?>).
pub fn montar_envi_nfe(xml_assinado: &str, id_lote: &str) -> Result<String> {
    let nfe = sem_declaracao(xml_assinado);
    if !(nfe.starts_with("<NFe ") || nfe.starts_with("<NFe>")) || !nfe.contains("<Signature") {
        bail!("Esperado o XML <NFe> assinado.");
    }
    Ok(format!("<enviNFe versao=\"4.00\" xmlns=\"{NS_NFE}\"><idLote>{id_lote}</idLote><indSinc>1</indSinc>{nfe}</enviNFe>"))
}

fn ler_protocolo(resposta: &str, raiz: Node) -> Option<Protocolo> {
    let prot = encontrar(raiz, "protNFe")?;
    let inf = filho(prot, "infProt");
    let c = |n: &str| inf.and_then(|i| txt(i, n));
    let c_stat = c("cStat");
    Some(Protocolo {
        autorizada: c_stat.as_deref().is_some_and(|s| AUTORIZADA.contains(&s)),
        cStat: c_stat,
        xMotivo: c("xMotivo"),
        chNFe: c("chNFe"),
        nProt: c("nProt"),
        dhRecbto: c("dhRecbto"),
        digVal: c("digVal"),
        xml: recortar_elemento(resposta, "protNFe"),
    })
}

fn ler_lote(r: Node) -> Lote {
    Lote { cStat: txt(r, "cStat"), xMotivo: txt(r, "xMotivo"), dhRecbto: txt(r, "dhRecbto"), verAplic: txt(r, "verAplic") }
}

pub fn ler_ret_envi_nfe(resposta: &str) -> Result<RetAutorizacao> {
    ler_retorno(resposta, "retEnviNFe", |r| RetAutorizacao {
        lote: ler_lote(r),
        recibo: filho(r, "infRec").and_then(|i| txt(i, "nRec")),
        protocolo: ler_protocolo(resposta, r),
        resposta_soap: resposta.to_string(),
        ..Default::default()
    })
}

pub fn ler_ret_cons_reci_nfe(resposta: &str) -> Result<RetAutorizacao> {
    ler_retorno(resposta, "retConsReciNFe", |r| RetAutorizacao {
        lote: ler_lote(r),
        recibo: txt(r, "nRec"),
        protocolo: ler_protocolo(resposta, r),
        resposta_soap: resposta.to_string(),
        ..Default::default()
    })
}

pub fn consultar_recibo(config: &Config, cert: &Certificado, n_rec: &str) -> Result<RetAutorizacao> {
    let xml = format!("<consReciNFe versao=\"4.00\" xmlns=\"{NS_NFE}\"><tpAmb>{}</tpAmb><nRec>{n_rec}</nRec></consReciNFe>", config.tp_amb);
    ler_ret_cons_reci_nfe(&enviar(config, cert, "NFeRetAutorizacao4", "nfeRetAutorizacaoLote", &xml, 30)?)
}

/// Envia a NF-e assinada (lote síncrono de uma nota). Se a SEFAZ processar de forma
/// assíncrona (103), consulta o recibo até sair do "em processamento" (105).
pub fn autorizar_nfe(config: &Config, cert: &Certificado, xml_assinado: &str) -> Result<RetAutorizacao> {
    let enviado = montar_envi_nfe(xml_assinado, &gerar_id_lote())?;
    let resposta = enviar(config, cert, "NFeAutorizacao4", "nfeAutorizacaoLote", &enviado, 60)?;
    let mut r = ler_ret_envi_nfe(&resposta)?;
    r.enviado = enviado.clone();
    if r.lote.cStat.as_deref() == Some("103") {
        if let Some(recibo) = r.recibo.clone() {
            for _ in 0..5 {
                std::thread::sleep(Duration::from_secs(3));
                let c = consultar_recibo(config, cert, &recibo)?;
                let processando = c.lote.cStat.as_deref() == Some("105");
                r = RetAutorizacao { enviado: enviado.clone(), ..c };
                if !processando {
                    break;
                }
            }
        }
    }
    Ok(r)
}

/// nfeProc: NF-e assinada + protocolo de autorização. É o XML que deve ser guardado
/// e entregue ao destinatário.
pub fn montar_nfe_proc(xml_assinado: &str, prot_nfe: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><nfeProc versao=\"4.00\" xmlns=\"{NS_NFE}\">{}{prot_nfe}</nfeProc>",
        sem_declaracao(xml_assinado)
    )
}

// ===========================================================================
// NFeRecepcaoEvento4: cancelamento (110111) e Carta de Correção (110110)
//   135 registrado e vinculado · 136 registrado sem vínculo · 155 cancelamento fora de prazo
// ===========================================================================

pub const EVENTO_REGISTRADO: [&str; 3] = ["135", "136", "155"];

pub const COND_USO_CCE: &str = "A Carta de Correcao e disciplinada pelo paragrafo 1o-A do art. 7o do Convenio S/N, de 15 de dezembro de 1970 \
e pode ser utilizada para regularizacao de erro ocorrido na emissao de documento fiscal, desde que o erro nao esteja \
relacionado com: I - as variaveis que determinam o valor do imposto tais como: base de calculo, aliquota, diferenca \
de preco, quantidade, valor da operacao ou da prestacao; II - a correcao de dados cadastrais que implique mudanca do \
remetente ou do destinatario; III - a data de emissao ou de saida.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoEvento {
    Cancelamento,
    Cce,
}

impl TipoEvento {
    pub fn codigo(self) -> &'static str {
        match self {
            TipoEvento::Cancelamento => "110111",
            TipoEvento::Cce => "110110",
        }
    }
    pub fn nome(self) -> &'static str {
        match self {
            TipoEvento::Cancelamento => "cancelamento",
            TipoEvento::Cce => "cce",
        }
    }
    pub fn pacote(self) -> Pacote {
        match self {
            TipoEvento::Cancelamento => Pacote::Cancelamento,
            TipoEvento::Cce => Pacote::Cce,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DadosEvento {
    pub chave: String,
    pub documento: String,
    pub n_seq_evento: u32,
    pub n_prot: Option<String>,
    pub x_just: Option<String>,
    pub x_correcao: Option<String>,
    pub dh_evento: Option<String>,
}

fn detalhe_evento(tipo: TipoEvento, d: &DadosEvento) -> Result<String> {
    match tipo {
        TipoEvento::Cancelamento => {
            let n_prot = so_digitos(d.n_prot.as_deref().unwrap_or(""));
            let x_just = texto(d.x_just.as_deref().unwrap_or(""));
            if n_prot.len() != 15 {
                bail!("Cancelamento exige o protocolo de autorização (15 dígitos).");
            }
            let tam = x_just.chars().count();
            if !(15..=255).contains(&tam) {
                bail!("A justificativa do cancelamento deve ter de 15 a 255 caracteres.");
            }
            Ok(format!("<descEvento>Cancelamento</descEvento><nProt>{n_prot}</nProt><xJust>{}</xJust>", escapar_xml(&x_just)))
        }
        TipoEvento::Cce => {
            let x = texto(d.x_correcao.as_deref().unwrap_or(""));
            let tam = x.chars().count();
            if !(15..=1000).contains(&tam) {
                bail!("O texto da correção deve ter de 15 a 1000 caracteres.");
            }
            Ok(format!(
                "<descEvento>Carta de Correcao</descEvento><xCorrecao>{}</xCorrecao><xCondUso>{COND_USO_CCE}</xCondUso>",
                escapar_xml(&x)
            ))
        }
    }
}

/// Monta o <evento> (ainda sem assinatura).
pub fn montar_evento(tipo: TipoEvento, d: &DadosEvento, config: &Config) -> Result<String> {
    if config.tp_amb != 2 {
        bail!("Somente homologação (tpAmb=2) é permitida nesta versão.");
    }
    if !validar_chave(&d.chave) {
        bail!("Chave de acesso inválida.");
    }
    if !(1..=20).contains(&d.n_seq_evento) {
        bail!("nSeqEvento deve estar entre 1 e 20.");
    }
    let doc = so_digitos(&d.documento);
    let tag_doc = match doc.len() {
        14 => format!("<CNPJ>{doc}</CNPJ>"),
        11 => format!("<CPF>{doc}</CPF>"),
        _ => bail!("Documento do autor do evento deve ser CPF ou CNPJ."),
    };
    let tp = tipo.codigo();
    let id = format!("ID{tp}{}{:02}", d.chave, d.n_seq_evento);
    Ok(format!(
        "<evento versao=\"1.00\" xmlns=\"{NS_NFE}\"><infEvento Id=\"{id}\">\
         <cOrgao>{}</cOrgao><tpAmb>{}</tpAmb>{tag_doc}<chNFe>{}</chNFe>\
         <dhEvento>{}</dhEvento><tpEvento>{tp}</tpEvento><nSeqEvento>{}</nSeqEvento>\
         <verEvento>1.00</verEvento><detEvento versao=\"1.00\">{}</detEvento></infEvento></evento>",
        config.c_uf,
        config.tp_amb,
        d.chave,
        d.dh_evento.clone().unwrap_or_else(agora),
        d.n_seq_evento,
        detalhe_evento(tipo, d)?
    ))
}

/// O <evento> vai dentro do lote sem repetir o xmlns (herda do envEvento).
pub fn montar_env_evento(evento_assinado: &str, id_lote: &str) -> String {
    let evento = evento_assinado.replacen(&format!("<evento versao=\"1.00\" xmlns=\"{NS_NFE}\">"), "<evento versao=\"1.00\">", 1);
    format!("<envEvento versao=\"1.00\" xmlns=\"{NS_NFE}\"><idLote>{id_lote}</idLote>{evento}</envEvento>")
}

pub struct EventoPreparado {
    pub evento: String,
    pub env_evento: String,
}

/// Monta, assina e devolve o lote do evento (não envia).
pub fn preparar_evento(tipo: TipoEvento, d: &DadosEvento, config: &Config, cert: &Certificado) -> Result<EventoPreparado> {
    let evento = assinar_xml(&montar_evento(tipo, d, config)?, cert, "infEvento")?;
    let env_evento = montar_env_evento(&evento, &gerar_id_lote());
    Ok(EventoPreparado { evento, env_evento })
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LoteEvento {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct InfRetEvento {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub chNFe: Option<String>,
    pub tpEvento: Option<String>,
    pub nSeqEvento: Option<String>,
    pub dhRegEvento: Option<String>,
    pub nProt: Option<String>,
    pub registrado: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RetEvento {
    pub lote: LoteEvento,
    pub evento: Option<InfRetEvento>,
    pub xml_ret_evento: Option<String>,
    pub resposta_soap: String,
}

pub fn ler_ret_env_evento(resposta: &str) -> Result<RetEvento> {
    ler_retorno(resposta, "retEnvEvento", |r| {
        let inf = filho(r, "retEvento").and_then(|e| filho(e, "infEvento"));
        let evento = inf.filter(|i| txt(*i, "cStat").is_some()).map(|i| {
            let c_stat = txt(i, "cStat");
            InfRetEvento {
                registrado: c_stat.as_deref().is_some_and(|s| EVENTO_REGISTRADO.contains(&s)),
                cStat: c_stat,
                xMotivo: txt(i, "xMotivo"),
                chNFe: txt(i, "chNFe"),
                tpEvento: txt(i, "tpEvento"),
                nSeqEvento: txt(i, "nSeqEvento"),
                dhRegEvento: txt(i, "dhRegEvento"),
                nProt: txt(i, "nProt"),
            }
        });
        RetEvento {
            lote: LoteEvento { cStat: txt(r, "cStat"), xMotivo: txt(r, "xMotivo") },
            evento,
            xml_ret_evento: recortar_elemento(resposta, "retEvento"),
            resposta_soap: resposta.to_string(),
        }
    })
}

pub fn montar_proc_evento(evento_assinado: &str, ret_evento: &str) -> String {
    let evento = evento_assinado.replacen(&format!(" xmlns=\"{NS_NFE}\""), "", 1);
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><procEventoNFe versao=\"1.00\" xmlns=\"{NS_NFE}\">{evento}{ret_evento}</procEventoNFe>")
}

pub fn enviar_evento(config: &Config, cert: &Certificado, env_evento: &str) -> Result<RetEvento> {
    ler_ret_env_evento(&enviar(config, cert, "NFeRecepcaoEvento4", "nfeRecepcaoEvento", env_evento, 30)?)
}

// ===========================================================================
// NFeConsultaProtocolo4: situação atual da NF-e
//   100 autorizada · 101 cancelada · 110/301/302 denegada · 217 não consta na base
// ===========================================================================

#[derive(Debug, Clone, Serialize, Default)]
pub struct ProtocoloConsulta {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub nProt: Option<String>,
    pub dhRecbto: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct EventoConsulta {
    pub tpEvento: Option<String>,
    pub xEvento: Option<String>,
    pub cStat: Option<String>,
    pub nProt: Option<String>,
    pub dhRegEvento: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RetConsSit {
    pub cStat: Option<String>,
    pub xMotivo: Option<String>,
    pub chNFe: Option<String>,
    pub protocolo: Option<ProtocoloConsulta>,
    pub eventos: Vec<EventoConsulta>,
}

pub fn montar_cons_sit_nfe(chave: &str, tp_amb: u8) -> Result<String> {
    if !validar_chave(chave) {
        bail!("Chave de acesso inválida.");
    }
    Ok(format!(
        "<consSitNFe versao=\"4.00\" xmlns=\"{NS_NFE}\"><tpAmb>{tp_amb}</tpAmb><xServ>CONSULTAR</xServ><chNFe>{chave}</chNFe></consSitNFe>"
    ))
}

pub fn ler_ret_cons_sit_nfe(resposta: &str) -> Result<RetConsSit> {
    ler_retorno(resposta, "retConsSitNFe", |r| RetConsSit {
        cStat: txt(r, "cStat"),
        xMotivo: txt(r, "xMotivo"),
        chNFe: txt(r, "chNFe"),
        protocolo: filho(r, "protNFe").and_then(|p| filho(p, "infProt")).map(|i| ProtocoloConsulta {
            cStat: txt(i, "cStat"),
            xMotivo: txt(i, "xMotivo"),
            nProt: txt(i, "nProt"),
            dhRecbto: txt(i, "dhRecbto"),
        }),
        eventos: filhos(r, "procEventoNFe")
            .filter_map(|p| filho(p, "retEvento").and_then(|e| filho(e, "infEvento")))
            .map(|e| EventoConsulta {
                tpEvento: txt(e, "tpEvento"),
                xEvento: txt(e, "xEvento"),
                cStat: txt(e, "cStat"),
                nProt: txt(e, "nProt"),
                dhRegEvento: txt(e, "dhRegEvento"),
            })
            .collect(),
    })
}

pub fn consultar_situacao(config: &Config, cert: &Certificado, chave: &str) -> Result<RetConsSit> {
    let xml = montar_cons_sit_nfe(chave, config.tp_amb)?;
    ler_ret_cons_sit_nfe(&enviar(config, cert, "NFeConsultaProtocolo4", "nfeConsultaNF", &xml, 30)?)
}

// ===========================================================================
// NFeInutilizacao4: no leiaute 4.00 o emitente precisa ter CNPJ. 102 = homologada.
// ===========================================================================

#[derive(Debug, Clone, Default)]
pub struct DadosInutilizacao {
    pub cnpj: String,
    pub serie: i64,
    pub n_nf_ini: i64,
    pub n_nf_fin: i64,
    pub x_just: String,
    pub ano: Option<i32>,
}

pub fn montar_inut_nfe(d: &DadosInutilizacao, config: &Config) -> Result<String> {
    if config.tp_amb != 2 {
        bail!("Somente homologação (tpAmb=2) é permitida nesta versão.");
    }
    let cnpj = so_digitos(&d.cnpj);
    if cnpj.len() != 14 {
        bail!("A inutilização exige emitente com CNPJ (o leiaute 4.00 não aceita CPF).");
    }
    if !(0..=999).contains(&d.serie) {
        bail!("Série inválida.");
    }
    if !(d.n_nf_ini >= 1 && d.n_nf_fin >= d.n_nf_ini && d.n_nf_fin <= 999_999_999) {
        bail!("Faixa de numeração inválida.");
    }
    let x_just = texto(&d.x_just);
    if !(15..=255).contains(&x_just.chars().count()) {
        bail!("A justificativa deve ter de 15 a 255 caracteres.");
    }
    let ano = d.ano.unwrap_or_else(|| chrono::Datelike::year(&chrono::Local::now())) % 100;
    let id = format!("ID{}{ano:02}{cnpj}55{:03}{:09}{:09}", config.c_uf, d.serie, d.n_nf_ini, d.n_nf_fin);
    Ok(format!(
        "<inutNFe versao=\"4.00\" xmlns=\"{NS_NFE}\"><infInut Id=\"{id}\">\
         <tpAmb>{}</tpAmb><xServ>INUTILIZAR</xServ><cUF>{}</cUF><ano>{ano:02}</ano><CNPJ>{cnpj}</CNPJ>\
         <mod>55</mod><serie>{}</serie><nNFIni>{}</nNFIni><nNFFin>{}</nNFFin><xJust>{}</xJust></infInut></inutNFe>",
        config.tp_amb,
        config.c_uf,
        d.serie,
        d.n_nf_ini,
        d.n_nf_fin,
        escapar_xml(&x_just)
    ))
}

pub fn preparar_inutilizacao(d: &DadosInutilizacao, config: &Config, cert: &Certificado) -> Result<String> {
    assinar_xml(&montar_inut_nfe(d, config)?, cert, "infInut")
}

#[derive(Debug, Clone, Default)]
pub struct RetInut {
    pub c_stat: Option<String>,
    pub x_motivo: Option<String>,
    pub n_prot: Option<String>,
    pub dh_recbto: Option<String>,
    pub homologada: bool,
    pub xml_ret: Option<String>,
    pub resposta_soap: String,
}

pub fn ler_ret_inut_nfe(resposta: &str) -> Result<RetInut> {
    let r = ler_retorno(resposta, "retInutNFe", |r| {
        filho(r, "infInut").map(|i| RetInut {
            homologada: txt(i, "cStat").as_deref() == Some("102"),
            c_stat: txt(i, "cStat"),
            x_motivo: txt(i, "xMotivo"),
            n_prot: txt(i, "nProt"),
            dh_recbto: txt(i, "dhRecbto"),
            xml_ret: recortar_elemento(resposta, "retInutNFe"),
            resposta_soap: resposta.to_string(),
        })
    })?;
    r.ok_or_else(|| erro_rede(format!("Resposta sem retInutNFe:\n{}", inicio(resposta, 1500))))
}

pub fn montar_proc_inut(inut_assinado: &str, xml_ret: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><ProcInutNFe versao=\"4.00\" xmlns=\"{NS_NFE}\">{inut_assinado}{xml_ret}</ProcInutNFe>")
}

pub fn enviar_inutilizacao(config: &Config, cert: &Certificado, inut_assinado: &str) -> Result<RetInut> {
    ler_ret_inut_nfe(&enviar(config, cert, "NFeInutilizacao4", "nfeInutilizacaoNF", inut_assinado, 30)?)
}
