//! Integração: API + banco em memória + SEFAZ falsa (nada é enviado).
//! Os certificados usados são os fictícios de tests/fixtures (senha "teste").

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use emissor_nfe::db::Repositorio;
use emissor_nfe::emissao::{Contexto, Sefaz};
use emissor_nfe::exemplo::venda_soja;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use emissor_nfe::cofre::Cofre;
use emissor_nfe::fiscal::certificado::Certificado;
use emissor_nfe::fiscal::config::Config;
use emissor_nfe::fiscal::servicos::*;
use emissor_nfe::servidor::criar_app;
use emissor_nfe::simulada::SefazSimulada;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;

/// SEFAZ falsa: igual à simulada, mas pode rejeitar a próxima autorização.
struct SefazTeste {
    rejeitar_proxima: Arc<AtomicBool>,
}

impl Sefaz for SefazTeste {
    fn status(&self, c: &Config, cert: &Certificado) -> anyhow::Result<RetStatus> {
        SefazSimulada.status(c, cert)
    }
    fn autorizar(&self, c: &Config, cert: &Certificado, xml: &str) -> anyhow::Result<RetAutorizacao> {
        if !self.rejeitar_proxima.swap(false, Ordering::SeqCst) {
            return SefazSimulada.autorizar(c, cert, xml);
        }
        Ok(RetAutorizacao {
            lote: Lote { cStat: Some("104".into()), xMotivo: Some("Lote processado".into()), ..Default::default() },
            protocolo: Some(Protocolo { cStat: Some("539".into()), xMotivo: Some("Rejeicao: Duplicidade (teste)".into()), ..Default::default() }),
            resposta_soap: "<x/>".into(),
            ..Default::default()
        })
    }
    fn evento(&self, c: &Config, cert: &Certificado, env: &str) -> anyhow::Result<RetEvento> {
        SefazSimulada.evento(c, cert, env)
    }
    fn inutilizar(&self, c: &Config, cert: &Certificado, inut: &str) -> anyhow::Result<RetInut> {
        SefazSimulada.inutilizar(c, cert, inut)
    }
    fn consultar(&self, c: &Config, cert: &Certificado, chave: &str) -> anyhow::Result<RetConsSit> {
        SefazSimulada.consultar(c, cert, chave)
    }
}

struct Api {
    app: Router,
    rejeitar: Arc<AtomicBool>,
    /// Cookie de sessão deste "navegador".
    cookie: Mutex<Option<String>>,
}

struct Resposta {
    status: StatusCode,
    tipo: String,
    corpo: Vec<u8>,
}

impl Resposta {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.corpo).unwrap_or_else(|_| panic!("não é JSON: {}", String::from_utf8_lossy(&self.corpo)))
    }
    fn texto(&self) -> String {
        String::from_utf8_lossy(&self.corpo).into_owned()
    }
}

pub const PFX_CNPJ: &[u8] = include_bytes!("fixtures/teste.pfx"); // EMPRESA TESTE, 12345678000195 (fictício)
pub const PFX_CNPJ_LEGADO: &[u8] = include_bytes!("fixtures/teste-legado.pfx"); // mesmo CNPJ, outro formato
pub const PFX_CPF: &[u8] = include_bytes!("fixtures/teste-cpf.pfx"); // PRODUTOR TESTE, 52998224725 (fictício)

fn envio_pfx(pfx: &[u8], senha: &str) -> Value {
    json!({ "pfx": B64.encode(pfx), "senha": senha })
}

impl Api {
    /// Banco vazio, sem usuários (tela de configuração inicial).
    fn vazia() -> Self {
        let rejeitar = Arc::new(AtomicBool::new(false));
        let ctx = Contexto::novo(
            Config::teste(),
            Repositorio::abrir(":memory:").unwrap(),
            Box::new(SefazTeste { rejeitar_proxima: rejeitar.clone() }),
            Cofre::de_teste(),
        );
        Api { app: criar_app(Arc::new(ctx), None), rejeitar, cookie: Mutex::new(None) }
    }

    /// Administrador logado com o produtor fictício (e-CNPJ 12345678000195) selecionado.
    async fn nova() -> Self {
        let api = Api::vazia();
        let r = api.post("/auth/configurar", json!({ "login": "admin", "nome": "Administrador", "senha": "senha-do-admin" })).await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
        let r = api.post("/produtores", envio_pfx(PFX_CNPJ, "teste")).await;
        assert_eq!(r.status, StatusCode::CREATED, "{}", r.texto());
        api
    }

    /// Outro navegador, sem sessão, falando com o mesmo servidor.
    fn outro_navegador(&self) -> Api {
        Api { app: self.app.clone(), rejeitar: self.rejeitar.clone(), cookie: Mutex::new(None) }
    }

