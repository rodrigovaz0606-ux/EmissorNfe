//! Mensalidade dos produtores, paga por Pix através do GatewayRR.
//!
//! Desligada por padrão: só funciona com `API_PAGAMENTO=true` no .env. Cada produtor tem
//! um vencimento, escolhido no cadastro. O Pix da mensalidade pode ser gerado a partir de
//! 10 dias antes do vencimento. Com 5 dias de atraso, o produtor fica bloqueado: nada é
//! emitido com o certificado dele até a mensalidade ser paga (a tela de pagamento continua
//! acessível). Pagar avança o vencimento em um mês.
//!
//! Segurança:
//! - Os pedidos ao gateway são assinados com Ed25519 (chave em `PAGAMENTO_CHAVE_PRIVADA`,
//!   que nunca sai desta máquina), por HTTPS, sem seguir redirecionamentos.
//! - O pagamento só é dado como pago depois de o gateway confirmar, consultado por aqui;
//!   nenhuma notificação de fora muda a situação.
//! - Cada tentativa usa uma chave de idempotência guardada antes do envio: repetir depois
//!   de uma falha de rede não cria um segundo Pix.

use crate::contas::SessaoAtual;
use crate::db::Cobranca;
use crate::emissao::{Contexto, ErroNegocio};
use crate::fiscal::soap::ErroRede;
use anyhow::{Result, anyhow, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{Datelike, Days, Local, Months, NaiveDate};
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Dias antes do vencimento em que o aviso aparece.
pub const DIAS_AVISO: i64 = 5;
/// Dias de atraso que bloqueiam o produtor.
pub const DIAS_BLOQUEIO: i64 = 5;
/// Dias antes do vencimento a partir dos quais o Pix pode ser gerado.
pub const DIAS_ANTECEDENCIA_PIX: i64 = 10;
/// Pix que vence em menos que isso não é reaproveitado: gera-se outro.
const MARGEM_EXPIRACAO_SEGS: i64 = 120;
/// O Asaas (provedor do gateway) não aceita Pix abaixo de R$ 5,00.
const VALOR_MINIMO_CENTAVOS: i64 = 500;
const TAMANHO_MAXIMO_NOME: usize = 100;

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

fn indisponivel() -> anyhow::Error {
    anyhow::Error::new(ErroRede("Serviço de pagamento indisponível. Tente de novo em alguns minutos.".into()))
}

/// Resposta do gateway a um pagamento.
#[derive(Debug, Clone, PartialEq)]
pub struct PagamentoGateway {
    pub id: String,
    pub status: String,
    pub valor_centavos: i64,
    pub copia_cola: Option<String>,
    pub expira_em: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErroGateway {
    /// Rede, gateway ou provedor fora do ar: tentar de novo com a mesma chave.
    Indisponivel,
    /// Pedido recusado (código de erro do gateway): tentar de novo não adianta.
    Recusado(String),
}

/// O GatewayRR visto por este sistema (o real fala HTTP; os testes usam um falso).
pub trait Gateway: Send + Sync {
    fn criar_pix(&self, chave: &str, valor_centavos: i64, nome: &str, documento: &str) -> Result<PagamentoGateway, ErroGateway>;
    fn consultar(&self, id: &str) -> Result<PagamentoGateway, ErroGateway>;
}

/// Mensalidade ligada: valor e gateway.
pub struct Pagamento {
    pub valor_centavos: i64,
    pub gateway: Box<dyn Gateway>,
    /// Um Pix por vez: dois cliques ao mesmo tempo não criam duas cobranças.
    criando: Mutex<()>,
}

impl Pagamento {
    pub fn novo(valor_centavos: i64, gateway: Box<dyn Gateway>) -> Self {
        Pagamento { valor_centavos, gateway, criando: Mutex::new(()) }
    }
}

// ---------- configuração (.env) ----------

/// Lê a configuração. `None` = mensalidade desligada. Configuração incompleta ou inválida
/// impede o sistema de subir, em vez de funcionar sem cobrar ou cobrar errado.
pub fn carregar(env: impl Fn(&str) -> Option<String>) -> Result<Option<Pagamento>> {
    match env("API_PAGAMENTO").map(|v| v.trim().to_lowercase()).as_deref() {
        None | Some("false") => return Ok(None),
        Some("true") => {}
        Some(outro) => bail!("API_PAGAMENTO=\"{outro}\" inválido: use true ou false."),
    }
    let url = env("URL_API_PAGAMENTO").ok_or_else(|| anyhow!("API_PAGAMENTO=true exige URL_API_PAGAMENTO."))?;
    let url = validar_url(&url)?;
    let chave_id = env("PAGAMENTO_CHAVE_ID").ok_or_else(|| anyhow!("API_PAGAMENTO=true exige PAGAMENTO_CHAVE_ID."))?;
    let chave_id = validar_chave_id(&chave_id)?;
    let semente = env("PAGAMENTO_CHAVE_PRIVADA").ok_or_else(|| anyhow!("API_PAGAMENTO=true exige PAGAMENTO_CHAVE_PRIVADA."))?;
    let semente: [u8; 32] = URL_SAFE_NO_PAD
        .decode(semente.trim())
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| anyhow!("PAGAMENTO_CHAVE_PRIVADA inválida: gere com `emissor-nfe pagamento-chave`."))?;
    let valor = env("PAGAMENTO_VALOR_MENSAL").ok_or_else(|| anyhow!("API_PAGAMENTO=true exige PAGAMENTO_VALOR_MENSAL (ex.: 99,90)."))?;
    let valor_centavos = valor_em_centavos(&valor)
        .filter(|v| *v >= VALOR_MINIMO_CENTAVOS)
        .ok_or_else(|| anyhow!("PAGAMENTO_VALOR_MENSAL inválido: use reais com vírgula (ex.: 99,90), no mínimo 5,00."))?;
    let gateway = GatewayHttp::novo(url, chave_id, SigningKey::from_bytes(&semente))?;
    Ok(Some(Pagamento::novo(valor_centavos, Box::new(gateway))))
}

/// Só HTTPS (HTTP apenas para a própria máquina, em desenvolvimento) e só o endereço do
/// servidor, sem caminho: o caminho assinado precisa ser exatamente o que o gateway recebe.
pub fn validar_url(url: &str) -> Result<String> {
    let url = url.trim().trim_end_matches('/');
    let invalida = || anyhow!("URL_API_PAGAMENTO inválida: use só o endereço, como https://pagamentos.exemplo.com.br");
    let (esquema, resto) = url.split_once("://").ok_or_else(invalida)?;
    let host = resto.rsplit_once(':').map_or(resto, |(h, porta)| if !porta.is_empty() && porta.bytes().all(|b| b.is_ascii_digit()) { h } else { resto });
    let host_valido = !host.is_empty() && host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    if !host_valido || resto.contains(['/', '?', '#', '@']) {
        return Err(invalida());
    }
    match esquema {
        "https" => Ok(url.to_string()),
        "http" if matches!(host, "localhost" | "127.0.0.1") => Ok(url.to_string()),
        _ => bail!("URL_API_PAGAMENTO precisa usar https:// (http:// só para localhost)."),
    }
}

fn validar_chave_id(id: &str) -> Result<String> {
    let id = id.trim().to_lowercase();
    let grupos: Vec<&str> = id.split('-').collect();
    let formato = grupos.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12]) && grupos.iter().all(|g| g.bytes().all(|b| b.is_ascii_hexdigit()));
    if formato { Ok(id) } else { bail!("PAGAMENTO_CHAVE_ID inválido: use o UUID informado pelo operador do gateway.") }
}

