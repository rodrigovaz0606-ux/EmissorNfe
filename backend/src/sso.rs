//! Login único pelo SaintSSO (OpenID Connect, Authorization Code + PKCE).
//!
//! O servidor guarda o `state`/PKCE em memória, troca o código no endpoint de token e confere
//! emissor, público, nonce e validade do `id_token`. A sessão local (cookie) continua a mesma.

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64U;
use rand::RngCore;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct Pendente {
    verificador: String,
    nonce: String,
    redirect_uri: String,
    expira: Instant,
}

static PENDENTES: OnceLock<Mutex<HashMap<String, Pendente>>> = OnceLock::new();

fn pendentes() -> &'static Mutex<HashMap<String, Pendente>> {
    PENDENTES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Identidade confirmada pelo SSO.
pub struct Identidade {
    pub sub: String,
    pub nome: String,
    pub email: String,
}

fn env(nome: &str, padrao: &str) -> String {
    std::env::var(nome).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| padrao.to_string())
}

/// Desligado por padrão: só liga com `SSO_ENABLED=true` (e o SaintSSO no ar em `SSO_ISSUER`).
/// Ligado sem SaintSSO, a tela de login mostraria um botão que só dá erro.
pub fn ativo() -> bool {
    std::env::var("SSO_ENABLED").is_ok_and(|v| v.trim() == "true")
}

fn emissor() -> String {
    env("SSO_ISSUER", "http://localhost:8080").trim_end_matches('/').to_string()
}

fn client_id() -> String {
    env("SSO_CLIENT_ID", "emissor-nfe-web")
}

fn aleatorio(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    B64U.encode(b)
}

async fn descoberta() -> Result<Value> {
    let url = format!("{}/.well-known/openid-configuration", emissor());
    let r = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|_| anyhow!("O SaintSSO está indisponível."))?;
    let doc: Value = r.error_for_status().map_err(|_| anyhow!("O SaintSSO está indisponível."))?.json().await?;
    if doc["issuer"].as_str() != Some(emissor().as_str()) {
        bail!("Emissor do SaintSSO inesperado.");
    }
    Ok(doc)
}

/// Monta o endereço de autorização e guarda o PKCE para o retorno.
pub async fn iniciar(redirect_uri: &str) -> Result<String> {
    let doc = descoberta().await?;
    let base = doc["authorization_endpoint"].as_str().ok_or_else(|| anyhow!("SaintSSO sem endpoint de autorização."))?;
    let (state, nonce, verificador) = (aleatorio(24), aleatorio(24), aleatorio(48));
    let desafio = B64U.encode(Sha256::digest(verificador.as_bytes()));
    {
        let mut mapa = pendentes().lock().unwrap();
        let agora = Instant::now();
        mapa.retain(|_, p| p.expira > agora);
        mapa.insert(
            state.clone(),
            Pendente { verificador, nonce: nonce.clone(), redirect_uri: redirect_uri.to_string(), expira: agora + Duration::from_secs(600) },
        );
    }
    let mut url = reqwest::Url::parse(base)?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id())
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", "openid profile email plan")
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &desafio)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

/// Troca o código por tokens e devolve a identidade validada.
pub async fn concluir(code: &str, state: &str) -> Result<Identidade> {
    let fluxo = pendentes().lock().unwrap().remove(state).filter(|p| p.expira > Instant::now());
    let Some(fluxo) = fluxo else { bail!("O pedido de login expirou. Tente entrar novamente.") };
    let doc = descoberta().await?;
    let endpoint = doc["token_endpoint"].as_str().ok_or_else(|| anyhow!("SaintSSO sem endpoint de token."))?;
    let cid = client_id();
    let mut req = reqwest::Client::new().post(endpoint).timeout(Duration::from_secs(10)).form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", fluxo.redirect_uri.as_str()),
        ("client_id", cid.as_str()),
        ("code_verifier", fluxo.verificador.as_str()),
    ]);
    if let Ok(segredo) = std::env::var("SSO_CLIENT_SECRET")
        && !segredo.is_empty()
    {
        req = req.basic_auth(&cid, Some(segredo));
    }
    let resp = req.send().await.map_err(|_| anyhow!("O SaintSSO está indisponível."))?;
    if !resp.status().is_success() {
        bail!("O SaintSSO recusou o login.");
    }
    let tokens: Value = resp.json().await?;
    let id_token = tokens["id_token"].as_str().ok_or_else(|| anyhow!("O SaintSSO recusou o login."))?;
    let carga = id_token.split('.').nth(1).ok_or_else(|| anyhow!("Resposta do SaintSSO inválida."))?;
    let claims: Value = serde_json::from_slice(&B64U.decode(carga)?)?;
    // O id_token veio direto do endpoint de token por TLS; mesmo assim conferimos o conteúdo.
    let publico_ok = match &claims["aud"] {
        Value::String(a) => *a == cid,
        Value::Array(v) => v.iter().any(|a| a.as_str() == Some(cid.as_str())),
        _ => false,
    };
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    if claims["iss"].as_str() != Some(emissor().as_str())
        || !publico_ok
        || claims["nonce"].as_str() != Some(fluxo.nonce.as_str())
        || claims["exp"].as_i64().unwrap_or(0) <= agora
    {
        bail!("Resposta do SaintSSO inválida.");
    }
    let sub = claims["sub"].as_str().filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("Resposta do SaintSSO sem identificação."))?;
    let email = claims["email"].as_str().unwrap_or("").trim().to_lowercase();
    let nome = claims["name"].as_str().filter(|n| !n.trim().is_empty()).map(str::to_string).unwrap_or_else(|| email.clone());
    Ok(Identidade { sub: sub.to_string(), nome, email })
}
