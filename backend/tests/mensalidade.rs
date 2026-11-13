//! Integração da mensalidade: API + banco em memória + GatewayRR falso (nada sai da máquina).

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::Days;
use emissor_nfe::cofre::Cofre;
use emissor_nfe::db::Repositorio;
use emissor_nfe::emissao::Contexto;
use emissor_nfe::fiscal::config::Config;
use emissor_nfe::pagamento::{ErroGateway, Gateway, PagamentoGateway, Pagamento, hoje, verificar_pendentes};
use emissor_nfe::servidor::criar_app;
use emissor_nfe::simulada::SefazSimulada;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

const PFX_CNPJ: &[u8] = include_bytes!("fixtures/teste.pfx"); // EMPRESA TESTE, 12345678000195 (fictício)

/// Pedido de criação recebido pelo gateway falso: (chave, valor, nome, documento).
type Pedido = (String, i64, String, String);

#[derive(Default)]
struct Estado {
    /// Pagamentos por chave de idempotência, como o gateway real.
    por_chave: HashMap<String, PagamentoGateway>,
    pedidos: Vec<Pedido>,
    fora_do_ar: bool,
}

#[derive(Clone, Default)]
struct GatewayFalso(Arc<Mutex<Estado>>);

impl GatewayFalso {
    fn pagar_tudo(&self) {
        for p in self.0.lock().unwrap().por_chave.values_mut() {
            p.status = "paid".into();
        }
    }
}

impl Gateway for GatewayFalso {
    fn criar_pix(&self, chave: &str, valor: i64, nome: &str, documento: &str) -> Result<PagamentoGateway, ErroGateway> {
        let mut e = self.0.lock().unwrap();
        e.pedidos.push((chave.into(), valor, nome.into(), documento.into()));
        if e.fora_do_ar {
            return Err(ErroGateway::Indisponivel);
        }
        let n = e.por_chave.len();
        let p = e.por_chave.entry(chave.into()).or_insert_with(|| PagamentoGateway {
            id: format!("00000000-0000-4000-8000-{n:012}"),
            status: "pending".into(),
            valor_centavos: valor,
            copia_cola: Some(format!("00020101021226PIX{n}6304ABCD")),
            expira_em: Some(chrono::Utc::now().timestamp() + 3600),
        });
        Ok(p.clone())
    }

    fn consultar(&self, id: &str) -> Result<PagamentoGateway, ErroGateway> {
        let e = self.0.lock().unwrap();
        if e.fora_do_ar {
            return Err(ErroGateway::Indisponivel);
        }
        e.por_chave.values().find(|p| p.id == id).cloned().ok_or(ErroGateway::Recusado("not_found".into()))
    }
}

struct Api {
    app: Router,
    ctx: Arc<Contexto>,
    cookie: Mutex<Option<String>>,
}

impl Api {
    fn nova(gateway: Option<GatewayFalso>) -> Self {
        let mut ctx = Contexto::novo(Config::teste(), Repositorio::abrir(":memory:").unwrap(), Box::new(SefazSimulada), Cofre::de_teste());
        ctx.pagamento = gateway.map(|g| Pagamento::novo(9990, Box::new(g)));
        let ctx = Arc::new(ctx);
        Api { app: criar_app(ctx.clone(), None), ctx, cookie: Mutex::new(None) }
    }