/// "99,90", "99.90" ou "100" → centavos.
pub fn valor_em_centavos(texto: &str) -> Option<i64> {
    let texto = texto.trim().replace(',', ".");
    let (reais, centavos) = texto.split_once('.').unwrap_or((&texto, "0"));
    if reais.is_empty() || !reais.bytes().all(|b| b.is_ascii_digit()) || centavos.len() > 2 || !centavos.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let centavos: i64 = format!("{centavos:0<2}").parse().ok()?;
    reais.parse::<i64>().ok()?.checked_mul(100)?.checked_add(centavos)
}

/// Novo par de chaves para registrar no gateway: (chave pública, semente privada), em base64url.
pub fn gerar_chaves() -> (String, String) {
    let mut semente = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut semente);
    let chave = SigningKey::from_bytes(&semente);
    (URL_SAFE_NO_PAD.encode(chave.verifying_key().to_bytes()), URL_SAFE_NO_PAD.encode(semente))
}

// ---------- cliente HTTP do GatewayRR ----------

pub struct GatewayHttp {
    base: String,
    chave_id: String,
    chave: SigningKey,
    cliente: reqwest::blocking::Client,
}

impl GatewayHttp {
    pub fn novo(base: String, chave_id: String, chave: SigningKey) -> Result<Self> {
        let cliente = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent("EmissorNFe")
            .build()?;
        Ok(GatewayHttp { base, chave_id, chave, cliente })
    }

