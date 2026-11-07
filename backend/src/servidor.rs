//! API HTTP (axum) e arquivos da interface (pasta dist/).
//!
//! Só escuta em 127.0.0.1: esta API assina e envia notas com os certificados guardados.
//! Erros de rede/SEFAZ → 502; regras de negócio e dados inválidos → 400;
//! sem login → 401; sem permissão → 403; sem produtor selecionado → 409.
//!
//! Acesso:
//! - `/api/auth/*`: estado, configuração inicial, entrar, sair, trocar senha.
//! - `/api/produtores/*`: usuário logado (cada um vê os seus; o administrador vê todos).
//! - `/api/admin/*`: só administrador.
//! - demais rotas: usuário logado com um produtor selecionado (usam o certificado dele).

use crate::auth::{self, DURACAO_SESSAO, NOME_COOKIE};
use crate::clientes;
use crate::cofre::Cofre;
use crate::contas::{self, SessaoAtual};
use crate::danfe::gerar_danfe;
use crate::db::{FiltroNotas, Repositorio};
use crate::emissao::{self, Ativo, Contexto, ErroNegocio, SefazReal};
use crate::estoque;
use crate::fiscal::config::carregar_config;
use crate::fiscal::soap::ErroRede;
use crate::produtos;
use crate::relatorios;
use crate::simulada::SefazSimulada;
use anyhow::{Result, bail};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

type Ctx = Arc<Contexto>;

pub struct ErroApi(StatusCode, String);

impl IntoResponse for ErroApi {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({ "erro": self.1 }))).into_response()
    }
}

fn erro(e: anyhow::Error) -> ErroApi {
    if e.downcast_ref::<ErroRede>().is_some() {
        return ErroApi(StatusCode::BAD_GATEWAY, e.to_string());
    }
    if e.downcast_ref::<ErroNegocio>().is_none() {
        eprintln!("[erro] {e:#}");
    }
    ErroApi(StatusCode::BAD_REQUEST, e.to_string())
}

fn nao_encontrado(msg: &str) -> ErroApi {
    ErroApi(StatusCode::NOT_FOUND, msg.into())
}

fn nao_autenticado() -> ErroApi {
    ErroApi(StatusCode::UNAUTHORIZED, "Sessão encerrada. Entre novamente.".into())
}

/// Executa trabalho bloqueante (SEFAZ, banco, assinatura, PDF, Argon2) fora do runtime assíncrono.
async fn bloqueante<T: Send + 'static>(ctx: &Ctx, f: impl FnOnce(&Contexto) -> Result<T> + Send + 'static) -> Result<T, ErroApi> {
    let ctx = ctx.clone();
    tokio::task::spawn_blocking(move || f(&ctx))
        .await
        .map_err(|e| ErroApi(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(erro)
}

fn json_ok(v: Value) -> Response {
    axum::Json(v).into_response()
}

fn corpo_json(b: &Bytes) -> Result<Value, ErroApi> {
    if b.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(b).map_err(|e| ErroApi(StatusCode::BAD_REQUEST, format!("JSON inválido: {e}")))
}

fn campo_texto(v: &Value, nome: &str) -> Option<String> {
    v.get(nome).and_then(Value::as_str).map(String::from)
}

fn arquivo(conteudo: Vec<u8>, tipo: &'static str, disposicao: String) -> Response {
    let mut r = Response::new(Body::from(conteudo));
    r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(tipo));
    if let Ok(v) = HeaderValue::from_str(&disposicao) {
        r.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    r
}

fn xml(conteudo: Option<String>, nome: &str) -> Response {
    match conteudo {
        Some(x) => arquivo(x.into_bytes(), "application/xml; charset=utf-8", format!("attachment; filename=\"{nome}\"")),
        None => nao_encontrado("XML não disponível").into_response(),
    }
}

fn pdf(conteudo: Vec<u8>, nome: &str) -> Response {
    arquivo(conteudo, "application/pdf", format!("inline; filename=\"{nome}\""))
}

fn id_de(s: &str) -> i64 {
    s.parse().unwrap_or(-1)
}

// ---------- sessão (cookie) ----------

fn token_do_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|par| par.trim().split_once('='))
        .find(|(nome, valor)| *nome == NOME_COOKIE && valor.len() == 64 && valor.bytes().all(|b| b.is_ascii_hexdigit()))
        .map(|(_, valor)| valor.to_string())
}