    async fn chamar(&self, metodo: &str, url: &str, corpo: Option<Value>) -> Resposta {
        let mut r = Request::builder().method(metodo).uri(format!("/api{url}"));
        if corpo.is_some() {
            r = r.header("content-type", "application/json");
        }
        if let Some(c) = self.cookie.lock().unwrap().clone() {
            r = r.header("cookie", c);
        }
        let req = r.body(corpo.map(|c| Body::from(c.to_string())).unwrap_or_default()).unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        if let Some(sc) = resp.headers().get("set-cookie").map(|v| v.to_str().unwrap().to_string()) {
            assert!(sc.contains("HttpOnly") && sc.contains("SameSite=Strict"), "{sc}");
            let par = sc.split(';').next().unwrap().to_string();
            *self.cookie.lock().unwrap() = if par.ends_with('=') { None } else { Some(par) };
        }
        let status = resp.status();
        let tipo = resp.headers().get("content-type").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
        let corpo = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        Resposta { status, tipo, corpo }
    }
    async fn get(&self, url: &str) -> Resposta {
        self.chamar("GET", url, None).await
    }
    async fn post(&self, url: &str, corpo: Value) -> Resposta {
        self.chamar("POST", url, Some(corpo)).await
    }
    async fn proximo(&self) -> i64 {
        self.get("/numeracao/1").await.json()["proximo"].as_i64().unwrap()
    }
}

fn nota(n_nf: i64) -> Value {
    let mut d = serde_json::to_value(venda_soja("52998224725", "EMPRESA TESTE")).unwrap();
    d["emit"]["CPF"] = Value::Null;
    d["emit"]["CNPJ"] = json!("12345678000195");
    d["ide"]["nNF"] = json!(n_nf);
    d
}

#[tokio::test]
async fn fluxo_completo() {
    let api = Api::nova().await;
    let e = api.get("/emissor").await.json();
    assert_eq!(e["podeInutilizar"], json!(true));
    assert_eq!(e["certificado"]["tipo"], json!("e-CNPJ"));
    assert_eq!(api.proximo().await, 1);

    // 1) rejeitada: grava, mas não consome o número
    api.rejeitar.store(true, Ordering::SeqCst);
    let r = api.post("/nfe/emitir", nota(1)).await.json();
    assert_eq!(r["status"], json!("rejeitada"), "{r}");
    assert_eq!(r["protocolo"]["cStat"], json!("539"));
    assert_eq!(api.proximo().await, 1);

    // 2) autorizada: consome o número e gera nfeProc
    let r = api.post("/nfe/emitir", nota(1)).await.json();
    assert_eq!(r["status"], json!("autorizada"), "{r}");
    assert_eq!(r["xsd"]["valido"], json!(true));
    assert_eq!(r["assinaturaValida"], json!(true));
    assert!(r["protocolo"].get("xml").is_none(), "o XML do protocolo não vai na resposta");
    let id = r["id"].as_i64().unwrap();
    assert_eq!(api.proximo().await, 2);

    // número repetido é barrado antes de enviar
    let dup = api.post("/nfe/emitir", nota(1)).await;
    assert_eq!(dup.status, StatusCode::BAD_REQUEST);
    let msg = dup.json()["erro"].as_str().unwrap().to_string();
    assert!(msg.contains("já foi usado") && msg.contains("Próximo disponível: 2"), "{msg}");

    // listagem e detalhe
    let lista = api.get("/notas").await.json();
    assert_eq!(lista["total"], json!(2));
    let status: Vec<_> = lista["notas"].as_array().unwrap().iter().map(|n| n["status"].as_str().unwrap()).collect();
    assert_eq!(status, ["autorizada", "rejeitada"]);
    let det = api.get(&format!("/notas/{id}")).await.json();
    assert_eq!(det["tem_proc"], json!(true));
    assert_eq!(det["dados"]["ide"]["nNF"], json!(1));
    assert!(det.get("xml_nfe").is_none());

    // XML autorizado e DANFE
    let x = api.get(&format!("/notas/{id}/xml")).await;
    assert!(x.tipo.starts_with("application/xml"));
    let x = x.texto();
    assert!(x.starts_with("<?xml") && x.contains("<nfeProc ") && x.ends_with("</nfeProc>"), "{x}");
    let danfe = api.get(&format!("/notas/{id}/danfe")).await;
    assert_eq!(danfe.tipo, "application/pdf");
    assert_eq!(&danfe.corpo[..5], b"%PDF-");

    // CC-e: sequência 1 e 2
    let c1 = api.post(&format!("/notas/{id}/cce"), json!({ "xCorrecao": "Corrigir complemento do endereco para Sala 2" })).await.json();
    let c2 = api.post(&format!("/notas/{id}/cce"), json!({ "xCorrecao": "Corrigir complemento do endereco para Sala 3" })).await.json();
    assert_eq!((c1["nSeqEvento"].clone(), c2["nSeqEvento"].clone(), c1["registrado"].clone()), (json!(1), json!(2), json!(true)));
    assert_eq!(api.post(&format!("/notas/{id}/cce"), json!({ "xCorrecao": "curta" })).await.status, StatusCode::BAD_REQUEST);

    // cancelamento
    assert_eq!(api.post(&format!("/notas/{id}/cancelar"), json!({ "xJust": "curta" })).await.status, StatusCode::BAD_REQUEST);
    let canc = api.post(&format!("/notas/{id}/cancelar"), json!({ "xJust": "Cancelamento de teste automatizado" })).await.json();
    assert_eq!(canc["registrado"], json!(true), "{canc}");
    let depois = api.get(&format!("/notas/{id}")).await.json();
    assert_eq!(depois["status"], json!("cancelada"));
    let eventos = depois["eventos"].as_array().unwrap();
    assert_eq!(eventos.len(), 3);
    let ev = api.get(&format!("/eventos/{}/xml", eventos[2]["id"])).await.texto();
    assert!(ev.contains("<procEventoNFe ") && ev.contains("<tpEvento>110111</tpEvento>") && ev.ends_with("</procEventoNFe>"));
    // cancelada continua consumindo o número; não cancela de novo
    assert_eq!(api.proximo().await, 2);
    let lista = api.get("/notas").await.json();
    assert_eq!(lista["notas"][0]["cces"], json!(2));
    let r = api.post(&format!("/notas/{id}/cancelar"), json!({ "xJust": "Cancelamento de teste automatizado" })).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // inutilização: não pode incluir número usado; faixa livre avança a numeração
    let usado = api.post("/inutilizacoes", json!({ "serie": 1, "nNFIni": 1, "nNFFin": 3, "xJust": "Teste de inutilizacao automatizado" })).await;
    assert_eq!(usado.status, StatusCode::BAD_REQUEST);
    let inut = api.post("/inutilizacoes", json!({ "serie": 1, "nNFIni": 2, "nNFFin": 4, "xJust": "Teste de inutilizacao automatizado" })).await.json();
    assert_eq!(inut["homologada"], json!(true), "{inut}");
    assert_eq!(api.proximo().await, 5);
    assert_eq!(api.get("/inutilizacoes").await.json().as_array().unwrap().len(), 1);

    // consulta (simulada)
    assert_eq!(api.get(&format!("/notas/{id}/consultar")).await.json()["cStat"], json!("100"));
}