    fn chamar(&self, metodo: &str, caminho: &str, corpo: Vec<u8>) -> Result<Value, ErroGateway> {
        let agora = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| ErroGateway::Indisponivel)?.as_secs();
        let mut aleatorio = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut aleatorio);
        let nonce = URL_SAFE_NO_PAD.encode(aleatorio);
        let assinatura = assinar(&self.chave, metodo, caminho, &self.chave_id, agora, &nonce, &corpo);
        let pedido = match metodo {
            "POST" => self.cliente.post(format!("{}{caminho}", self.base)).header("Content-Type", "application/json").body(corpo),
            _ => self.cliente.get(format!("{}{caminho}", self.base)),
        };
        let resposta = pedido
            .header("X-Key-Id", &self.chave_id)
            .header("X-Timestamp", agora.to_string())
            .header("X-Nonce", nonce)
            .header("X-Signature", assinatura)
            .send()
            .map_err(|e| {
                eprintln!("[pagamento] gateway inacessível (tempo esgotado: {})", e.is_timeout());
                ErroGateway::Indisponivel
            })?;
        let status = resposta.status().as_u16();
        let corpo: Value = resposta.json().unwrap_or(Value::Null);
        let codigo = corpo["error"].as_str().unwrap_or("").to_string();
        match status {
            200..=299 => Ok(corpo),
            401 | 403 => {
                eprintln!("[pagamento] o gateway recusou a chave ({status}): confira PAGAMENTO_CHAVE_ID, PAGAMENTO_CHAVE_PRIVADA e o relógio");
                Err(ErroGateway::Indisponivel)
            }
            400 | 404 | 409 | 422 => Err(ErroGateway::Recusado(codigo)),
            _ => Err(ErroGateway::Indisponivel),
        }
    }
}

/// Assinatura do GatewayRR (docs/AUTHENTICATION.md): Ed25519 sobre sete linhas.
pub fn assinar(chave: &SigningKey, metodo: &str, caminho: &str, chave_id: &str, timestamp: u64, nonce: &str, corpo: &[u8]) -> String {
    let hash_corpo: String = Sha256::digest(corpo).iter().map(|b| format!("{b:02x}")).collect();
    let mensagem = ["GATEWAYRR-ED25519-V1", metodo, caminho, chave_id, &timestamp.to_string(), nonce, &hash_corpo].join("\n");
    URL_SAFE_NO_PAD.encode(chave.sign(mensagem.as_bytes()).to_bytes())
}

