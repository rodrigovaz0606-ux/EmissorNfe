//! Integração com o Campo Certo (pacote Pro do SaintSSO): cada nota autorizada (XML autorizado)
//! e cada cancelamento de produtor com CPF vão para o Campo Certo, que os importa na empresa da
//! mesma conta do login único.
//!
//! - Fila no banco (`integracao_saida`): a nota entra na fila na hora da autorização ou do
//!   cancelamento e é enviada em segundo plano. Se o Campo Certo estiver fora do ar, tenta de
//!   novo mais tarde (1 min, 2 min, 4 min... até 6 h). Nada se perde.
//! - Só dentro do servidor: o endereço tem de ser `http://127.0.0.1` ou `http://localhost`.
//! - Assinado: HMAC-SHA256 do corpo com `INTEGRACAO_EMISSOR_SEGREDO` (o mesmo nos dois sistemas).
//!
//! Configuração (.env): `CAMPO_CERTO_URL=http://127.0.0.1:80` e
//! `INTEGRACAO_EMISSOR_SEGREDO=<32+ caracteres>`. Sem `CAMPO_CERTO_URL`, fica desligada (a fila
//! continua sendo preenchida e é enviada quando ligar).

use crate::db::ItemIntegracao;
use crate::emissao::Contexto;
use anyhow::{Result, bail};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

/// Quantos envios por rodada do verificador.
const LOTE: i64 = 20;
/// Espera máxima entre tentativas.
const ESPERA_MAXIMA: i64 = 6 * 60 * 60;

/// Quem leva o pedido ao Campo Certo (o real fala HTTP; os testes usam um falso).
pub trait Envio: Send + Sync {
    /// Devolve o status HTTP e o corpo da resposta.
    fn enviar(&self, url: &str, corpo: &[u8], hora: i64, assinatura: &str) -> Result<(u16, String)>;
}

pub struct Integracao {
    pub url: String,
    segredo: String,
    envio: Box<dyn Envio>,
}

impl Integracao {
    pub fn nova(url: &str, segredo: &str, envio: Box<dyn Envio>) -> Result<Self> {
        let base = url.trim().trim_end_matches('/');
        let local = ["http://127.0.0.1", "http://localhost"]
            .iter()
            .any(|p| base == *p || base.strip_prefix(p).is_some_and(|resto| resto.starts_with(':')));
        if !local {
            bail!("CAMPO_CERTO_URL precisa ser http://127.0.0.1:<porta> ou http://localhost:<porta> (a integração é só dentro do servidor).");
        }
        if segredo.len() < 32 {
            bail!("INTEGRACAO_EMISSOR_SEGREDO precisa ter pelo menos 32 caracteres (o mesmo no Campo Certo).");
        }
        Ok(Integracao { url: format!("{base}/api/integracao/emissor"), segredo: segredo.to_string(), envio })
    }
}