/// Cookie de sessão: HttpOnly (JavaScript não lê) e SameSite=Strict (outros sites não o enviam).
fn com_cookie(mut r: Response, token: Option<&str>) -> Response {
    let valor = match token {
        Some(t) => format!("{NOME_COOKIE}={t}; HttpOnly; SameSite=Strict; Path=/; Max-Age={DURACAO_SESSAO}"),
        None => format!("{NOME_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"),
    };
    if let Ok(v) = HeaderValue::from_str(&valor) {
        r.headers_mut().append(header::SET_COOKIE, v);
    }
    r
}

async fn sessao_de(parts: &Parts, ctx: &Ctx) -> Result<Option<SessaoAtual>, ErroApi> {
    let Some(token) = token_do_cookie(&parts.headers) else { return Ok(None) };
    bloqueante(ctx, move |c| contas::sessao_por_token(c, &token)).await
}

/// Usuário logado.
pub struct Logado(SessaoAtual);

impl FromRequestParts<Ctx> for Logado {
    type Rejection = ErroApi;
    async fn from_request_parts(parts: &mut Parts, ctx: &Ctx) -> Result<Self, ErroApi> {
        sessao_de(parts, ctx).await?.map(Logado).ok_or_else(nao_autenticado)
    }
}

/// Administrador logado.
pub struct Admin(SessaoAtual);

impl FromRequestParts<Ctx> for Admin {
    type Rejection = ErroApi;
    async fn from_request_parts(parts: &mut Parts, ctx: &Ctx) -> Result<Self, ErroApi> {
        let Logado(s) = Logado::from_request_parts(parts, ctx).await?;
        if !s.usuario.admin {
            return Err(ErroApi(StatusCode::FORBIDDEN, "Acesso restrito ao administrador.".into()));
        }
        Ok(Admin(s))
    }
}

/// Usuário logado com um produtor selecionado (e o certificado dele aberto).
pub struct ComProdutor(Ativo);

impl FromRequestParts<Ctx> for ComProdutor {
    type Rejection = ErroApi;
    async fn from_request_parts(parts: &mut Parts, ctx: &Ctx) -> Result<Self, ErroApi> {
        let Logado(s) = Logado::from_request_parts(parts, ctx).await?;
        match bloqueante(ctx, move |c| contas::ativo_da_sessao(c, &s)).await? {
            Some(a) => Ok(ComProdutor(a)),
            None => Err(ErroApi(StatusCode::CONFLICT, "Selecione um produtor para continuar.".into())),
        }
    }
}

/// Origem aceita para requisições que alteram dados: a própria API ou uma página servida
/// pela própria máquina (ex.: o Vite em localhost:5175 no `npm run dev`, cujo proxy troca o Host).
/// Sites de fora (que poderiam tentar usar a sessão do navegador) são recusados.
pub fn origem_permitida(origem: &str, host: &str) -> bool {
    let Some(resto) = origem.strip_prefix("http://").or_else(|| origem.strip_prefix("https://")) else { return false };
    if resto == host {
        return true;
    }
    let nome = match resto.strip_prefix('[') {
        Some(ipv6) => ipv6.split(']').next().unwrap_or(""),
        None => resto.rsplit_once(':').map_or(resto, |(n, porta)| if porta.bytes().all(|b| b.is_ascii_digit()) { n } else { resto }),
    };
    matches!(nome, "localhost" | "127.0.0.1" | "::1")
}