fn pagamento_de(v: &Value) -> Result<PagamentoGateway, ErroGateway> {
    let id = v["id"].as_str().filter(|id| id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'));
    let (Some(id), Some(status), Some(valor)) = (id, v["status"].as_str(), v["amount_minor"].as_i64()) else {
        eprintln!("[pagamento] resposta inesperada do gateway");
        return Err(ErroGateway::Indisponivel);
    };
    Ok(PagamentoGateway {
        id: id.to_string(),
        status: status.to_string(),
        valor_centavos: valor,
        copia_cola: v["pix"]["copy_paste_code"].as_str().map(String::from),
        expira_em: v["pix"]["expires_at"].as_i64(),
    })
}

impl Gateway for GatewayHttp {
    fn criar_pix(&self, chave: &str, valor_centavos: i64, nome: &str, documento: &str) -> Result<PagamentoGateway, ErroGateway> {
        let nome: String = nome.chars().filter(|c| !c.is_control()).take(TAMANHO_MAXIMO_NOME).collect();
        let corpo = json!({
            "idempotency_key": chave,
            "method": "pix",
            "amount_minor": valor_centavos,
            "currency": "BRL",
            "payer": { "name": nome.trim(), "tax_id": documento },
        });
        pagamento_de(&self.chamar("POST", "/v1/payments", corpo.to_string().into_bytes())?)
    }

    fn consultar(&self, id: &str) -> Result<PagamentoGateway, ErroGateway> {
        if !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(ErroGateway::Recusado("not_found".into()));
        }
        pagamento_de(&self.chamar("GET", &format!("/v1/payments/{id}"), Vec::new())?)
    }
}

// ---------- regras de vencimento ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    EmDia,
    VenceEmBreve,
    Atrasado,
    Bloqueado,
}

impl Situacao {
    pub fn nome(self) -> &'static str {
        match self {
            Situacao::EmDia => "em_dia",
            Situacao::VenceEmBreve => "vence_em_breve",
            Situacao::Atrasado => "atrasado",
            Situacao::Bloqueado => "bloqueado",
        }
    }
}

/// Situação pela diferença entre hoje e o vencimento (dias > 0: falta; < 0: atraso).
pub fn situacao(dias_ate_vencimento: i64) -> Situacao {
    match dias_ate_vencimento {
        d if d > DIAS_AVISO => Situacao::EmDia,
        d if d >= 0 => Situacao::VenceEmBreve,
        d if d > -DIAS_BLOQUEIO => Situacao::Atrasado,
        _ => Situacao::Bloqueado,
    }
}

pub fn hoje() -> NaiveDate {
    Local::now().date_naive()
}

fn data(iso: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(iso, "%Y-%m-%d").map_err(|_| anyhow!("Vencimento inválido no banco: {iso}"))
}

fn data_br(d: NaiveDate) -> String {
    d.format("%d/%m/%Y").to_string()
}

/// Primeiro vencimento de um produtor novo: a próxima data, depois de hoje, com esse dia do mês.
pub fn primeiro_vencimento(dia: u32, hoje: NaiveDate) -> Result<NaiveDate> {
    if !(1..=28).contains(&dia) {
        return Err(negocio("Escolha o dia do vencimento entre 1 e 28."));
    }
    let neste_mes = hoje.with_day0(dia - 1).ok_or_else(|| anyhow!("data inválida"))?;
    if neste_mes > hoje {
        Ok(neste_mes)
    } else {
        neste_mes.checked_add_months(Months::new(1)).ok_or_else(|| anyhow!("data inválida"))
    }
}


/// Vencimento informado pelo administrador (AAAA-MM-DD, dia 1 a 28).
pub fn validar_vencimento(texto: &str) -> Result<NaiveDate> {
    let d = NaiveDate::parse_from_str(texto.trim(), "%Y-%m-%d").map_err(|_| negocio("Data de vencimento inválida."))?;
    if d.day() > 28 {
        return Err(negocio("Use um dia de vencimento entre 1 e 28, que existe em todos os meses."));
    }
    Ok(d)
}

pub fn bloqueado(ctx: &Contexto, produtor_id: i64) -> Result<bool> {
    if ctx.pagamento.is_none() {
        return Ok(false);
    }
    let Some(venc) = ctx.repo().vencimento_produtor(produtor_id)? else { return Ok(false) };
    Ok(situacao((data(&venc)? - hoje()).num_days()) == Situacao::Bloqueado)
}

