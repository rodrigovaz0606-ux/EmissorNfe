//! Regras de negócio usadas pela API: emitir, cancelar, corrigir, inutilizar e consultar.
//! Toda comunicação com a SEFAZ passa por aqui e o resultado é gravado no banco.

use crate::db::{NovaInutilizacao, NovaNota, NovoEvento, Repositorio, Resultado};
use crate::fiscal::assinatura::{assinar_nfe, verificar_assinatura};
use crate::fiscal::certificado::Certificado;
use crate::fiscal::config::Config;
use crate::fiscal::nfe::{DadosNFe, gerar_xml_nfe};
use crate::fiscal::servicos::{self, *};
use crate::fiscal::xsd::{Pacote, ResultadoXsd, validar_xsd};
use anyhow::Result;
use serde_json::{Value, json};
use crate::auth::Tentativas;
use crate::cofre::Cofre;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

/// Regra de negócio violada (vira HTTP 400 sem registrar como erro do sistema).
#[derive(Debug)]
pub struct ErroNegocio(pub String);

impl fmt::Display for ErroNegocio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ErroNegocio {}

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

const DENEGADA: [&str; 4] = ["110", "301", "302", "303"];

/// Chamadas à SEFAZ. A implementação real fala com os webservices; a simulada
/// (e as dos testes) devolve respostas prontas.
pub trait Sefaz: Send + Sync {
    fn simulada(&self) -> bool {
        false
    }
    fn status(&self, c: &Config, cert: &Certificado) -> Result<RetStatus>;
    fn autorizar(&self, c: &Config, cert: &Certificado, xml_assinado: &str) -> Result<RetAutorizacao>;
    fn evento(&self, c: &Config, cert: &Certificado, env_evento: &str) -> Result<RetEvento>;
    fn inutilizar(&self, c: &Config, cert: &Certificado, inut_assinado: &str) -> Result<RetInut>;
    fn consultar(&self, c: &Config, cert: &Certificado, chave: &str) -> Result<RetConsSit>;
}

pub struct SefazReal;

impl Sefaz for SefazReal {
    fn status(&self, c: &Config, cert: &Certificado) -> Result<RetStatus> {
        servicos::consultar_status_servico(c, cert)
    }
    fn autorizar(&self, c: &Config, cert: &Certificado, xml: &str) -> Result<RetAutorizacao> {
        servicos::autorizar_nfe(c, cert, xml)
    }
    fn evento(&self, c: &Config, cert: &Certificado, env: &str) -> Result<RetEvento> {
        servicos::enviar_evento(c, cert, env)
    }
    fn inutilizar(&self, c: &Config, cert: &Certificado, inut: &str) -> Result<RetInut> {
        servicos::enviar_inutilizacao(c, cert, inut)
    }
    fn consultar(&self, c: &Config, cert: &Certificado, chave: &str) -> Result<RetConsSit> {
        servicos::consultar_situacao(c, cert, chave)
    }
}

pub struct Contexto {
    pub config: Config,
    pub repo: Mutex<Repositorio>,
    pub sefaz: Box<dyn Sefaz>,
    /// Chave que cifra os certificados guardados no banco.
    pub cofre: Cofre,
    /// Certificados já abertos, por produtor (evita decifrar e ler o .pfx a cada requisição).
    pub certificados: Mutex<HashMap<i64, Arc<Certificado>>>,
    pub tentativas: Tentativas,
}

impl Contexto {
    pub fn novo(config: Config, repo: Repositorio, sefaz: Box<dyn Sefaz>, cofre: Cofre) -> Self {
        Contexto {
            config,
            repo: Mutex::new(repo),
            sefaz,
            cofre,
            certificados: Mutex::new(HashMap::new()),
            tentativas: Tentativas::default(),
        }
    }