/// Requisições que alteram dados precisam vir da própria interface.
async fn mesma_origem(req: Request, next: Next) -> Response {
    let altera = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if altera && let Some(origem) = req.headers().get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("");
        if !origem_permitida(origem, host) {
            return ErroApi(StatusCode::FORBIDDEN, "Origem não permitida.".into()).into_response();
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod testes_origem {
    use super::origem_permitida;

    #[test]
    fn so_a_propria_maquina() {
        let host = "127.0.0.1:3335";
        assert!(origem_permitida("http://127.0.0.1:3335", host));
        assert!(origem_permitida("http://localhost:5175", host)); // Vite em npm run dev
        assert!(origem_permitida("http://127.0.0.1:5175", host));
        assert!(origem_permitida("http://[::1]:5175", host));
        assert!(!origem_permitida("http://site-malicioso.com", host));
        assert!(!origem_permitida("http://localhost.site-malicioso.com", host));
        assert!(!origem_permitida("http://127.0.0.1.nip.io:5175", host));
        assert!(!origem_permitida("null", host));
        assert!(!origem_permitida("file://", host));
    }
}

async fn sem_cache(mut r: Response) -> Response {
    r.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

// ---------- login ----------

fn estado_json(c: &Contexto, s: Option<&SessaoAtual>) -> Result<Value> {
    let configurado = c.repo().contar_usuarios()? > 0;
    let Some(s) = s else { return Ok(json!({ "configurado": configurado, "usuario": null, "produtor": null })) };
    let (produtor, aviso) = match contas::ativo_da_sessao(c, s) {
        Ok(Some(a)) => (contas::produtor_json(c, a.id)?, Value::Null),
        Ok(None) => (Value::Null, Value::Null),
        Err(e) => (Value::Null, json!(e.to_string())),
    };
    Ok(json!({
        "configurado": configurado, "usuario": contas::usuario_json(&s.usuario), "produtor": produtor, "aviso": aviso,
        "simulada": c.sefaz.simulada(), "uf": c.config.uf,
    }))
}

async fn estado(State(ctx): State<Ctx>, headers: HeaderMap) -> Result<Response, ErroApi> {
    let s = match token_do_cookie(&headers) {
        Some(t) => bloqueante(&ctx, move |c| contas::sessao_por_token(c, &t)).await?,
        None => None,
    };
    Ok(json_ok(bloqueante(&ctx, move |c| estado_json(c, s.as_ref())).await?))
}

/// Depois de entrar: devolve o estado da nova sessão com o cookie.
async fn responder_sessao(ctx: &Ctx, token: String) -> Result<Response, ErroApi> {
    let t = token.clone();
    let v = bloqueante(ctx, move |c| {
        let s = contas::sessao_por_token(c, &t)?;
        estado_json(c, s.as_ref())
    })
    .await?;
    Ok(com_cookie(json_ok(v), Some(&token)))
}

async fn configurar(State(ctx): State<Ctx>, corpo: Bytes) -> Result<Response, ErroApi> {
    let d = corpo_json(&corpo)?;
    let token = bloqueante(&ctx, move |c| {
        let t = |n: &str| campo_texto(&d, n).unwrap_or_default();
        contas::configurar_primeiro_admin(c, &t("login"), &t("nome"), &t("senha"))
    })
    .await?;
    responder_sessao(&ctx, token).await
}

async fn entrar(State(ctx): State<Ctx>, corpo: Bytes) -> Result<Response, ErroApi> {
    let d = corpo_json(&corpo)?;
    let token = bloqueante(&ctx, move |c| {
        contas::entrar(c, &campo_texto(&d, "login").unwrap_or_default(), &campo_texto(&d, "senha").unwrap_or_default())
    })
    .await
    .map_err(|e| if e.0 == StatusCode::BAD_REQUEST { ErroApi(StatusCode::UNAUTHORIZED, e.1) } else { e })?;
    responder_sessao(&ctx, token).await
}

async fn sair(State(ctx): State<Ctx>, Logado(s): Logado) -> Result<Response, ErroApi> {
    bloqueante(&ctx, move |c| contas::sair(c, &s)).await?;
    Ok(com_cookie(StatusCode::NO_CONTENT.into_response(), None))
}

async fn trocar_senha(State(ctx): State<Ctx>, Logado(s): Logado, corpo: Bytes) -> Result<Response, ErroApi> {
    let d = corpo_json(&corpo)?;
    bloqueante(&ctx, move |c| {
        contas::trocar_senha(c, &s, &campo_texto(&d, "atual").unwrap_or_default(), &campo_texto(&d, "nova").unwrap_or_default())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

// ---------- produtores ----------

async fn produtores(State(ctx): State<Ctx>, Logado(s): Logado) -> Result<Response, ErroApi> {
    Ok(json_ok(Value::Array(bloqueante(&ctx, move |c| contas::listar_produtores(c, &s.usuario)).await?)))
}

async fn adicionar_produtor(State(ctx): State<Ctx>, Logado(s): Logado, corpo: Bytes) -> Result<Response, ErroApi> {
    let d = corpo_json(&corpo)?;
    let p = bloqueante(&ctx, move |c| {
        let p = contas::adicionar_produtor(c, &s, &d)?;
        // O primeiro produtor (ou o recém-adicionado) já fica selecionado.
        if let Some(id) = p["id"].as_i64() {
            contas::selecionar_produtor(c, &s, Some(id))?;
        }
        Ok(p)
    })
    .await?;
    Ok((StatusCode::CREATED, axum::Json(p)).into_response())
}

async fn renovar_certificado(State(ctx): State<Ctx>, Logado(s): Logado, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, d) = (id_de(&id), corpo_json(&corpo)?);
    Ok(json_ok(bloqueante(&ctx, move |c| contas::renovar_certificado(c, &s, id, &d)).await?))
}

async fn definir_emitente(State(ctx): State<Ctx>, Logado(s): Logado, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, d) = (id_de(&id), corpo_json(&corpo)?);
    Ok(json_ok(bloqueante(&ctx, move |c| contas::definir_emitente(c, &s, id, &d)).await?))
}

async fn remover_produtor(State(ctx): State<Ctx>, Logado(s): Logado, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    bloqueante(&ctx, move |c| contas::remover_produtor(c, &s, id)).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn selecionar_produtor(State(ctx): State<Ctx>, Logado(s): Logado, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    let v = bloqueante(&ctx, move |c| {
        contas::selecionar_produtor(c, &s, Some(id))?;
        contas::produtor_json(c, id)
    })
    .await?;
    Ok(json_ok(v))
}

// ---------- administração ----------

async fn usuarios(State(ctx): State<Ctx>, Admin(_): Admin) -> Result<Response, ErroApi> {
    Ok(json_ok(Value::Array(bloqueante(&ctx, |c| c.repo().listar_usuarios()).await?)))
}

async fn criar_usuario(State(ctx): State<Ctx>, Admin(_): Admin, corpo: Bytes) -> Result<Response, ErroApi> {
    let d = corpo_json(&corpo)?;
    let u = bloqueante(&ctx, move |c| contas::criar_usuario(c, &d)).await?;
    Ok((StatusCode::CREATED, axum::Json(u)).into_response())
}

async fn alterar_usuario(State(ctx): State<Ctx>, Admin(s): Admin, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, d) = (id_de(&id), corpo_json(&corpo)?);
    Ok(json_ok(bloqueante(&ctx, move |c| contas::alterar_usuario(c, &s, id, &d)).await?))
}

// ---------- emissão (produtor selecionado) ----------

async fn emissor(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    let id = a.id;
    let produtor = bloqueante(&ctx, move |c| contas::produtor_json(c, id)).await?;
    let info = &a.cert.info;
    Ok(json_ok(json!({
        "uf": ctx.config.uf, "cUF": ctx.config.c_uf, "ambiente": "homologacao", "simulada": ctx.sefaz.simulada(),
        "certificado": info, "podeInutilizar": info.documento.as_ref().is_some_and(|d| d.len() == 14),
        "produtor": produtor,
    })))
}

async fn status(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    let r = bloqueante(&ctx, move |c| c.sefaz.status(&c.config, &a.cert)).await?;
    Ok(json_ok(serde_json::to_value(r).unwrap_or_default()))
}

async fn numeracao(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(serie): Path<String>) -> Result<Response, ErroApi> {
    let serie: i64 = serie.parse().map_err(|_| ErroApi(StatusCode::BAD_REQUEST, "Série inválida".into()))?;
    let proximo = bloqueante(&ctx, move |c| c.repo().proximo_numero(a.id, serie)).await?;
    Ok(json_ok(json!({ "serie": serie, "proximo": proximo })))
}

async fn validar(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::validar_nfe(&dados, c, &a)).await?))
}

async fn danfe_previa(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    let (chave, conteudo) = bloqueante(&ctx, move |c| {
        let p = emissao::preparar_nfe(&dados, c, &a)?;
        Ok((p.chave, gerar_danfe(&p.assinado, false)?))
    })
    .await?;
    Ok(pdf(conteudo, &format!("previa-{chave}.pdf")))
}

async fn emitir(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::emitir_nfe(&dados, c, &a)).await?))
}