    async fn chamar(&self, metodo: &str, url: &str, corpo: Option<Value>) -> (StatusCode, Value) {
        let mut r = Request::builder().method(metodo).uri(format!("/api{url}"));
        if corpo.is_some() {
            r = r.header("content-type", "application/json");
        }
        if let Some(c) = self.cookie.lock().unwrap().clone() {
            r = r.header("cookie", c);
        }
        let resp = self.app.clone().oneshot(r.body(corpo.map(|c| Body::from(c.to_string())).unwrap_or_default()).unwrap()).await.unwrap();
        if let Some(sc) = resp.headers().get("set-cookie").map(|v| v.to_str().unwrap().to_string()) {
            let par = sc.split(';').next().unwrap().to_string();
            *self.cookie.lock().unwrap() = if par.ends_with('=') { None } else { Some(par) };
        }
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn get(&self, url: &str) -> (StatusCode, Value) {
        self.chamar("GET", url, None).await
    }

    async fn post(&self, url: &str, corpo: Value) -> (StatusCode, Value) {
        self.chamar("POST", url, Some(corpo)).await
    }

    async fn put(&self, url: &str, corpo: Value) -> (StatusCode, Value) {
        self.chamar("PUT", url, Some(corpo)).await
    }

    /// Administrador logado e o produtor fictício cadastrado com vencimento no dia 10.
    async fn com_produtor(&self) -> i64 {
        let (s, r) = self.post("/auth/configurar", json!({ "login": "admin", "nome": "Administrador", "senha": "senha-do-admin" })).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        let (s, p) = self.post("/produtores", json!({ "pfx": B64.encode(PFX_CNPJ), "senha": "teste", "dia_vencimento": 10 })).await;
        assert_eq!(s, StatusCode::CREATED, "{p}");
        let id = p["id"].as_i64().unwrap();
        let (s, r) = self.post(&format!("/produtores/{id}/selecionar"), json!({})).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        id
    }

    async fn vencimento_em(&self, produtor: i64, dias_atras: u64) -> String {
        let data = (hoje() - Days::new(dias_atras)).to_string();
        let (s, r) = self.put(&format!("/admin/produtores/{produtor}/vencimento"), json!({ "vencimento": data })).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        data
    }
}

#[tokio::test]
async fn cadastro_exige_o_dia_do_vencimento() {
    let api = Api::nova(Some(GatewayFalso::default()));
    api.post("/auth/configurar", json!({ "login": "admin", "nome": "Administrador", "senha": "senha-do-admin" })).await;
    for dia in [json!(null), json!(0), json!(29)] {
        let (s, r) = api.post("/produtores", json!({ "pfx": B64.encode(PFX_CNPJ), "senha": "teste", "dia_vencimento": dia })).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{dia}: {r}");
        assert!(r["erro"].as_str().unwrap().contains("dia do vencimento"), "{r}");
    }
    let (s, _) = api.post("/produtores", json!({ "pfx": B64.encode(PFX_CNPJ), "senha": "teste", "dia_vencimento": 10 })).await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, estado) = api.get("/auth/estado").await;
    assert_eq!(estado["mensalidade_ativa"], true);
    let venc = estado["mensalidade"]["vencimento"].as_str().unwrap();
    assert!(venc.ends_with("-10") && venc > hoje().to_string().as_str(), "{venc}");
}

#[tokio::test]
async fn com_5_dias_de_atraso_bloqueia_a_emissao_e_o_pix_desbloqueia() {
    let gateway = GatewayFalso::default();
    let api = Api::nova(Some(gateway.clone()));
    let produtor = api.com_produtor().await;

    // 4 dias de atraso: aviso, mas ainda emite.
    api.vencimento_em(produtor, 4).await;
    assert_eq!(api.get("/emissor").await.0, StatusCode::OK);
    let (_, estado) = api.get("/auth/estado").await;
    assert_eq!(estado["mensalidade"]["situacao"], "atrasado");

    // 5 dias: bloqueado.
    let vencimento = api.vencimento_em(produtor, 5).await;
    let (s, r) = api.get("/emissor").await;
    assert_eq!(s, StatusCode::PAYMENT_REQUIRED, "{r}");
    assert_eq!(api.post("/nfe/validar", json!({})).await.0, StatusCode::PAYMENT_REQUIRED);
    let (s, m) = api.get("/mensalidade").await;
    assert_eq!(s, StatusCode::OK, "{m}");
    assert_eq!(m["resumo"]["situacao"], "bloqueado");

    // O Pix continua acessível bloqueado, e repetir não cria outro.
    let (s, pix) = api.post("/mensalidade/pix", json!({})).await;
    assert_eq!(s, StatusCode::OK, "{pix}");
    assert!(pix["copia_cola"].as_str().unwrap().starts_with("000201"));
    assert!(pix["qr_svg"].as_str().unwrap().contains("<svg"));
    let (_, de_novo) = api.post("/mensalidade/pix", json!({})).await;
    assert_eq!(de_novo["copia_cola"], pix["copia_cola"]);
    {
        let e = gateway.0.lock().unwrap();
        assert_eq!(e.por_chave.len(), 1);
        let (chave, valor, nome, documento) = &e.pedidos[0];
        assert!((16..=64).contains(&chave.len()) && chave.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "{chave}");
        assert_eq!((*valor, nome.as_str(), documento.as_str()), (9990, "EMPRESA TESTE", "12345678000195"));
    }

    // Pago no gateway: a conferência periódica reconhece, o vencimento avança um mês e libera.
    gateway.pagar_tudo();
    verificar_pendentes(&api.ctx).unwrap();
    assert_eq!(api.get("/emissor").await.0, StatusCode::OK);
    let (_, m) = api.get("/mensalidade").await;
    let proximo = chrono::NaiveDate::parse_from_str(&vencimento, "%Y-%m-%d").unwrap().checked_add_months(chrono::Months::new(1)).unwrap();
    assert_eq!(m["resumo"]["vencimento"], proximo.to_string());
    assert_eq!(m["pagas"][0]["vencimento"], vencimento);
    assert_eq!(m["pix"], Value::Null);

    // Conferir de novo não avança outra vez.
    verificar_pendentes(&api.ctx).unwrap();
    let (_, m) = api.get("/mensalidade").await;
    assert_eq!(m["resumo"]["vencimento"], proximo.to_string());
}