/// Resumo da mensalidade do produtor para a interface (`null` com a mensalidade desligada).
pub fn resumo(ctx: &Contexto, produtor_id: i64) -> Result<Value> {
    let Some(pag) = &ctx.pagamento else { return Ok(Value::Null) };
    let Some(venc) = ctx.repo().vencimento_produtor(produtor_id)? else {
        return Ok(json!({ "ativo": true, "situacao": "sem_vencimento", "valor_centavos": pag.valor_centavos }));
    };
    let venc = data(&venc)?;
    let dias = (venc - hoje()).num_days();
    Ok(json!({
        "ativo": true,
        "situacao": situacao(dias).nome(),
        "vencimento": venc.to_string(),
        "dias": dias,
        "bloqueio_em": (venc + Days::new(DIAS_BLOQUEIO as u64)).to_string(),
        "valor_centavos": pag.valor_centavos,
        "pode_pagar": dias <= DIAS_ANTECEDENCIA_PIX,
        "pix_a_partir_de": (venc - Days::new(DIAS_ANTECEDENCIA_PIX as u64)).to_string(),
    }))
}

/// Produtor da sessão (sem abrir o certificado, que um produtor bloqueado não usa).
pub fn produtor_da_sessao(ctx: &Contexto, s: &SessaoAtual) -> Result<i64> {
    let id = s.produtor_id.ok_or_else(|| negocio("Selecione um produtor para continuar."))?;
    let p = ctx.repo().produtor(id)?.filter(|p| p.ativo && (s.usuario.admin || p.dono_id == Some(s.usuario.id)));
    p.map(|p| p.id).ok_or_else(|| negocio("Produtor não encontrado."))
}

// ---------- Pix ----------

fn agora_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// QR code do Pix em SVG (gerado aqui, sem serviço de fora).
pub fn qr_svg(copia_cola: &str) -> Result<String> {
    let qr = qrcode::QrCode::with_error_correction_level(copia_cola.as_bytes(), qrcode::EcLevel::M)?;
    Ok(qr.render::<qrcode::render::svg::Color>().min_dimensions(240, 240).quiet_zone(true).build())
}

fn cobranca_json(c: &Cobranca) -> Result<Value> {
    let copia_cola = c.pix_copia_cola.clone().unwrap_or_default();
    Ok(json!({
        "vencimento": c.vencimento,
        "valor_centavos": c.valor_centavos,
        "situacao": c.situacao,
        "copia_cola": copia_cola,
        "qr_svg": qr_svg(&copia_cola)?,
        "expira_em": c.expira_em,
    }))
}

/// Aplica a resposta do gateway à cobrança. Devolve a situação final dela.
fn aplicar(ctx: &Contexto, c: &Cobranca, p: &PagamentoGateway) -> Result<&'static str> {
    match p.status.as_str() {
        "paid" if p.valor_centavos == c.valor_centavos => {
            let proximo = data(&c.vencimento)?.checked_add_months(Months::new(1)).ok_or_else(|| anyhow!("data inválida"))?;
            ctx.repo().confirmar_cobranca(c, &proximo.to_string())?;
            Ok("paga")
        }
        "paid" => {
            eprintln!("[pagamento] cobrança {} paga com valor diferente do esperado: conferir no gateway", c.id);
            Ok("pendente")
        }
        "expired" => {
            ctx.repo().encerrar_cobranca(c.id, "expirada")?;
            Ok("expirada")
        }
        "failed" => {
            ctx.repo().encerrar_cobranca(c.id, "falhou")?;
            Ok("falhou")
        }
        _ => Ok("pendente"),
    }
}

fn mensagem_recusa(codigo: &str) -> String {
    match codigo {
        "invalid_payer" | "provider_rejected" => {
            "O provedor de pagamento recusou o Pix. Confira se o CPF/CNPJ do certificado está correto e tente de novo.".into()
        }
        "amount_below_minimum" => "O valor da mensalidade está abaixo do mínimo aceito para Pix (R$ 5,00).".into(),
        _ => format!("O serviço de pagamento recusou o pedido ({codigo}). Avise o administrador."),
    }
}