#[derive(Deserialize)]
struct ConsultaNotas {
    pagina: Option<String>,
    de: Option<String>,
    ate: Option<String>,
    status: Option<String>,
    busca: Option<String>,
}

async fn notas(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Query(q): Query<ConsultaNotas>) -> Result<Response, ErroApi> {
    let pagina = q.pagina.and_then(|p| p.parse::<i64>().ok()).unwrap_or(1).max(1);
    let filtro = FiltroNotas { de: q.de, ate: q.ate, status: q.status, busca: q.busca };
    Ok(json_ok(bloqueante(&ctx, move |c| c.repo().listar_notas(a.id, &filtro, pagina, 50)).await?))
}

#[derive(Deserialize)]
struct Periodo {
    de: Option<String>,
    ate: Option<String>,
}

async fn exportar_notas(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Query(q): Query<Periodo>) -> Result<Response, ErroApi> {
    let (de, ate) = (q.de.unwrap_or_default(), q.ate.unwrap_or_default());
    let nome = format!("notas-{de}-a-{ate}.zip");
    let zip = bloqueante(&ctx, move |c| relatorios::exportar_zip(c, &a, &de, &ate)).await?;
    Ok(arquivo(zip, "application/zip", format!("attachment; filename=\"{nome}\"")))
}

