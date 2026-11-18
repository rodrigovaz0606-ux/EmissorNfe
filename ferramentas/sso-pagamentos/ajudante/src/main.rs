//! Ajudante da instalação do SaintSSO + GatewayRR no servidor Windows.
//!
//! Faz, sem Node nem psql, o que os scripts do SaintSSO e o guia do GatewayRR pedem. Segredos
//! entram SEMPRE por variável de ambiente (nunca pela linha de comando, que aparece na lista de
//! processos) e saem só para arquivos, nunca na tela.
//!
//!   ajudante-sso segredo                     32 bytes aleatórios em base64url
//!   ajudante-sso chave-ed25519               "<pública> <semente>" (base64url sem padding)
//!   ajudante-sso sql                         executa $SQL_COMANDO no banco $SQL_URL
//!   ajudante-sso webhook                     registra o webhook do SaintSSO no GatewayRR
//!                                            ($GATEWAYRR_URL, $GATEWAYRR_KEY_ID,
//!                                             $GATEWAYRR_PRIVATE_KEY_SEED, $SAINTSSO_ISSUER)
//!   ajudante-sso clientes <entrada.json> <saida.json>
//!                                            cadastra os clientes OIDC ($SAINTSSO_URL,
//!                                            $SAINTSSO_ADMIN_EMAIL, $SAINTSSO_ADMIN_PASSWORD) e
//!                                            grava os client_secret novos em <saida.json>
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64U;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

fn env(nome: &str) -> Result<String> {
    std::env::var(nome).ok().filter(|v| !v.trim().is_empty()).with_context(|| format!("defina {nome}"))
}

fn aleatorio<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("sem fonte aleatória: {e}"))?;
    Ok(b)
}

/// Mensagem assinada pelo GatewayRR (docs/AUTHENTICATION.md), igual a scripts/registrar-webhook.mjs.
pub fn mensagem_gatewayrr(metodo: &str, caminho: &str, key_id: &str, hora: &str, nonce: &str, corpo: &[u8]) -> String {
    let resumo: String = Sha256::digest(corpo).iter().map(|b| format!("{b:02x}")).collect();
    ["GATEWAYRR-ED25519-V1", metodo, caminho, key_id, hora, nonce, &resumo].join("\n")
}

/// Chamada ao GatewayRR assinada com a chave do SaintSSO ($GATEWAYRR_URL, $GATEWAYRR_KEY_ID,
/// $GATEWAYRR_PRIVATE_KEY_SEED). Devolve o status e o corpo da resposta.
fn chamada_assinada(metodo: &str, caminho: &str, corpo: &str) -> Result<(u16, String)> {
    let url = env("GATEWAYRR_URL")?;
    let key_id = env("GATEWAYRR_KEY_ID")?;
    let semente: [u8; 32] = B64U.decode(env("GATEWAYRR_PRIVATE_KEY_SEED")?.trim())?.try_into().map_err(|_| anyhow::anyhow!("semente precisa ter 32 bytes"))?;
    let chave = SigningKey::from_bytes(&semente);
    let hora = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs().to_string();
    let nonce: String = aleatorio::<16>()?.iter().map(|b| format!("{b:02x}")).collect();
    let assinatura = B64U.encode(chave.sign(mensagem_gatewayrr(metodo, caminho, &key_id, &hora, &nonce, corpo.as_bytes()).as_bytes()).to_bytes());
    let metodo_http = reqwest::Method::from_bytes(metodo.as_bytes())?;
    let mut req = reqwest::blocking::Client::new()
        .request(metodo_http, format!("{}{caminho}", url.trim_end_matches('/')))
        .header("x-key-id", &key_id)
        .header("x-timestamp", &hora)
        .header("x-nonce", &nonce)
        .header("x-signature", assinatura);
    if !corpo.is_empty() {
        req = req.header("content-type", "application/json");
    }
    let r = req.body(corpo.to_string()).send()?;
    let status = r.status().as_u16();
    Ok((status, r.text().unwrap_or_default()))
}

fn webhook() -> Result<()> {
    let issuer = env("SAINTSSO_ISSUER")?;
    let corpo = json!({ "url": format!("{}/webhooks/gatewayrr", issuer.trim_end_matches('/')) }).to_string();
    let (status, texto) = chamada_assinada("PUT", "/v1/events/endpoint", &corpo)?;
    println!("{status} {texto}");
    if !(200..300).contains(&status) {
        bail!("o GatewayRR recusou o webhook");
    }
    Ok(())
}