#[tokio::test]
async fn gateway_fora_do_ar_repete_com_a_mesma_chave() {
    let gateway = GatewayFalso::default();
    let api = Api::nova(Some(gateway.clone()));
    let produtor = api.com_produtor().await;
    api.vencimento_em(produtor, 1).await;

    gateway.0.lock().unwrap().fora_do_ar = true;
    let (s, r) = api.post("/mensalidade/pix", json!({})).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY, "{r}");
    gateway.0.lock().unwrap().fora_do_ar = false;
    let (s, _) = api.post("/mensalidade/pix", json!({})).await;
    assert_eq!(s, StatusCode::OK);

    let e = gateway.0.lock().unwrap();
    assert_eq!(e.pedidos.len(), 2);
    assert_eq!(e.pedidos[0].0, e.pedidos[1].0, "a repetição usa a mesma chave de idempotência");
    assert_eq!(e.por_chave.len(), 1);
}

#[tokio::test]
async fn pix_so_a_partir_de_10_dias_antes() {
    let api = Api::nova(Some(GatewayFalso::default()));
    let produtor = api.com_produtor().await;
    let longe = (hoje() + Days::new(20)).to_string();
    api.put(&format!("/admin/produtores/{produtor}/vencimento"), json!({ "vencimento": longe })).await;
    let (s, r) = api.post("/mensalidade/pix", json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{r}");
    assert!(r["erro"].as_str().unwrap().contains("disponível a partir"), "{r}");
}

#[tokio::test]
async fn so_o_administrador_muda_o_vencimento() {
    let api = Api::nova(Some(GatewayFalso::default()));
    let produtor = api.com_produtor().await;
    let (s, _) = api.post("/admin/usuarios", json!({ "login": "joao", "nome": "João", "senha": "senha-do-joao" })).await;
    assert_eq!(s, StatusCode::CREATED);
    api.post("/auth/sair", json!({})).await;
    api.post("/auth/entrar", json!({ "login": "joao", "senha": "senha-do-joao" })).await;
    let (s, _) = api.put(&format!("/admin/produtores/{produtor}/vencimento"), json!({ "vencimento": "2030-01-10" })).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn desligada_nao_pede_vencimento_nem_bloqueia() {
    let api = Api::nova(None);
    api.post("/auth/configurar", json!({ "login": "admin", "nome": "Administrador", "senha": "senha-do-admin" })).await;
    let (s, p) = api.post("/produtores", json!({ "pfx": B64.encode(PFX_CNPJ), "senha": "teste" })).await;
    assert_eq!(s, StatusCode::CREATED, "{p}");
    api.post(&format!("/produtores/{}/selecionar", p["id"]), json!({})).await;
    let (_, estado) = api.get("/auth/estado").await;
    assert_eq!(estado["mensalidade_ativa"], false);
    assert_eq!(estado["mensalidade"], Value::Null);
    assert_eq!(api.get("/emissor").await.0, StatusCode::OK);
    assert_eq!(api.post("/mensalidade/pix", json!({})).await.0, StatusCode::BAD_REQUEST);
}