    pub fn repo(&self) -> MutexGuard<'_, Repositorio> {
        self.repo.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn cache_certificados(&self) -> MutexGuard<'_, HashMap<i64, Arc<Certificado>>> {
        self.certificados.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Produtor ativo na sessão: tudo o que é emitido usa o certificado dele e
/// os dados (notas, numeração, inutilizações, produtos) ficam separados por produtor.
#[derive(Clone)]
pub struct Ativo {
    pub id: i64,
    pub cert: Arc<Certificado>,
}

fn ler_dados(v: &Value) -> Result<DadosNFe> {
    serde_json::from_value(v.clone()).map_err(|e| anyhow::anyhow!("Dados da nota inválidos: {e}"))
}

pub struct NFePreparada {
    pub chave: String,
    pub v_nf: String,
    pub assinado: String,
    pub assinatura_valida: bool,
    pub xsd: ResultadoXsd,
}

/// Gera, assina, confere a assinatura e valida no XSD (não envia).
pub fn preparar_nfe(dados: &Value, ctx: &Contexto, p: &Ativo) -> Result<NFePreparada> {
    let g = gerar_xml_nfe(&ler_dados(dados)?, &ctx.config)?;
    let assinado = assinar_nfe(&g.xml, &p.cert)?;
    let assinatura_valida = verificar_assinatura(&assinado).valida;
    let xsd = validar_xsd(&assinado, Pacote::Nfe)?;
    Ok(NFePreparada { chave: g.chave, v_nf: g.v_nf, assinado, assinatura_valida, xsd })
}

pub fn validar_nfe(dados: &Value, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let p = preparar_nfe(dados, ctx, ativo)?;
    Ok(json!({ "chave": p.chave, "vNF": p.v_nf, "assinaturaValida": p.assinatura_valida, "xsd": p.xsd }))
}

fn inteiro(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) as i64,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

pub fn emitir_nfe(dados: &Value, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let serie = inteiro(dados.pointer("/ide/serie"));
    let n_nf = inteiro(dados.pointer("/ide/nNF"));
    {
        let repo = ctx.repo();
        if repo.numero_ja_usado(ativo.id, serie, n_nf)? {
            return Err(negocio(format!(
                "O número {n_nf} da série {serie} já foi usado. Próximo disponível: {}.",
                repo.proximo_numero(ativo.id, serie)?
            )));
        }
    }

    let p = preparar_nfe(dados, ctx, ativo)?;
    let mut resposta = json!({ "chave": p.chave, "vNF": p.v_nf, "assinaturaValida": p.assinatura_valida, "xsd": p.xsd });
    if !p.assinatura_valida || !p.xsd.valido {
        resposta["enviada"] = json!(false);
        return Ok(resposta);
    }

    // Grava antes de enviar: se a conexão cair, a nota fica "pendente" e pode ser consultada.
    let d = ler_dados(dados)?;
    let texto = |v: &Option<Value>| crate::fiscal::nfe::s(v);
    let id = ctx.repo().salvar_nota(&NovaNota {
        produtor_id: ativo.id,
        chave: p.chave.clone(),
        serie,
        n_nf,
        status: "pendente".into(),
        v_nf: p.v_nf.clone(),
        emit_doc: d.emit.as_ref().and_then(|e| e.documento()),
        dest_doc: d.dest.as_ref().and_then(|e| e.documento()),
        dest_nome: d.dest.as_ref().and_then(|e| texto(&e.xNome)),
        dados_json: dados.to_string(),
        xml_nfe: format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{}", p.assinado),
    })?;

    let r = ctx.sefaz.autorizar(&ctx.config, &ativo.cert, &p.assinado)?;
    let prot = r.protocolo.as_ref();
    let c_stat_prot = prot.and_then(|x| x.cStat.as_deref());
    let status = if prot.is_some_and(|x| x.autorizada) {
        "autorizada"
    } else if c_stat_prot.is_some_and(|c| DENEGADA.contains(&c)) {
        "denegada"
    } else if r.lote.cStat.as_deref() == Some("105") {
        "pendente"
    } else {
        "rejeitada"
    };
    ctx.repo().atualizar_resultado(
        id,
        &Resultado {
            status: status.into(),
            c_stat: prot.and_then(|x| x.cStat.clone()).or_else(|| r.lote.cStat.clone()),
            x_motivo: prot.and_then(|x| x.xMotivo.clone()).or_else(|| r.lote.xMotivo.clone()),
            n_prot: prot.and_then(|x| x.nProt.clone()),
            dh_recbto: prot.and_then(|x| x.dhRecbto.clone()).or_else(|| r.lote.dhRecbto.clone()),
            xml_proc: prot.filter(|x| x.autorizada).and_then(|x| x.xml.as_deref()).map(|x| montar_nfe_proc(&p.assinado, x)),
            xml_resposta: Some(r.resposta_soap.clone()),
        },
    )?;

    resposta["id"] = json!(id);
    resposta["status"] = json!(status);
    resposta["enviada"] = json!(true);
    resposta["lote"] = serde_json::to_value(&r.lote)?;
    resposta["recibo"] = json!(r.recibo);
    resposta["protocolo"] = serde_json::to_value(&r.protocolo)?;
    Ok(resposta)
}

fn nota_ou_erro(ctx: &Contexto, ativo: &Ativo, id: i64) -> Result<crate::db::Nota> {
    ctx.repo().nota(ativo.id, id)?.ok_or_else(|| negocio("Nota não encontrada."))
}

fn registrar_evento(tipo: TipoEvento, nota: &crate::db::Nota, mut dados: DadosEvento, texto: String, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let n_seq = if tipo == TipoEvento::Cce { ctx.repo().proximo_seq_cce(nota.id)? } else { 1 };
    if n_seq > 20 {
        return Err(negocio("Limite de 20 cartas de correção por nota atingido."));
    }
    dados.chave = nota.chave.clone();
    dados.documento = nota.emit_doc.clone().unwrap_or_default();
    dados.n_seq_evento = n_seq as u32;
    let p = preparar_evento(tipo, &dados, &ctx.config, &ativo.cert)?;
    let xsd = validar_xsd(&p.env_evento, tipo.pacote())?;
    if !xsd.valido {
        return Err(negocio(format!("Evento inválido no XSD: {}", xsd.erros.join("; "))));
    }

    let r = ctx.sefaz.evento(&ctx.config, &ativo.cert, &p.env_evento)?;
    let ev = r.evento.as_ref();
    let registrado = ev.is_some_and(|e| e.registrado);
    let c_stat = ev.and_then(|e| e.cStat.clone()).or_else(|| r.lote.cStat.clone());
    let x_motivo = ev.and_then(|e| e.xMotivo.clone()).or_else(|| r.lote.xMotivo.clone());
    let n_prot = ev.and_then(|e| e.nProt.clone());
    let evento_id = ctx.repo().salvar_evento(&NovoEvento {
        nota_id: nota.id,
        tipo: tipo.nome().into(),
        n_seq_evento: n_seq,
        texto,
        c_stat: c_stat.clone(),
        x_motivo: x_motivo.clone(),
        n_prot: n_prot.clone(),
        dh_reg_evento: ev.and_then(|e| e.dhRegEvento.clone()),
        registrado,
        xml_proc: r.xml_ret_evento.as_deref().filter(|_| registrado).map(|ret| montar_proc_evento(&p.evento, ret)),
        xml_resposta: Some(r.resposta_soap.clone()),
    })?;
    Ok(json!({
        "eventoId": evento_id, "registrado": registrado, "cStat": c_stat, "xMotivo": x_motivo,
        "nProt": n_prot, "nSeqEvento": n_seq,
    }))
}

pub fn cancelar_nota(id: i64, x_just: Option<String>, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let nota = nota_ou_erro(ctx, ativo, id)?;
    if nota.status != "autorizada" {
        return Err(negocio("Só é possível cancelar nota autorizada."));
    }
    let texto = x_just.clone().unwrap_or_default();
    let dados = DadosEvento { n_prot: nota.n_prot.clone(), x_just, ..Default::default() };
    let r = registrar_evento(TipoEvento::Cancelamento, &nota, dados, texto, ctx, ativo)?;
    if r["registrado"] == json!(true) {
        ctx.repo().marcar_cancelada(nota.id)?;
    }
    Ok(r)
}

pub fn carta_correcao(id: i64, x_correcao: Option<String>, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let nota = nota_ou_erro(ctx, ativo, id)?;
    if nota.status != "autorizada" {
        return Err(negocio("Só é possível emitir carta de correção para nota autorizada."));
    }
    let texto = x_correcao.clone().unwrap_or_default();
    registrar_evento(TipoEvento::Cce, &nota, DadosEvento { x_correcao, ..Default::default() }, texto, ctx, ativo)
}

pub fn inutilizar_faixa(dados: &Value, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let serie = inteiro(dados.get("serie"));
    let ini = inteiro(dados.get("nNFIni"));
    let fim = inteiro(dados.get("nNFFin"));
    let x_just = dados.get("xJust").and_then(Value::as_str).unwrap_or("").to_string();
    {
        let repo = ctx.repo();
        for n in ini..=fim.min(ini + 10_000) {
            if repo.numero_ja_usado(ativo.id, serie, n)? {
                return Err(negocio(format!("O número {n} já foi usado por uma nota e não pode ser inutilizado.")));
            }
        }
    }
    let d = DadosInutilizacao {
        cnpj: ativo.cert.info.documento.clone().unwrap_or_default(),
        serie,
        n_nf_ini: ini,
        n_nf_fin: fim,
        x_just: x_just.clone(),
        ano: None,
    };
    let inut = preparar_inutilizacao(&d, &ctx.config, &ativo.cert)?;
    let xsd = validar_xsd(&inut, Pacote::Inutilizacao)?;
    if !xsd.valido {
        return Err(negocio(format!("Inutilização inválida no XSD: {}", xsd.erros.join("; "))));
    }
    let r = ctx.sefaz.inutilizar(&ctx.config, &ativo.cert, &inut)?;
    ctx.repo().salvar_inutilizacao(&NovaInutilizacao {
        produtor_id: ativo.id,
        serie,
        n_nf_ini: ini,
        n_nf_fin: fim,
        x_just,
        c_stat: r.c_stat.clone(),
        x_motivo: r.x_motivo.clone(),
        n_prot: r.n_prot.clone(),
        homologada: r.homologada,
        xml_proc: r.xml_ret.as_deref().filter(|_| r.homologada).map(|x| montar_proc_inut(&inut, x)),
        xml_resposta: Some(r.resposta_soap.clone()),
    })?;
    Ok(json!({ "cStat": r.c_stat, "xMotivo": r.x_motivo, "nProt": r.n_prot, "homologada": r.homologada }))
}

pub fn consultar_nota(id: i64, ctx: &Contexto, ativo: &Ativo) -> Result<Value> {
    let nota = nota_ou_erro(ctx, ativo, id)?;
    Ok(serde_json::to_value(ctx.sefaz.consultar(&ctx.config, &ativo.cert, &nota.chave)?)?)
}