fn clientes(entrada: &str, saida: &str) -> Result<()> {
    let base = env("SAINTSSO_URL")?.trim_end_matches('/').to_string();
    // Aceita o BOM que o PowerShell 5.1 grava no início de arquivos UTF-8.
    let texto = std::fs::read_to_string(entrada)?;
    let lista: Vec<Value> = serde_json::from_str(texto.trim_start_matches('\u{feff}'))?;
    let http = reqwest::blocking::Client::builder().cookie_store(true).build()?;
    let login = http
        .post(format!("{base}/api/session"))
        .header("x-requested-with", "saintsso")
        .json(&json!({ "email": env("SAINTSSO_ADMIN_EMAIL")?, "password": env("SAINTSSO_ADMIN_PASSWORD")?, "remember": false }))
        .send()?;
    if !login.status().is_success() {
        bail!("login do administrador falhou ({}). A conta já foi criada em {base}/register?", login.status().as_u16());
    }
    let mut segredos = serde_json::Map::new();
    let mut falhas = 0;
    for c in &lista {
        let id = c["client_id"].as_str().unwrap_or("?").to_string();
        let r = http.post(format!("{base}/api/admin/clients")).header("x-requested-with", "saintsso").json(c).send()?;
        let status = r.status().as_u16();
        let corpo: Value = r.json().unwrap_or(Value::Null);
        match status {
            201 => {
                println!("cadastrado: {id}");
                if let Some(s) = corpo["client_secret"].as_str() {
                    segredos.insert(id, json!(s));
                }
            }
            // Já existia: gera um segredo novo (o antigo deixa de valer), que o instalador grava
            // no sistema logo em seguida. Assim dá para rodar de novo quantas vezes for preciso.
            409 => {
                let r = http.post(format!("{base}/api/admin/clients/{id}/secret")).header("x-requested-with", "saintsso").send()?;
                let st = r.status().as_u16();
                let corpo: Value = r.json().unwrap_or(Value::Null);
                match corpo["client_secret"].as_str() {
                    Some(s) if st == 200 => {
                        println!("já existia, segredo novo gerado: {id}");
                        segredos.insert(id, json!(s));
                    }
                    _ => {
                        falhas += 1;
                        eprintln!("falhou ao gerar segredo novo: {id}: {st}");
                    }
                }
            }
            _ => {
                falhas += 1;
                eprintln!("falhou: {id}: {status} {}", corpo["message"].as_str().or(corpo["error"].as_str()).unwrap_or(""));
            }
        }
    }
    let _ = http.delete(format!("{base}/api/session")).header("x-requested-with", "saintsso").send();
    std::fs::write(saida, Value::Object(segredos).to_string())?;
    if falhas > 0 {
        bail!("{falhas} cliente(s) não cadastrado(s)");
    }
    Ok(())
}

async fn sql() -> Result<()> {
    use sqlx::Connection;
    let mut c = sqlx::PgConnection::connect(&env("SQL_URL")?).await.context("não conectou no banco")?;
    sqlx::raw_sql(sqlx::AssertSqlSafe(env("SQL_COMANDO")?)).execute(&mut c).await.context("o comando SQL falhou")?;
    c.close().await?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["segredo"] => println!("{}", B64U.encode(aleatorio::<32>()?)),
        ["chave-ed25519"] => {
            let semente = aleatorio::<32>()?;
            let publica = SigningKey::from_bytes(&semente).verifying_key().to_bytes();
            println!("{} {}", B64U.encode(publica), B64U.encode(semente));
        }
        ["sql"] => sql().await?,
        ["webhook"] => tokio::task::spawn_blocking(webhook).await??,
        // Chamada assinada avulsa (ex.: pagar no simulador: POST /v1/simulator/payments/{id}/pay).
        ["assinado", metodo, caminho] => {
            let (m, c) = (metodo.to_string(), caminho.to_string());
            let (status, texto) = tokio::task::spawn_blocking(move || chamada_assinada(&m, &c, "")).await??;
            println!("{status} {texto}");
            if !(200..300).contains(&status) {
                bail!("HTTP {status}");
            }
        }
        ["clientes", entrada, saida] => {
            let (e, s) = (entrada.to_string(), saida.to_string());
            tokio::task::spawn_blocking(move || clientes(&e, &s)).await??
        }
        _ => bail!("uso: ajudante-sso segredo | chave-ed25519 | sql | webhook | assinado <METODO> <caminho> | clientes <entrada.json> <saida.json>"),
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mensagem_igual_ao_script_do_saintsso() {
        let m = mensagem_gatewayrr("PUT", "/v1/events/endpoint", "kid", "1800000000", "abc", b"{}");
        assert_eq!(m, "GATEWAYRR-ED25519-V1\nPUT\n/v1/events/endpoint\nkid\n1800000000\nabc\n44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a");
        // Assinatura e chave pública calculadas pelo Node (mesmo código do registrar-webhook.mjs),
        // com a semente de 32 bytes 0x07.
        let chave = SigningKey::from_bytes(&[7u8; 32]);
        assert_eq!(B64U.encode(chave.verifying_key().to_bytes()), "6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw");
        assert_eq!(
            B64U.encode(chave.sign(m.as_bytes()).to_bytes()),
            "bh9vpvXXrkVxT96UMfniHr5wsS2AhQ7DVyEOcEOBM-AxK3EqDd-Tb6gaR4-Pikd_y5FMxM_LIXtx8GsYY4PHBg"
        );
    }
}