async fn painel(State(ctx): State<Ctx>, Logado(s): Logado, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    Ok(json_ok(bloqueante(&ctx, move |c| relatorios::painel(c, &s, &a)).await?))
}

async fn nota(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    match bloqueante(&ctx, move |c| c.repo().nota_json(a.id, id)).await? {
        Some(v) => Ok(json_ok(v)),
        None => Err(nao_encontrado("Nota não encontrada")),
    }
}

#[derive(Deserialize)]
struct TipoXml {
    tipo: Option<String>,
}

async fn nota_xml(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>, Query(q): Query<TipoXml>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    let Some(n) = bloqueante(&ctx, move |c| c.repo().nota(a.id, id)).await? else { return Err(nao_encontrado("Nota não encontrada")) };
    let tipo = q.tipo.unwrap_or_else(|| if n.xml_proc.is_some() { "proc".into() } else { "nfe".into() });
    let conteudo = match tipo.as_str() {
        "proc" => n.xml_proc,
        "nfe" => Some(n.xml_nfe),
        "resposta" => n.xml_resposta,
        _ => None,
    };
    let sufixo = if tipo == "proc" { "procNFe" } else { &tipo };
    Ok(xml(conteudo, &format!("{}-{sufixo}.xml", n.chave)))
}

async fn nota_danfe(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    let Some(n) = bloqueante(&ctx, move |c| c.repo().nota(a.id, id)).await? else { return Err(nao_encontrado("Nota não encontrada")) };
    let chave = n.chave.clone();
    let conteudo = bloqueante(&ctx, move |_| gerar_danfe(n.xml_proc.as_deref().unwrap_or(&n.xml_nfe), n.status == "cancelada")).await?;
    Ok(pdf(conteudo, &format!("DANFE-{chave}.pdf")))
}

async fn cancelar(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, x_just) = (id_de(&id), campo_texto(&corpo_json(&corpo)?, "xJust"));
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::cancelar_nota(id, x_just, c, &a)).await?))
}

async fn cce(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, texto) = (id_de(&id), campo_texto(&corpo_json(&corpo)?, "xCorrecao"));
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::carta_correcao(id, texto, c, &a)).await?))
}

async fn consultar(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::consultar_nota(id, c, &a)).await?))
}

async fn evento_xml(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    match bloqueante(&ctx, move |c| c.repo().evento(a.id, id)).await? {
        Some((tipo, seq, proc, resposta)) => Ok(xml(proc.or(resposta), &format!("evento-{tipo}-{seq}.xml"))),
        None => Err(nao_encontrado("Evento não encontrado")),
    }
}