/// Gera (ou reaproveita) o Pix da mensalidade atual do produtor.
pub fn gerar_pix(ctx: &Contexto, produtor_id: i64) -> Result<Value> {
    let pag = ctx.pagamento.as_ref().ok_or_else(|| negocio("A mensalidade não está ativada neste sistema."))?;
    let _um_por_vez = pag.criando.lock().unwrap_or_else(|e| e.into_inner());
    let (produtor, venc, instalacao) = {
        let repo = ctx.repo();
        let p = repo.produtor(produtor_id)?.ok_or_else(|| negocio("Produtor não encontrado."))?;
        let venc = repo
            .vencimento_produtor(produtor_id)?
            .ok_or_else(|| negocio("Este produtor ainda não tem vencimento. Peça ao administrador para definir."))?;
        (p, venc, repo.instalacao()?)
    };
    let dias = (data(&venc)? - hoje()).num_days();
    if dias > DIAS_ANTECEDENCIA_PIX {
        let a_partir = data(&venc)? - Days::new(DIAS_ANTECEDENCIA_PIX as u64);
        return Err(negocio(format!("O Pix desta mensalidade fica disponível a partir de {}.", data_br(a_partir))));
    }

    let aberta = ctx.repo().cobranca_aberta(produtor_id, &venc)?;
    let cobranca = match aberta {
        // Pix já gerado: confere se foi pago ou expirou antes de mostrar de novo.
        Some(c) if c.situacao == "pendente" => {
            let id = c.pagamento_id.clone().unwrap_or_default();
            let situacao = match pag.gateway.consultar(&id) {
                Ok(p) => aplicar(ctx, &c, &p)?,
                Err(_) => "pendente",
            };
            if situacao == "paga" {
                return Ok(json!({ "paga": true }));
            }
            if situacao == "pendente" && c.expira_em.unwrap_or(0) > agora_unix() + MARGEM_EXPIRACAO_SEGS {
                return cobranca_json(&c);
            }
            if situacao == "pendente" {
                ctx.repo().encerrar_cobranca(c.id, "expirada")?;
            }
            nova(ctx, pag, produtor_id, &venc, &instalacao)?
        }
        // Tentativa anterior sem resposta: repete com a mesma chave (não cria outro Pix).
        Some(c) => c,
        None => nova(ctx, pag, produtor_id, &venc, &instalacao)?,
    };

    match pag.gateway.criar_pix(&cobranca.chave, cobranca.valor_centavos, &produtor.titular, &produtor.documento) {
        Ok(p) => {
            if aplicar(ctx, &cobranca, &p)? == "paga" {
                return Ok(json!({ "paga": true }));
            }
            let (Some(copia_cola), Some(expira_em)) = (p.copia_cola.clone(), p.expira_em) else { return Err(indisponivel()) };
            ctx.repo().cobranca_pendente(cobranca.id, &p.id, &copia_cola, expira_em)?;
            cobranca_json(&Cobranca { situacao: "pendente".into(), pagamento_id: Some(p.id), pix_copia_cola: Some(copia_cola), expira_em: Some(expira_em), ..cobranca })
        }
        Err(ErroGateway::Indisponivel) => Err(indisponivel()),
        Err(ErroGateway::Recusado(codigo)) => {
            ctx.repo().encerrar_cobranca(cobranca.id, "falhou")?;
            Err(negocio(mensagem_recusa(&codigo)))
        }
    }
}

fn nova(ctx: &Contexto, pag: &Pagamento, produtor_id: i64, venc: &str, instalacao: &str) -> Result<Cobranca> {
    let repo = ctx.repo();
    let tentativa = repo.proxima_tentativa(produtor_id, venc)?;
    // 16 a 64 caracteres de A-Z, a-z, 0-9, - e _ (regra do gateway).
    let chave = format!("emissor-{instalacao}-{produtor_id}-{}-{tentativa}", venc.replace('-', ""));
    repo.nova_cobranca(produtor_id, venc, pag.valor_centavos, tentativa, &chave)
}