/// Lê a configuração do .env. `None` = desligada.
pub fn carregar(env: impl Fn(&str) -> Option<String>) -> Result<Option<Integracao>> {
    let Some(url) = env("CAMPO_CERTO_URL") else { return Ok(None) };
    let segredo = env("INTEGRACAO_EMISSOR_SEGREDO").unwrap_or_default();
    Ok(Some(Integracao::nova(&url, &segredo, Box::new(EnvioHttp))?))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// HMAC-SHA256(segredo, "{hora}\n{sha256(corpo) em hex}"), em hex. Igual ao `assinarPedido` do Campo Certo.
pub fn assinar(segredo: &str, hora: i64, corpo: &[u8]) -> String {
    let resumo = hex(&Sha256::digest(corpo));
    let mut mac = Hmac::<Sha256>::new_from_slice(segredo.as_bytes()).expect("HMAC aceita chave de qualquer tamanho");
    mac.update(format!("{hora}\n{resumo}").as_bytes());
    hex(&mac.finalize().into_bytes())
}

struct EnvioHttp;

impl Envio for EnvioHttp {
    fn enviar(&self, url: &str, corpo: &[u8], hora: i64, assinatura: &str) -> Result<(u16, String)> {
        let cliente = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let r = cliente
            .post(url)
            .header("content-type", "application/json")
            .header("x-integracao-hora", hora.to_string())
            .header("x-integracao-assinatura", assinatura)
            .body(corpo.to_vec())
            .send()?;
        let status = r.status().as_u16();
        Ok((status, r.text().unwrap_or_default()))
    }
}

/// Coloca a nota (ou o cancelamento) na fila. Erro aqui nunca atrapalha a emissão.
pub fn enfileirar(ctx: &Contexto, nota_id: i64, tipo: &str) {
    if let Err(e) = ctx.repo().enfileirar_integracao(nota_id, tipo) {
        eprintln!("[campo certo] não foi possível pôr a nota {nota_id} na fila: {e:#}");
    }
}

fn pedido(item: &ItemIntegracao) -> Value {
    let mut p = json!({
        "tipo": item.tipo,
        "conta": item.conta,
        "chave": item.chave,
        "produtor": { "documento": item.documento, "nome": item.titular },
    });
    if item.tipo == "nota" {
        p["xml"] = json!(item.xml_proc);
    }
    p
}

fn agora() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn espera(tentativas: i64) -> i64 {
    (60_i64 << tentativas.clamp(0, 20)).min(ESPERA_MAXIMA)
}

fn mensagem(corpo: &str) -> String {
    serde_json::from_str::<Value>(corpo)
        .ok()
        .and_then(|v| v.get("resultado").or_else(|| v.get("error")).and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| corpo.chars().take(200).collect())
}

/// Uma rodada: envia o que está na vez. Devolve quantos foram entregues.
/// O banco nunca fica preso durante o envio (só ao ler a fila e ao gravar o resultado).
pub fn processar(ctx: &Contexto) -> Result<usize> {
    let Some(integ) = &ctx.integracao else { return Ok(0) };
    let itens = ctx.repo().integracao_pendentes(agora(), LOTE)?;
    let mut entregues = 0;
    for item in itens {
        if item.tipo == "nota" && item.xml_proc.is_none() {
            ctx.repo().integracao_concluida(item.id, "ignorada", "nota sem XML autorizado")?;
            continue;
        }
        let corpo = serde_json::to_vec(&pedido(&item))?;
        let hora = agora();
        let assinatura = assinar(&integ.segredo, hora, &corpo);
        let resposta = integ.envio.enviar(&integ.url, &corpo, hora, &assinatura);
        let repo = ctx.repo();
        match resposta {
            Ok((s, texto)) if (200..300).contains(&s) => {
                repo.integracao_concluida(item.id, "enviada", &mensagem(&texto))?;
                entregues += 1;
            }
            // Recusa definitiva (ex.: XML inválido para o Campo Certo): não adianta repetir.
            Ok((422, texto)) => repo.integracao_concluida(item.id, "ignorada", &mensagem(&texto))?,
            // Conta que ainda não entrou no Campo Certo, Campo Certo fora do ar, assinatura...
            Ok((s, texto)) => repo.integracao_reagendar(item.id, agora() + espera(item.tentativas), &format!("HTTP {s}: {}", mensagem(&texto)))?,
            Err(e) => repo.integracao_reagendar(item.id, agora() + espera(item.tentativas), &format!("sem resposta: {e}"))?,
        }
    }
    Ok(entregues)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn so_aceita_endereco_local_e_segredo_longo() {
        let s = "x".repeat(32);
        assert!(Integracao::nova("http://127.0.0.1:80", &s, Box::new(EnvioHttp)).is_ok());
        assert!(Integracao::nova("http://localhost:3333/", &s, Box::new(EnvioHttp)).is_ok());
        assert!(Integracao::nova("https://campocerto.com.br", &s, Box::new(EnvioHttp)).is_err());
        assert!(Integracao::nova("http://127.0.0.1.nip.io", &s, Box::new(EnvioHttp)).is_err());
        assert!(Integracao::nova("http://127.0.0.1:80", "curto", Box::new(EnvioHttp)).is_err());
    }

    #[test]
    fn assinatura_igual_a_do_campo_certo() {
        // Mesmo cálculo do assinarPedido (Node): HMAC-SHA256 de "hora\nsha256(corpo)".
        // Valor calculado pelo próprio server/integracao.js do Campo Certo com as mesmas entradas.
        let a = assinar("segredo-de-teste-da-integracao-0123456789", 1_800_000_000, b"{\"tipo\":\"nota\"}");
        assert_eq!(a, "117074e939985e01e83b9b86f0f103cc06df38d9889c41d0e5c0d020eb965ad9");
        assert_ne!(a, assinar("segredo-de-teste-da-integracao-0123456789", 1_800_000_001, b"{\"tipo\":\"nota\"}"));
    }

    #[test]
    fn espera_cresce_ate_seis_horas() {
        assert_eq!(espera(0), 60);
        assert_eq!(espera(3), 480);
        assert_eq!(espera(30), ESPERA_MAXIMA);
    }
}