async fn inutilizacoes(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    Ok(json_ok(Value::Array(bloqueante(&ctx, move |c| c.repo().listar_inutilizacoes(a.id)).await?)))
}

async fn inutilizar(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    Ok(json_ok(bloqueante(&ctx, move |c| emissao::inutilizar_faixa(&dados, c, &a)).await?))
}

async fn listar_produtos(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    Ok(json_ok(Value::Array(bloqueante(&ctx, move |c| c.repo().listar_produtos(a.id)).await?)))
}

async fn criar_produto(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    let p = bloqueante(&ctx, move |c| produtos::salvar_produto(None, &dados, c, &a)).await?;
    Ok((StatusCode::CREATED, axum::Json(p)).into_response())
}

async fn alterar_produto(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, dados) = (id_de(&id), corpo_json(&corpo)?);
    Ok(json_ok(bloqueante(&ctx, move |c| produtos::salvar_produto(Some(id), &dados, c, &a)).await?))
}

async fn excluir_produto(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    if bloqueante(&ctx, move |c| c.repo().excluir_produto(a.id, id)).await? {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(nao_encontrado("Produto não encontrado"))
    }
}

async fn estoque_do_produto(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    Ok(json_ok(bloqueante(&ctx, move |c| estoque::consultar(c, &a, id)).await?))
}

async fn movimentar_estoque(
    State(ctx): State<Ctx>,
    Logado(s): Logado,
    ComProdutor(a): ComProdutor,
    Path(id): Path<String>,
    corpo: Bytes,
) -> Result<Response, ErroApi> {
    let (id, dados) = (id_de(&id), corpo_json(&corpo)?);
    let v = bloqueante(&ctx, move |c| estoque::movimentar_manual(c, &a, s.usuario.id, id, &dados)).await?;
    Ok((StatusCode::CREATED, axum::Json(v)).into_response())
}

async fn listar_clientes(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor) -> Result<Response, ErroApi> {
    Ok(json_ok(Value::Array(bloqueante(&ctx, move |c| c.repo().listar_clientes(a.id)).await?)))
}

async fn criar_cliente(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, corpo: Bytes) -> Result<Response, ErroApi> {
    let dados = corpo_json(&corpo)?;
    let p = bloqueante(&ctx, move |c| clientes::salvar_cliente(None, &dados, c, &a)).await?;
    Ok((StatusCode::CREATED, axum::Json(p)).into_response())
}

async fn alterar_cliente(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>, corpo: Bytes) -> Result<Response, ErroApi> {
    let (id, dados) = (id_de(&id), corpo_json(&corpo)?);
    Ok(json_ok(bloqueante(&ctx, move |c| clientes::salvar_cliente(Some(id), &dados, c, &a)).await?))
}

async fn excluir_cliente(State(ctx): State<Ctx>, ComProdutor(a): ComProdutor, Path(id): Path<String>) -> Result<Response, ErroApi> {
    let id = id_de(&id);
    if bloqueante(&ctx, move |c| c.repo().excluir_cliente(a.id, id)).await? {
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(nao_encontrado("Cliente não encontrado"))
    }
}