/// Confere no gateway o Pix em aberto do produtor (botão "Já paguei" e a tela aberta).
pub fn verificar(ctx: &Contexto, produtor_id: i64) -> Result<Value> {
    let pag = ctx.pagamento.as_ref().ok_or_else(|| negocio("A mensalidade não está ativada neste sistema."))?;
    let venc = ctx.repo().vencimento_produtor(produtor_id)?;
    let aberta = match &venc {
        Some(v) => ctx.repo().cobranca_aberta(produtor_id, v)?,
        None => None,
    };
    let mut pix = Value::Null;
    if let Some(c) = aberta.filter(|c| c.situacao == "pendente") {
        let id = c.pagamento_id.clone().unwrap_or_default();
        let situacao = match pag.gateway.consultar(&id) {
            Ok(p) => aplicar(ctx, &c, &p)?,
            Err(_) => "pendente",
        };
        if situacao == "pendente" && c.expira_em.unwrap_or(0) > agora_unix() {
            pix = cobranca_json(&c)?;
        }
    }
    Ok(json!({ "resumo": resumo(ctx, produtor_id)?, "pix": pix, "pagas": ctx.repo().cobrancas_do_produtor(produtor_id)? }))
}

/// Confere todos os Pix em aberto. Roda a cada minuto, para o pagamento ser reconhecido
/// mesmo com a tela fechada.
pub fn verificar_pendentes(ctx: &Contexto) -> Result<()> {
    let Some(pag) = &ctx.pagamento else { return Ok(()) };
    let pendentes = ctx.repo().cobrancas_pendentes(agora_unix() - 86_400)?;
    for c in pendentes {
        if let Ok(p) = pag.gateway.consultar(c.pagamento_id.as_deref().unwrap_or_default()) {
            aplicar(ctx, &c, &p)?;
        }
    }
    Ok(())
}