#[tokio::test]
async fn erros_de_dados_viram_400_e_previa_do_danfe() {
    let api = Api::nova().await;
    let mut d = nota(9);
    d["itens"][0]["NCM"] = json!("12");
    let r = api.post("/nfe/validar", d).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(r.json()["erro"].as_str().unwrap().contains("NCM"));

    let v = api.post("/nfe/validar", nota(9)).await.json();
    assert_eq!(v["xsd"]["valido"], json!(true), "{v}");

    let previa = api.post("/nfe/danfe-previa", nota(9)).await;
    assert_eq!(previa.tipo, "application/pdf");
    assert_eq!(api.get("/notas/999").await.status, StatusCode::NOT_FOUND);
    assert_eq!(api.get("/notas/abc").await.status, StatusCode::NOT_FOUND);
    let st = api.get("/sefaz/status").await;
    assert_eq!(st.json()["cStat"], json!("107"));
    let r = api.chamar("POST", "/nfe/emitir", None).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn cadastro_de_produtos() {
    let api = Api::nova().await;
    assert_eq!(api.get("/produtos").await.json(), json!([]));

    let soja = json!({ "cProd": "SOJA01", "xProd": "soja em graos", "NCM": "1201.90.00", "CFOP": "5101", "uCom": "SC", "vUnCom": "132,50" });
    let r = api.post("/produtos", soja.clone()).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let criado = r.json();
    let id = criado["id"].as_i64().unwrap();
    assert_eq!(criado["xProd"], json!("SOJA EM GRAOS"));
    assert_eq!(criado["NCM"], json!("12019000"));

    // código repetido (sem diferenciar maiúsculas) e dados inválidos → 400
    let mut dup = soja.clone();
    dup["cProd"] = json!("soja01");
    let r = api.post("/produtos", dup).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(r.json()["erro"].as_str().unwrap().contains("Já existe"));
    let mut ruim = soja.clone();
    ruim["NCM"] = json!("12");
    assert_eq!(api.post("/produtos", ruim).await.status, StatusCode::BAD_REQUEST);

    // segundo produto, alteração e listagem em ordem de descrição
    let milho = json!({ "cProd": "MILHO01", "xProd": "MILHO", "NCM": "10059010", "CFOP": "5101", "uCom": "SC",
                        "icms": { "orig": "0", "CST": "00", "pICMS": "12" } });
    let id_milho = api.post("/produtos", milho).await.json()["id"].as_i64().unwrap();
    let mut alterado = soja.clone();
    alterado["vUnCom"] = json!("140,00");
    let r = api.chamar("PUT", &format!("/produtos/{id}"), Some(alterado)).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    let lista = api.get("/produtos").await.json();
    let nomes: Vec<_> = lista.as_array().unwrap().iter().map(|p| p["xProd"].as_str().unwrap()).collect();
    assert_eq!(nomes, ["MILHO", "SOJA EM GRAOS"]);
    assert_eq!(lista[1]["vUnCom"], json!("140,00"));
    assert_eq!(lista[0]["icms"]["pICMS"], json!("12"));

    // alterar para o código de outro produto é barrado
    let mut troca = soja.clone();
    troca["cProd"] = json!("MILHO01");
    assert_eq!(api.chamar("PUT", &format!("/produtos/{id}"), Some(troca)).await.status, StatusCode::BAD_REQUEST);

    // exclusão
    assert_eq!(api.chamar("DELETE", &format!("/produtos/{id_milho}"), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api.chamar("DELETE", &format!("/produtos/{id_milho}"), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(api.get("/produtos").await.json().as_array().unwrap().len(), 1);
}

// ---------- login, produtores e administração ----------

#[tokio::test]
async fn primeiro_acesso_login_e_bloqueio() {
    let api = Api::vazia();
    let e = api.get("/auth/estado").await.json();
    assert_eq!((e["configurado"].clone(), e["usuario"].clone()), (json!(false), Value::Null));
    assert_eq!(api.get("/notas").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.get("/produtores").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.get("/admin/usuarios").await.status, StatusCode::UNAUTHORIZED);

    // configuração inicial: senha fraca é recusada; depois cria o administrador e já entra
    let r = api.post("/auth/configurar", json!({ "login": "rodrigo", "nome": "Rodrigo", "senha": "curta" })).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = api.post("/auth/configurar", json!({ "login": "Rodrigo", "nome": "Rodrigo", "senha": "uma-senha-boa" })).await.json();
    assert_eq!(r["usuario"]["login"], json!("rodrigo"));
    assert_eq!(r["usuario"]["admin"], json!(true));
    assert_eq!(r["produtor"], Value::Null);
    // sem produtor selecionado, a emissão pede para escolher um
    assert_eq!(api.get("/notas").await.status, StatusCode::CONFLICT);

    // configurar de novo não é possível (nem de outro navegador)
    let intruso = api.outro_navegador();
    let r = intruso.post("/auth/configurar", json!({ "login": "intruso", "nome": "X", "senha": "outra-senha-boa" })).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert_eq!(intruso.get("/auth/estado").await.json()["configurado"], json!(true));

    // sair encerra a sessão
    assert_eq!(api.post("/auth/sair", json!({})).await.status, StatusCode::NO_CONTENT);
    assert!(api.cookie.lock().unwrap().is_none());
    assert_eq!(api.get("/produtores").await.status, StatusCode::UNAUTHORIZED);

    // senha errada e usuário inexistente dão a mesma mensagem
    let r1 = api.post("/auth/entrar", json!({ "login": "rodrigo", "senha": "errada-123" })).await;
    let r2 = api.post("/auth/entrar", json!({ "login": "ninguem", "senha": "errada-123" })).await;
    assert_eq!(r1.status, StatusCode::UNAUTHORIZED);
    assert_eq!(r1.json()["erro"], r2.json()["erro"]);
    let r = api.post("/auth/entrar", json!({ "login": "RODRIGO", "senha": "uma-senha-boa" })).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    assert_eq!(api.get("/produtores").await.status, StatusCode::OK);

    // 5 senhas erradas seguidas bloqueiam o login, mesmo com a senha certa depois
    for _ in 0..5 {
        intruso.post("/auth/entrar", json!({ "login": "rodrigo", "senha": "chute-qualquer" })).await;
    }
    let r = intruso.post("/auth/entrar", json!({ "login": "rodrigo", "senha": "uma-senha-boa" })).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.json()["erro"].as_str().unwrap().contains("Muitas tentativas"));
    // a sessão já aberta continua valendo
    assert_eq!(api.get("/produtores").await.status, StatusCode::OK);

    // cookie forjado não vale
    *intruso.cookie.lock().unwrap() = Some(format!("emissor_sessao={}", "a".repeat(64)));
    assert_eq!(intruso.get("/produtores").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn produtores_troca_e_isolamento_dos_dados() {
    let api = Api::nova().await; // e-CNPJ fictício já selecionado
    let cnpj = api.get("/produtores").await.json()[0]["id"].as_i64().unwrap();
    assert_eq!(api.get("/emissor").await.json()["certificado"]["tipo"], json!("e-CNPJ"));

    // nota e produto do produtor CNPJ
    assert_eq!(api.post("/nfe/emitir", nota(1)).await.json()["status"], json!("autorizada"));
    api.post("/produtos", json!({ "cProd": "SOJA01", "xProd": "SOJA", "NCM": "12019000", "CFOP": "5101", "uCom": "SC" })).await;

    // adicionar o segundo produtor (e-CPF fictício): senha errada, depois certa
    let r = api.post("/produtores", envio_pfx(PFX_CPF, "errada")).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    assert!(r.json()["erro"].as_str().unwrap().contains("senha incorreta"));
    let r = api.post("/produtores", envio_pfx(PFX_CPF, "teste")).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.texto());
    let cpf = r.json()["id"].as_i64().unwrap();
    assert_eq!(r.json()["documento"], json!("52998224725"));

    // o recém-adicionado fica selecionado; os dados do outro produtor não aparecem
    let e = api.get("/emissor").await.json();
    assert_eq!((e["certificado"]["tipo"].clone(), e["podeInutilizar"].clone()), (json!("e-CPF"), json!(false)));
    assert_eq!(api.get("/notas").await.json()["total"], json!(0));
    assert_eq!(api.get("/notas/1").await.status, StatusCode::NOT_FOUND);
    assert_eq!(api.get("/notas/1/xml").await.status, StatusCode::NOT_FOUND);
    assert_eq!(api.get("/produtos").await.json(), json!([]));
    assert_eq!(api.proximo().await, 1, "numeração é por produtor");

    // trocar de volta
    let r = api.post(&format!("/produtores/{cnpj}/selecionar"), json!({})).await;
    assert_eq!(r.json()["documento"], json!("12345678000195"));
    assert_eq!(api.get("/notas").await.json()["total"], json!(1));
    assert_eq!(api.get("/produtos").await.json().as_array().unwrap().len(), 1);
    assert_eq!(api.proximo().await, 2);

    // mesmo certificado de novo: recusado; renovação só com o mesmo documento
    let r = api.post("/produtores", envio_pfx(PFX_CNPJ_LEGADO, "teste")).await;
    assert!(r.json()["erro"].as_str().unwrap().contains("já está cadastrado"));
    let r = api.chamar("PUT", &format!("/produtores/{cnpj}/certificado"), Some(envio_pfx(PFX_CPF, "teste"))).await;
    assert!(r.json()["erro"].as_str().unwrap().contains("outro documento"));
    let r = api.chamar("PUT", &format!("/produtores/{cnpj}/certificado"), Some(envio_pfx(PFX_CNPJ_LEGADO, "teste"))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    assert_eq!(api.post("/nfe/validar", nota(2)).await.json()["assinaturaValida"], json!(true));

    // emitente padrão fica guardado no produtor (CPF/CNPJ não: vêm do certificado)
    let emit = json!({ "xNome": "EMPRESA TESTE", "IE": "109876547", "CRT": "3", "CNPJ": "99999999999999", "ender": { "xMun": "GOIANIA" } });
    let r = api.chamar("PUT", &format!("/produtores/{cnpj}/emitente"), Some(emit)).await.json();
    assert_eq!(r.get("CNPJ"), None);
    assert_eq!(api.get("/emissor").await.json()["produtor"]["emitente"]["IE"], json!("109876547"));
    assert_eq!(api.get("/auth/estado").await.json()["produtor"]["emitente"]["ender"]["xMun"], json!("GOIANIA"));

    // remover o produtor CPF: some da lista; se volta, volta com o mesmo cadastro
    assert_eq!(api.chamar("DELETE", &format!("/produtores/{cpf}"), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(api.get("/produtores").await.json().as_array().unwrap().len(), 1);
    assert_eq!(api.post(&format!("/produtores/{cpf}/selecionar"), json!({})).await.status, StatusCode::BAD_REQUEST);
    let r = api.post("/produtores", envio_pfx(PFX_CPF, "teste")).await.json();
    assert_eq!(r["id"].as_i64(), Some(cpf));

    // remover o produtor selecionado deixa a sessão sem produtor
    api.post(&format!("/produtores/{cpf}/selecionar"), json!({})).await;
    api.chamar("DELETE", &format!("/produtores/{cpf}"), None).await;
    assert_eq!(api.get("/notas").await.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn administrador_gerencia_usuarios_e_cada_um_ve_os_seus_produtores() {
    let admin = Api::nova().await;
    let cnpj = admin.get("/produtores").await.json()[0]["id"].as_i64().unwrap();

    // só o administrador cria usuários
    let r = admin.post("/admin/usuarios", json!({ "login": "joao", "nome": "João", "senha": "senha-do-joao" })).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.texto());
    let joao_id = r.json()["id"].as_i64().unwrap();
    let r = admin.post("/admin/usuarios", json!({ "login": "JOAO", "nome": "Outro", "senha": "senha-do-joao2" })).await;
    assert!(r.json()["erro"].as_str().unwrap().contains("já está em uso"));

    let joao = admin.outro_navegador();
    let r = joao.post("/auth/entrar", json!({ "login": "joao", "senha": "senha-do-joao" })).await.json();
    assert_eq!(r["usuario"]["admin"], json!(false));
    assert_eq!(joao.get("/admin/usuarios").await.status, StatusCode::FORBIDDEN);
    let r = joao.post("/admin/usuarios", json!({ "login": "x123", "nome": "X", "senha": "senha-qualquer" })).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // João não vê nem usa o produtor do administrador
    assert_eq!(joao.get("/produtores").await.json(), json!([]));
    assert_eq!(joao.post(&format!("/produtores/{cnpj}/selecionar"), json!({})).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(joao.chamar("DELETE", &format!("/produtores/{cnpj}"), None).await.status, StatusCode::BAD_REQUEST);
    let r = joao.post("/produtores", envio_pfx(PFX_CNPJ, "teste")).await;
    assert!(r.json()["erro"].as_str().unwrap().contains("outro usuário"));
    let r = joao.post("/produtores", envio_pfx(PFX_CPF, "teste")).await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(joao.get("/produtores").await.json().as_array().unwrap().len(), 1);

    // o administrador vê todos, com o dono
    let todos = admin.get("/produtores").await.json();
    let donos: Vec<_> = todos.as_array().unwrap().iter().map(|p| p["dono_nome"].as_str().unwrap()).collect();
    assert_eq!(donos, ["Administrador", "João"]);
    let lista = admin.get("/admin/usuarios").await.json();
    assert_eq!(lista.as_array().unwrap().len(), 2);
    assert!(!lista.to_string().contains("senha"), "a lista não expõe hash de senha");

    // o administrador não pode se rebaixar nem se desativar
    let eu = lista.as_array().unwrap().iter().find(|u| u["login"] == "admin").unwrap()["id"].as_i64().unwrap();
    let r = admin.chamar("PUT", &format!("/admin/usuarios/{eu}"), Some(json!({ "admin": false }))).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // João troca a própria senha: a senha atual é conferida e as outras sessões dele caem
    let joao2 = admin.outro_navegador();
    joao2.post("/auth/entrar", json!({ "login": "joao", "senha": "senha-do-joao" })).await;
    let r = joao.post("/auth/senha", json!({ "atual": "errada", "nova": "nova-senha-joao" })).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = joao.post("/auth/senha", json!({ "atual": "senha-do-joao", "nova": "nova-senha-joao" })).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(joao.get("/produtores").await.status, StatusCode::OK);
    assert_eq!(joao2.get("/produtores").await.status, StatusCode::UNAUTHORIZED);

    // desativar derruba a sessão e impede o login
    let r = admin.chamar("PUT", &format!("/admin/usuarios/{joao_id}"), Some(json!({ "ativo": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    assert_eq!(joao.get("/produtores").await.status, StatusCode::UNAUTHORIZED);
    let r = joao.post("/auth/entrar", json!({ "login": "joao", "senha": "nova-senha-joao" })).await;
    assert!(r.json()["erro"].as_str().unwrap().contains("desativado"));

    // reativar com senha redefinida pelo administrador
    admin.chamar("PUT", &format!("/admin/usuarios/{joao_id}"), Some(json!({ "ativo": true, "senha": "senha-redefinida" }))).await;
    let r = joao.post("/auth/entrar", json!({ "login": "joao", "senha": "senha-redefinida" })).await;
    assert_eq!(r.status, StatusCode::OK);
}

#[tokio::test]
async fn requisicao_de_outra_origem_e_recusada() {
    let api = Api::nova().await;
    let cookie = api.cookie.lock().unwrap().clone().unwrap();
    let pedido = |rota: &str, origem: &str| {
        Request::builder()
            .method("POST")
            .uri(format!("/api{rota}"))
            .header("content-type", "application/json")
            .header("cookie", cookie.clone())
            .header("origin", origem)
            .header("host", "127.0.0.1:3335")
            .body(Body::from(nota(1).to_string()))
            .unwrap()
    };
    let resp = api.app.clone().oneshot(pedido("/nfe/emitir", "http://site-malicioso.com")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let resp = api.app.clone().oneshot(pedido("/nfe/validar", "http://127.0.0.1:3335")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn banco_antigo_e_migrado_e_os_dados_vao_para_o_produtor() {
    let arq = std::env::temp_dir().join(format!("emissor-migracao-{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&arq);
    {
        // esquema de antes do login (versão 0), com uma nota, um produto e uma inutilização
        let db = rusqlite::Connection::open(&arq).unwrap();
        db.execute_batch(
            "CREATE TABLE notas (id INTEGER PRIMARY KEY AUTOINCREMENT, chave TEXT NOT NULL UNIQUE, serie INTEGER NOT NULL, nNF INTEGER NOT NULL,
               status TEXT NOT NULL, cStat TEXT, xMotivo TEXT, nProt TEXT, dhRecbto TEXT, vNF TEXT NOT NULL, emit_doc TEXT, dest_doc TEXT,
               dest_nome TEXT, dados_json TEXT NOT NULL, xml_nfe TEXT NOT NULL, xml_proc TEXT, xml_resposta TEXT,
               criada_em TEXT NOT NULL DEFAULT (datetime('now', 'localtime')));
             CREATE TABLE inutilizacoes (id INTEGER PRIMARY KEY AUTOINCREMENT, serie INTEGER NOT NULL, nNFIni INTEGER NOT NULL, nNFFin INTEGER NOT NULL,
               xJust TEXT NOT NULL, cStat TEXT, xMotivo TEXT, nProt TEXT, homologada INTEGER NOT NULL DEFAULT 0, xml_proc TEXT, xml_resposta TEXT,
               criado_em TEXT NOT NULL DEFAULT (datetime('now', 'localtime')));
             CREATE TABLE produtos (id INTEGER PRIMARY KEY AUTOINCREMENT, cProd TEXT NOT NULL UNIQUE COLLATE NOCASE, xProd TEXT NOT NULL,
               dados_json TEXT NOT NULL, criado_em TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
               alterado_em TEXT NOT NULL DEFAULT (datetime('now', 'localtime')));
             INSERT INTO notas (chave, serie, nNF, status, vNF, emit_doc, dados_json, xml_nfe)
               VALUES ('52000000000000000000000000000000000000000001', 1, 7, 'autorizada', '10.00', '12345678000195', '{}', '<NFe/>');
             INSERT INTO produtos (cProd, xProd, dados_json) VALUES ('SOJA01', 'SOJA', '{}');
             INSERT INTO inutilizacoes (serie, nNFIni, nNFFin, xJust, homologada) VALUES (1, 8, 9, 'teste de migracao', 1);",
        )
        .unwrap();
    }
    let repo = Repositorio::abrir(arq.to_str().unwrap()).unwrap();
    let ctx = Contexto::novo(Config::teste(), repo, Box::new(SefazSimulada), Cofre::de_teste());
    let admin = ctx.repo().criar_usuario("admin", "Admin", "x", true).unwrap();
    let s = emissor_nfe::contas::SessaoAtual {
        token_hash: "t".into(),
        usuario: ctx.repo().usuario(admin).unwrap().unwrap(),
        produtor_id: None,
    };
    let p = emissor_nfe::contas::adicionar_produtor(&ctx, &s, &envio_pfx(PFX_CNPJ, "teste")).unwrap();
    let id = p["id"].as_i64().unwrap();
    {
        let repo = ctx.repo();
        assert_eq!(repo.listar_notas(id, &emissor_nfe::db::FiltroNotas::default(), 1, 50).unwrap()["total"], json!(1));
        assert_eq!(repo.listar_produtos(id).unwrap().len(), 1);
        assert_eq!(repo.listar_inutilizacoes(id).unwrap().len(), 1);
        assert_eq!(repo.proximo_numero(id, 1).unwrap(), 10);
    }
    drop(ctx);
    // abrir de novo não migra outra vez
    Repositorio::abrir(arq.to_str().unwrap()).unwrap();
    for sufixo in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{sufixo}", arq.display()));
    }
}

// ---------- clientes, filtros, exportação, painel e notas referenciadas ----------

fn cliente(doc: &str, nome: &str) -> Value {
    json!({ "tipo": "PF", "CPF": doc, "xNome": nome, "indIEDest": "9",
            "ender": { "xLgr": "RUA A", "nro": "1", "xBairro": "CENTRO", "cMun": "5208707", "xMun": "GOIANIA", "UF": "GO", "CEP": "74000000" } })
}

#[tokio::test]
async fn cadastro_de_clientes_separado_por_produtor() {
    let api = Api::nova().await;
    assert_eq!(api.get("/clientes").await.json(), json!([]));
    let r = api.post("/clientes", cliente("529.982.247-25", "joao")).await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.texto());
    let id = r.json()["id"].as_i64().unwrap();
    assert_eq!(r.json()["CPF"], json!("52998224725"));
    assert_eq!(r.json()["xNome"], json!("JOAO"));

    // mesmo documento é recusado; documento inválido também
    assert!(api.post("/clientes", cliente("52998224725", "OUTRO")).await.json()["erro"].as_str().unwrap().contains("Já existe"));
    assert_eq!(api.post("/clientes", cliente("111", "X Y")).await.status, StatusCode::BAD_REQUEST);

    // alterar mantendo o documento funciona; ordem alfabética
    let r = api.chamar("PUT", &format!("/clientes/{id}"), Some(cliente("52998224725", "ZECA"))).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    api.post("/clientes", cliente("39053344705", "ANA")).await;
    let lista = api.get("/clientes").await.json();
    let nomes: Vec<_> = lista.as_array().unwrap().iter().map(|c| c["xNome"].as_str().unwrap()).collect();
    assert_eq!(nomes, ["ANA", "ZECA"]);

    // outro produtor não vê nem altera
    api.post("/produtores", envio_pfx(PFX_CPF, "teste")).await;
    assert_eq!(api.get("/clientes").await.json(), json!([]));
    assert_eq!(api.chamar("PUT", &format!("/clientes/{id}"), Some(cliente("52998224725", "HACK"))).await.status, StatusCode::BAD_REQUEST);
    assert_eq!(api.chamar("DELETE", &format!("/clientes/{id}"), None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn filtros_e_paginacao_das_notas() {
    let api = Api::nova().await;
    for n in 1..=3 {
        let mut d = nota(n);
        d["dest"]["xNome"] = json!(if n == 3 { "FAZENDA 100% BOA" } else { "COMPRADOR" });
        api.post("/nfe/emitir", d).await;
    }
    api.rejeitar.store(true, Ordering::SeqCst);
    api.post("/nfe/emitir", nota(4)).await;

    let total = |r: Resposta| r.json()["total"].as_i64().unwrap();
    assert_eq!(total(api.get("/notas").await), 4);
    assert_eq!(total(api.get("/notas?status=rejeitada").await), 1);
    assert_eq!(total(api.get("/notas?status=autorizada").await), 3);
    assert_eq!(total(api.get("/notas?busca=fazenda").await), 1);
    assert_eq!(total(api.get("/notas?busca=100%25%20boa").await), 1);
    assert_eq!(total(api.get("/notas?busca=a%25boa").await), 0, "% é texto, não curinga");
    assert_eq!(total(api.get("/notas?busca=%25%25%25").await), 0);
    let hoje = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert_eq!(total(api.get(&format!("/notas?de={hoje}&ate={hoje}")).await), 4);
    assert_eq!(total(api.get("/notas?ate=2000-01-01").await), 0);
    // número exato ou CPF/CNPJ
    let so_tres = api.get("/notas?busca=3").await.json();
    assert!(so_tres["notas"].as_array().unwrap().iter().any(|n| n["nNF"] == json!(3)));
    assert_eq!(total(api.get("/notas?busca=abc'%20OR%201=1--").await), 0, "sem injeção de SQL");
    // paginação
    let p = api.get("/notas?pagina=2").await.json();
    assert_eq!((p["pagina"].clone(), p["porPagina"].clone(), p["notas"].as_array().unwrap().len()), (json!(2), json!(50), 0));
}

#[tokio::test]
async fn exportacao_para_o_contador_e_painel() {
    let api = Api::nova().await;
    let hoje = chrono::Local::now().format("%Y-%m-%d").to_string();
    let url = format!("/notas/exportar?de={hoje}&ate={hoje}");
    assert!(api.get(&url).await.json()["erro"].as_str().unwrap().contains("Nenhum documento"));
    assert_eq!(api.get("/notas/exportar?de=ontem&ate=hoje").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(api.get("/notas/exportar?de=2026-02-01&ate=2026-01-01").await.status, StatusCode::BAD_REQUEST);

    // autorizada, cancelada e rejeitada
    let a1 = api.post("/nfe/emitir", nota(1)).await.json();
    let a2 = api.post("/nfe/emitir", nota(2)).await.json();
    api.post(&format!("/notas/{}/cancelar", a2["id"]), json!({ "xJust": "Cancelamento de teste automatizado" })).await;
    api.rejeitar.store(true, Ordering::SeqCst);
    let rej = api.post("/nfe/emitir", nota(3)).await.json();
    api.post("/inutilizacoes", json!({ "serie": 1, "nNFIni": 7, "nNFFin": 8, "xJust": "Teste de inutilizacao automatizado" })).await;

    let r = api.get(&url).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.texto());
    assert_eq!(r.tipo, "application/zip");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(r.corpo)).unwrap();
    let nomes: Vec<String> = z.file_names().map(|n| n.unwrap().to_string()).collect();
    let tem = |n: String| assert!(nomes.contains(&n), "{n} não está em {nomes:?}");
    tem(format!("notas/{}-procNFe.xml", a1["chave"].as_str().unwrap()));
    tem(format!("notas/{}-procNFe.xml", a2["chave"].as_str().unwrap()));
    tem(format!("eventos/{}-cancelamento-1.xml", a2["chave"].as_str().unwrap()));
    tem("inutilizacoes/inutilizacao-serie1-7-8.xml".to_string());
    assert!(!nomes.iter().any(|n| n.contains(rej["chave"].as_str().unwrap())), "rejeitada fica de fora");

    let mut csv = String::new();
    std::io::Read::read_to_string(&mut z.by_name("resumo.csv").unwrap(), &mut csv).unwrap();
    assert!(csv.starts_with('\u{feff}'));
    assert_eq!(csv.lines().count(), 3, "{csv}");
    assert!(csv.contains("\"cancelada\"") && csv.contains(a1["chave"].as_str().unwrap()));
    assert!(!csv.contains(rej["chave"].as_str().unwrap()));

    // painel do mês: cancelada não soma no valor
    let p = api.get("/painel").await.json();
    assert_eq!(p["mes"]["autorizadas"], json!(1));
    assert_eq!(p["mes"]["canceladas"], json!(1));
    assert_eq!(p["mes"]["rejeitadas"], json!(1));
    let centavos = (a1["vNF"].as_str().unwrap().parse::<f64>().unwrap() * 100.0).round() as i64;
    assert_eq!(p["mes"]["valor_autorizado"], json!(centavos));
    assert_eq!(p["meses"].as_array().unwrap().len(), 6);
    assert_eq!(p["meses"][5]["valor"], json!(centavos));
    assert_eq!(p["meses"][5]["mes"], json!(hoje[..7]));
    assert_eq!(p["meses"][0]["valor"], json!(0));

    // outro produtor: nada disso aparece nem é exportado
    api.post("/produtores", envio_pfx(PFX_CPF, "teste")).await;
    assert_eq!(api.get("/painel").await.json()["mes"]["autorizadas"], json!(0));
    assert_eq!(api.get(&url).await.status, StatusCode::BAD_REQUEST);
    // sem produtor nem login
    assert_eq!(api.outro_navegador().get("/painel").await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(api.outro_navegador().get(&url).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn devolucao_entrada_e_remessa_passam_no_schema() {
    let api = Api::nova().await;
    let original = api.post("/nfe/emitir", nota(1)).await.json();
    let chave = original["chave"].as_str().unwrap().to_string();

    // devolução sem nota referenciada é barrada; com chave inválida também
    let mut d = nota(2);
    d["ide"]["finNFe"] = json!(4);
    d["ide"]["natOp"] = json!("DEVOLUCAO DE COMPRA");
    d["itens"][0]["CFOP"] = json!("5201");
    d["pag"] = json!([{ "tPag": "90" }]);
    assert!(api.post("/nfe/validar", d.clone()).await.json()["erro"].as_str().unwrap().contains("referenciada"));
    d["ide"]["refNFe"] = json!(["52000000000000000000000000000000000000000000"]);
    assert!(api.post("/nfe/validar", d.clone()).await.json()["erro"].as_str().unwrap().contains("inválida"));
    d["ide"]["refNFe"] = json!([chave]);
    let v = api.post("/nfe/validar", d.clone()).await.json();
    assert_eq!(v["xsd"]["valido"], json!(true), "{v}");

    // a referência vai no XML, no fim de <ide>, e aparece no DANFE
    let r = api.post("/nfe/emitir", d.clone()).await.json();
    assert_eq!(r["status"], json!("autorizada"), "{r}");
    let xml = api.get(&format!("/notas/{}/xml", r["id"])).await.texto();
    assert!(xml.contains(&format!("<NFref><refNFe>{chave}</refNFe></NFref></ide>")), "{xml}");
    assert!(xml.contains("<finNFe>4</finNFe>") && xml.contains("<tPag>90</tPag><vPag>0.00</vPag>"));
    assert_eq!(api.get(&format!("/notas/{}/danfe", r["id"])).await.tipo, "application/pdf");

    // entrada (tpNF 0) e remessa
    let mut e = nota(3);
    e["ide"]["tpNF"] = json!(0);
    e["itens"][0]["CFOP"] = json!("1101");
    assert_eq!(api.post("/nfe/validar", e).await.json()["xsd"]["valido"], json!(true));
    let mut re = nota(4);
    re["itens"][0]["CFOP"] = json!("5905");
    re["pag"] = json!([{ "tPag": "90" }]);
    assert_eq!(api.post("/nfe/validar", re).await.json()["xsd"]["valido"], json!(true));
}