/// Monta a aplicação. `dist`: pasta da interface compilada (opcional).
pub fn criar_app(ctx: Ctx, dist: Option<PathBuf>) -> Router {
    let api = Router::new()
        // login
        .route("/auth/estado", get(estado))
        .route("/auth/configurar", post(configurar))
        .route("/auth/entrar", post(entrar))
        .route("/auth/sair", post(sair))
        .route("/auth/senha", post(trocar_senha))
        // produtores
        .route("/produtores", get(produtores).post(adicionar_produtor))
        .route("/produtores/{id}", axum::routing::delete(remover_produtor))
        .route("/produtores/{id}/certificado", put(renovar_certificado))
        .route("/produtores/{id}/emitente", put(definir_emitente))
        .route("/produtores/{id}/selecionar", post(selecionar_produtor))
        // administração
        .route("/admin/usuarios", get(usuarios).post(criar_usuario))
        .route("/admin/usuarios/{id}", put(alterar_usuario))
        // emissão
        .route("/emissor", get(emissor))
        .route("/sefaz/status", get(status))
        .route("/numeracao/{serie}", get(numeracao))
        .route("/nfe/validar", post(validar))
        .route("/nfe/danfe-previa", post(danfe_previa))
        .route("/nfe/emitir", post(emitir))
        .route("/painel", get(painel))
        .route("/notas", get(notas))
        .route("/notas/exportar", get(exportar_notas))
        .route("/notas/{id}", get(nota))
        .route("/notas/{id}/xml", get(nota_xml))
        .route("/notas/{id}/danfe", get(nota_danfe))
        .route("/notas/{id}/cancelar", post(cancelar))
        .route("/notas/{id}/cce", post(cce))
        .route("/notas/{id}/consultar", get(consultar))
        .route("/eventos/{id}/xml", get(evento_xml))
        .route("/inutilizacoes", get(inutilizacoes).post(inutilizar))
        .route("/produtos", get(listar_produtos).post(criar_produto))
        .route("/produtos/{id}", put(alterar_produto).delete(excluir_produto))
        .route("/produtos/{id}/estoque", get(estoque_do_produto).post(movimentar_estoque))
        .route("/clientes", get(listar_clientes).post(criar_cliente))
        .route("/clientes/{id}", put(alterar_cliente).delete(excluir_cliente))
        .layer(axum::middleware::map_response(sem_cache))
        .layer(axum::middleware::from_fn(mesma_origem))
        .with_state(ctx);
    let app = Router::new().nest("/api", api);
    match dist.filter(|d| d.join("index.html").exists()) {
        Some(d) => app.fallback_service(ServeDir::new(&d).fallback(ServeFile::new(d.join("index.html")))),
        None => app,
    }
}

fn env(nome: &str) -> Option<String> {
    std::env::var(nome).ok().filter(|v| !v.is_empty())
}

/// Sobe o servidor com a configuração do ambiente (.env).
/// Os certificados agora vêm do cadastro de produtores (não do CERT_PFX_PATH).
pub fn iniciar() -> Result<bool> {
    let config = carregar_config(env)?; // falha se não for homologação
    let simulada = env("NFE_SEFAZ_SIMULADA").as_deref() == Some("1");
    let banco = env("NFE_DB");
    if simulada && banco.is_none() {
        bail!("Modo SEFAZ simulada exige um banco separado: defina NFE_DB (ex.: data/simulado.sqlite).");
    }
    let raiz = crate::raiz_do_projeto();
    let arquivo = banco.clone().unwrap_or_else(|| raiz.join("data").join("emissor.sqlite").to_string_lossy().into_owned());
    let repo = Repositorio::abrir(&arquivo)?;
    let pasta_banco = std::path::Path::new(&arquivo).parent().map(PathBuf::from).unwrap_or_default();
    let cofre = Cofre::carregar(env("NFE_CHAVE_MESTRA"), &pasta_banco.join("chave-mestra.key"))?;
    let porta: u16 = env("PORT").and_then(|p| p.parse().ok()).unwrap_or(3335);
    let sem_usuarios = repo.contar_usuarios()? == 0;

    println!("Emissor NF-e (HOMOLOGAÇÃO, {}) em http://localhost:{porta}", config.uf);
    if simulada {
        println!("*** SEFAZ SIMULADA — nada é enviado. Banco: {arquivo} ***");
    }
    if sem_usuarios {
        println!("Primeiro acesso: abra o endereço acima para criar o usuário administrador.");
    }
    let sefaz: Box<dyn emissao::Sefaz> = if simulada { Box::new(SefazSimulada) } else { Box::new(SefazReal) };
    let ctx = Arc::new(Contexto::novo(config, repo, sefaz, cofre));
    let app = criar_app(ctx, Some(raiz.join("dist")));
    let _ = auth::hash_ficticio(); // prepara já (evita a primeira tentativa de login mais lenta)

    tokio::runtime::Runtime::new()?.block_on(async move {
        let ouvinte = tokio::net::TcpListener::bind(("127.0.0.1", porta)).await?;
        axum::serve(ouvinte, app)
            .with_graceful_shutdown(async {
                let _ = tokio::signal::ctrl_c().await;
            })
            .await?;
        anyhow::Ok(())
    })?;
    Ok(true)
}