/// Administrador: define o próximo vencimento de um produtor (ex.: pagamento combinado por fora).
pub fn definir_vencimento(ctx: &Contexto, produtor_id: i64, texto: &str) -> Result<Value> {
    if ctx.pagamento.is_none() {
        return Err(negocio("A mensalidade não está ativada neste sistema."));
    }
    let d = validar_vencimento(texto)?;
    let repo = ctx.repo();
    repo.produtor(produtor_id)?.filter(|p| p.ativo).ok_or_else(|| negocio("Produtor não encontrado."))?;
    repo.definir_vencimento(produtor_id, &d.to_string())?;
    Ok(json!({ "id": produtor_id, "vencimento": d.to_string() }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn assinatura_igual_ao_vetor_de_teste_do_gateway() {
        let chave = SigningKey::from_bytes(&[7u8; 32]);
        let assinatura = assinar(
            &chave,
            "POST",
            "/v1/payments?expand=history",
            "01920000-0000-4000-8000-000000000001",
            1_800_000_000,
            "0123456789abcdef",
            br#"{"amount":1000}"#,
        );
        let bytes = URL_SAFE_NO_PAD.decode(assinatura).unwrap();
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "fc4a18ffaf6aad664bef30da812e894d7884328889e7ed83c55a153f9901b772515d1068c2d3f1d71b6c3654d00a14bba66e51011e450e2c47e542535de0ad07"
        );
    }

    #[test]
    fn situacao_por_dias() {
        assert_eq!(situacao(6), Situacao::EmDia);
        assert_eq!(situacao(5), Situacao::VenceEmBreve);
        assert_eq!(situacao(0), Situacao::VenceEmBreve);
        assert_eq!(situacao(-1), Situacao::Atrasado);
        assert_eq!(situacao(-4), Situacao::Atrasado);
        assert_eq!(situacao(-5), Situacao::Bloqueado);
        assert_eq!(situacao(-40), Situacao::Bloqueado);
    }

    #[test]
    fn primeiro_vencimento_e_sempre_depois_de_hoje() {
        let d = |s| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(primeiro_vencimento(10, d("2026-11-05")).unwrap(), d("2026-11-10"));
        assert_eq!(primeiro_vencimento(10, d("2026-11-10")).unwrap(), d("2026-12-10"));
        assert_eq!(primeiro_vencimento(5, d("2026-12-20")).unwrap(), d("2027-01-05"));
        assert!(primeiro_vencimento(0, d("2026-11-05")).is_err());
        assert!(primeiro_vencimento(29, d("2026-11-05")).is_err());
        assert!(validar_vencimento("2026-12-28").is_ok());
        assert!(validar_vencimento("2026-12-29").is_err());
        assert!(validar_vencimento("10/12/2026").is_err());
    }

    #[test]
    fn valores_e_configuracao() {
        assert_eq!(valor_em_centavos("99,90"), Some(9990));
        assert_eq!(valor_em_centavos("99.9"), Some(9990));
        assert_eq!(valor_em_centavos("100"), Some(10000));
        assert_eq!(valor_em_centavos(" 5,00 "), Some(500));
        for invalido in ["", "-5", "1,234", "abc", "1.2.3", ",50"] {
            assert_eq!(valor_em_centavos(invalido), None, "{invalido}");
        }
        assert_eq!(validar_url("https://pag.exemplo.com.br/").unwrap(), "https://pag.exemplo.com.br");
        assert!(validar_url("https://pag.exemplo.com.br:8443").is_ok());
        assert!(validar_url("http://localhost:8080").is_ok());
        assert!(validar_url("http://127.0.0.1:8080").is_ok());
        for invalida in ["http://pag.exemplo.com.br", "https://pag.exemplo.com.br/api", "https://user@pag.exemplo.com", "ftp://x", "pag.exemplo.com", "https://", "https://a.com?x=1"] {
            assert!(validar_url(invalida).is_err(), "{invalida}");
        }
    }

    #[test]
    fn desligada_por_padrao_e_incompleta_nao_sobe() {
        let env = |pares: &'static [(&'static str, &'static str)]| move |k: &str| pares.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string());
        assert!(carregar(env(&[])).unwrap().is_none());
        assert!(carregar(env(&[("API_PAGAMENTO", "false")])).unwrap().is_none());
        assert!(carregar(env(&[("API_PAGAMENTO", "sim")])).is_err());
        assert!(carregar(env(&[("API_PAGAMENTO", "true")])).is_err());
        let (_, semente) = gerar_chaves();
        let semente: &'static str = Box::leak(semente.into_boxed_str());
        let completo: &'static [(&'static str, &'static str)] = Box::leak(Box::new([
            ("API_PAGAMENTO", "true"),
            ("URL_API_PAGAMENTO", "https://pag.exemplo.com.br"),
            ("PAGAMENTO_CHAVE_ID", "01920000-0000-4000-8000-000000000001"),
            ("PAGAMENTO_CHAVE_PRIVADA", semente),
            ("PAGAMENTO_VALOR_MENSAL", "99,90"),
        ]));
        let pag = carregar(env(completo)).unwrap().unwrap();
        assert_eq!(pag.valor_centavos, 9990);
        let abaixo: &'static [(&'static str, &'static str)] = Box::leak(Box::new([
            ("API_PAGAMENTO", "true"),
            ("URL_API_PAGAMENTO", "https://pag.exemplo.com.br"),
            ("PAGAMENTO_CHAVE_ID", "01920000-0000-4000-8000-000000000001"),
            ("PAGAMENTO_CHAVE_PRIVADA", semente),
            ("PAGAMENTO_VALOR_MENSAL", "4,99"),
        ]));
        assert!(carregar(env(abaixo)).is_err());
    }

    #[test]
    fn qr_code_em_svg() {
        let svg = qr_svg("00020101021226820014br.gov.bcb.pix2560exemplo6304ABCD").unwrap();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
        assert!(!svg.contains("<script"));
    }
}
