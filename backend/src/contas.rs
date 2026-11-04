//! Usuários, sessões e produtores (emitentes identificados pelo certificado A1).
//!
//! Regras de acesso:
//! - O primeiro usuário é criado pela tela de configuração inicial e é administrador.
//!   Depois disso, só um administrador cria usuários.
//! - Cada produtor pertence ao usuário que cadastrou o certificado; o administrador vê todos.
//! - O certificado (.pfx e senha) fica cifrado no banco (ver `cofre.rs`) e só é aberto
//!   para assinar/transmitir pelo produtor ativo da sessão.

use crate::auth::{self, DURACAO_SESSAO};
use crate::db::{CertificadoProdutor, Produtor, Usuario};
use crate::emissao::{Ativo, Contexto, ErroNegocio};
use crate::fiscal::certificado::{Certificado, ler_pfx};
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use std::sync::Arc;

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

/// Sessão válida de um usuário ativo.
#[derive(Debug, Clone)]
pub struct SessaoAtual {
    pub token_hash: String,
    pub usuario: Usuario,
    pub produtor_id: Option<i64>,
}

pub fn usuario_json(u: &Usuario) -> Value {
    json!({ "id": u.id, "login": u.login, "nome": u.nome, "admin": u.admin })
}

// ---------- login ----------

/// Confere o token do cookie; renova o prazo da sessão a cada uso.
pub fn sessao_por_token(ctx: &Contexto, token: &str) -> Result<Option<SessaoAtual>> {
    let hash = auth::hash_token(token);
    let repo = ctx.repo();
    let Some(s) = repo.sessao(&hash)? else { return Ok(None) };
    let agora = auth::agora();
    if s.expira_em <= agora {
        repo.excluir_sessao(&hash)?;
        return Ok(None);
    }
    let Some(usuario) = repo.usuario(s.usuario_id)?.filter(|u| u.ativo) else {
        repo.excluir_sessao(&hash)?;
        return Ok(None);
    };
    // Renova só depois de 1 minuto, para não escrever no banco a cada requisição.
    if s.expira_em - agora < DURACAO_SESSAO - 60 {
        repo.renovar_sessao(&hash, agora + DURACAO_SESSAO)?;
    }
    Ok(Some(SessaoAtual { token_hash: hash, usuario, produtor_id: s.produtor_id }))
}

fn abrir_sessao(ctx: &Contexto, usuario_id: i64) -> Result<String> {
    let token = auth::gerar_token();
    let agora = auth::agora();
    let hash = auth::hash_token(&token);
    let repo = ctx.repo();
    repo.criar_sessao(&hash, usuario_id, agora, agora + DURACAO_SESSAO)?;
    repo.registrar_acesso(usuario_id)?;
    // Quem tem um único produtor já entra com ele selecionado.
    let admin = repo.usuario(usuario_id)?.is_some_and(|u| u.admin);
    let produtores = repo.listar_produtores(if admin { None } else { Some(usuario_id) })?;
    if let [unico] = produtores.as_slice()
        && let Some(id) = unico["id"].as_i64()
    {
        repo.selecionar_produtor(&hash, Some(id))?;
    }
    Ok(token)
}

/// Login com usuário e senha. Devolve o token da nova sessão.
pub fn entrar(ctx: &Contexto, login: &str, senha: &str) -> Result<String> {
    let login = login.trim().to_lowercase();
    if let Some(falta) = ctx.tentativas.bloqueado(&login) {
        let min = falta.as_secs().div_ceil(60);
        return Err(negocio(format!("Muitas tentativas erradas. Tente de novo em {min} minuto(s).")));
    }
    let usuario = ctx.repo().usuario_por_login(&login)?;
    let hash = usuario.as_ref().map(|u| u.senha_hash.clone()).unwrap_or_else(|| auth::hash_ficticio().to_string());
    let confere = auth::senha_confere(senha, &hash);
    match usuario {
        Some(u) if confere && u.ativo => {
            ctx.tentativas.acertou(&login);
            abrir_sessao(ctx, u.id)
        }
        Some(u) if confere && !u.ativo => Err(negocio("Usuário desativado. Fale com o administrador.")),
        _ => {
            ctx.tentativas.falhou(&login);
            Err(negocio("Usuário ou senha incorretos."))
        }
    }
}

/// Primeira execução: cria o administrador (só funciona com o banco sem usuários).
pub fn configurar_primeiro_admin(ctx: &Contexto, login: &str, nome: &str, senha: &str) -> Result<String> {
    let login = auth::validar_login(login)?;
    let nome = auth::validar_nome(nome)?;
    auth::validar_senha(senha, &login)?;
    let hash = auth::gerar_hash_senha(senha)?;
    let id = {
        let repo = ctx.repo(); // contar e criar sob a mesma trava: não há corrida
        if repo.contar_usuarios()? > 0 {
            return Err(negocio("O sistema já foi configurado. Entre com seu usuário."));
        }
        repo.criar_usuario(&login, &nome, &hash, true)?
    };
    abrir_sessao(ctx, id)
}

pub fn sair(ctx: &Contexto, s: &SessaoAtual) -> Result<()> {
    ctx.repo().excluir_sessao(&s.token_hash)
}

pub fn trocar_senha(ctx: &Contexto, s: &SessaoAtual, atual: &str, nova: &str) -> Result<()> {
    if !auth::senha_confere(atual, &s.usuario.senha_hash) {
        return Err(negocio("Senha atual incorreta."));
    }
    auth::validar_senha(nova, &s.usuario.login)?;
    let hash = auth::gerar_hash_senha(nova)?;
    let repo = ctx.repo();
    repo.definir_senha(s.usuario.id, &hash)?;
    repo.excluir_sessoes_do_usuario(s.usuario.id, Some(&s.token_hash)) // derruba as outras sessões
}

// ---------- administração de usuários ----------

pub fn criar_usuario(ctx: &Contexto, dados: &Value) -> Result<Value> {
    let texto = |c: &str| dados.get(c).and_then(Value::as_str).unwrap_or("").to_string();
    let login = auth::validar_login(&texto("login"))?;
    let nome = auth::validar_nome(&texto("nome"))?;
    let senha = texto("senha");
    auth::validar_senha(&senha, &login)?;
    let admin = dados.get("admin").and_then(Value::as_bool).unwrap_or(false);
    let hash = auth::gerar_hash_senha(&senha)?;
    let repo = ctx.repo();
    if repo.usuario_por_login(&login)?.is_some() {
        return Err(negocio(format!("O login {login} já está em uso.")));
    }
    let id = repo.criar_usuario(&login, &nome, &hash, admin)?;
    Ok(json!({ "id": id, "login": login, "nome": nome, "admin": admin, "ativo": true }))
}

/// Altera nome, perfil, situação e (opcionalmente) redefine a senha de um usuário.
pub fn alterar_usuario(ctx: &Contexto, admin: &SessaoAtual, id: i64, dados: &Value) -> Result<Value> {
    let Some(u) = ctx.repo().usuario(id)? else { return Err(negocio("Usuário não encontrado.")) };
    let nome = match dados.get("nome").and_then(Value::as_str) {
        Some(n) => auth::validar_nome(n)?,
        None => u.nome.clone(),
    };
    let e_admin = dados.get("admin").and_then(Value::as_bool).unwrap_or(u.admin);
    let ativo = dados.get("ativo").and_then(Value::as_bool).unwrap_or(u.ativo);
    if id == admin.usuario.id && (!e_admin || !ativo) {
        return Err(negocio("Você não pode tirar o seu próprio acesso de administrador nem se desativar."));
    }
    let nova_senha = dados.get("senha").and_then(Value::as_str).filter(|s| !s.is_empty());
    let hash = match nova_senha {
        Some(s) => {
            auth::validar_senha(s, &u.login)?;
            Some(auth::gerar_hash_senha(s)?)
        }
        None => None,
    };
    let repo = ctx.repo();
    if u.admin && u.ativo && (!e_admin || !ativo) && repo.contar_admins_ativos()? <= 1 {
        return Err(negocio("O sistema precisa de pelo menos um administrador ativo."));
    }
    repo.atualizar_usuario(id, &nome, e_admin, ativo)?;
    if let Some(h) = &hash {
        repo.definir_senha(id, h)?;
    }
    if !ativo || hash.is_some() {
        repo.excluir_sessoes_do_usuario(id, None)?; // desativado ou senha redefinida: sai de todos os lugares
    }
    Ok(json!({ "id": id, "login": u.login, "nome": nome, "admin": e_admin, "ativo": ativo }))
}

// ---------- produtores ----------

fn pode_acessar(u: &Usuario, p: &Produtor) -> bool {
    p.ativo && (u.admin || p.dono_id == Some(u.id))
}

fn produtor_acessivel(ctx: &Contexto, u: &Usuario, id: i64) -> Result<Produtor> {
    ctx.repo()
        .produtor(id)?
        .filter(|p| pode_acessar(u, p))
        .ok_or_else(|| negocio("Produtor não encontrado."))
}

pub fn listar_produtores(ctx: &Contexto, u: &Usuario) -> Result<Vec<Value>> {
    ctx.repo().listar_produtores(if u.admin { None } else { Some(u.id) })
}

/// Lê o .pfx enviado (base64) e confere senha, documento e validade.
fn ler_envio(dados: &Value) -> Result<(Vec<u8>, String, Certificado)> {
    let b64 = dados.get("pfx").and_then(Value::as_str).unwrap_or("");
    let senha = dados.get("senha").and_then(Value::as_str).unwrap_or("").to_string();
    if b64.is_empty() {
        return Err(negocio("Envie o arquivo do certificado (.pfx ou .p12)."));
    }
    let pfx = B64.decode(b64.trim()).map_err(|_| negocio("Arquivo do certificado inválido."))?;
    if pfx.len() > 64 * 1024 {
        return Err(negocio("Arquivo grande demais para um certificado A1."));
    }
    let cert = ler_pfx(&pfx, &senha).map_err(|e| negocio(e.to_string()))?;
    if cert.info.documento.is_none() {
        return Err(negocio("O certificado não traz CPF/CNPJ no padrão ICP-Brasil (NOME:DOCUMENTO)."));
    }
    if !cert.info.vigente {
        return Err(negocio(format!("Certificado fora da validade (válido até {}).", &cert.info.valido_ate[..10.min(cert.info.valido_ate.len())])));
    }
    Ok((pfx, senha, cert))
}

fn cifrar_certificado(ctx: &Contexto, pfx: &[u8], senha: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    Ok((ctx.cofre.cifrar(pfx)?, ctx.cofre.cifrar(senha.as_bytes())?))
}

fn resumo(cert: &Certificado, id: i64) -> Value {
    json!({ "id": id, "documento": cert.info.documento, "titular": cert.info.titular, "tipo": cert.info.tipo, "validoAte": cert.info.valido_ate })
}

/// Cadastra um produtor a partir do certificado. Se o mesmo documento tinha sido removido,
/// ele volta (com o histórico de notas) com o certificado novo.
pub fn adicionar_produtor(ctx: &Contexto, s: &SessaoAtual, dados: &Value) -> Result<Value> {
    let (pfx, senha, cert) = ler_envio(dados)?;
    let doc = cert.info.documento.clone().unwrap_or_default();
    let (pfx_c, senha_c) = cifrar_certificado(ctx, &pfx, &senha)?;
    let c = CertificadoProdutor {
        documento: &doc,
        titular: &cert.info.titular,
        tipo: &cert.info.tipo,
        emissor: &cert.info.emissor,
        valido_ate: &cert.info.valido_ate,
        pfx_cifrado: &pfx_c,
        senha_cifrada: &senha_c,
    };
    let id = {
        let repo = ctx.repo();
        match repo.produtor_por_documento(&doc)? {
            Some(p) if p.ativo && pode_acessar(&s.usuario, &p) => {
                return Err(negocio(format!("{} já está cadastrado. Para trocar o certificado, use \"Renovar certificado\".", p.titular)));
            }
            Some(p) if p.ativo => {
                return Err(negocio("Este produtor já foi cadastrado por outro usuário. Fale com o administrador."));
            }
            Some(p) => {
                repo.atualizar_certificado(p.id, &c, s.usuario.id)?;
                p.id
            }
            None => {
                let id = repo.inserir_produtor(&c, s.usuario.id)?;
                repo.adotar_dados_antigos(id, &doc)?;
                id
            }
        }
    };
    ctx.cache_certificados().insert(id, Arc::new(cert.clone()));
    Ok(resumo(&cert, id))
}

/// Troca o certificado de um produtor (renovação). Precisa ser do mesmo CPF/CNPJ.
pub fn renovar_certificado(ctx: &Contexto, s: &SessaoAtual, id: i64, dados: &Value) -> Result<Value> {
    let p = produtor_acessivel(ctx, &s.usuario, id)?;
    let (pfx, senha, cert) = ler_envio(dados)?;
    let doc = cert.info.documento.clone().unwrap_or_default();
    if doc != p.documento {
        return Err(negocio(format!(
            "Este certificado é de outro documento ({doc}). Para outro produtor, use \"Adicionar produtor\"."
        )));
    }
    let (pfx_c, senha_c) = cifrar_certificado(ctx, &pfx, &senha)?;
    let c = CertificadoProdutor {
        documento: &doc,
        titular: &cert.info.titular,
        tipo: &cert.info.tipo,
        emissor: &cert.info.emissor,
        valido_ate: &cert.info.valido_ate,
        pfx_cifrado: &pfx_c,
        senha_cifrada: &senha_c,
    };
    ctx.repo().atualizar_certificado(id, &c, p.dono_id.unwrap_or(s.usuario.id))?;
    ctx.cache_certificados().insert(id, Arc::new(cert.clone()));
    Ok(resumo(&cert, id))
}

pub fn remover_produtor(ctx: &Contexto, s: &SessaoAtual, id: i64) -> Result<()> {
    produtor_acessivel(ctx, &s.usuario, id)?;
    ctx.repo().desativar_produtor(id)?;
    ctx.cache_certificados().remove(&id);
    Ok(())
}

/// Dados padrão do emitente (nome, IE, regime, endereço). CPF/CNPJ vêm sempre do certificado.
pub fn definir_emitente(ctx: &Contexto, s: &SessaoAtual, id: i64, dados: &Value) -> Result<Value> {
    produtor_acessivel(ctx, &s.usuario, id)?;
    let Some(obj) = dados.as_object() else { return Err(negocio("Dados do emitente inválidos.")) };
    let mut limpo = serde_json::Map::new();
    for campo in ["xNome", "xFant", "IE", "CRT", "ender"] {
        if let Some(v) = obj.get(campo) {
            limpo.insert(campo.into(), v.clone());
        }
    }
    let texto = Value::Object(limpo).to_string();
    if texto.len() > 8 * 1024 {
        return Err(negocio("Dados do emitente grandes demais."));
    }
    ctx.repo().definir_emitente(id, &texto)?;
    Ok(serde_json::from_str(&texto)?)
}

pub fn selecionar_produtor(ctx: &Contexto, s: &SessaoAtual, id: Option<i64>) -> Result<()> {
    if let Some(id) = id {
        produtor_acessivel(ctx, &s.usuario, id)?;
        abrir_certificado(ctx, id)?; // confere já que o certificado guardado abre
    }
    ctx.repo().selecionar_produtor(&s.token_hash, id)
}

/// Certificado do produtor, decifrado do banco (com cache em memória).
pub fn abrir_certificado(ctx: &Contexto, id: i64) -> Result<Arc<Certificado>> {
    if let Some(c) = ctx.cache_certificados().get(&id) {
        return Ok(c.clone());
    }
    let p = ctx.repo().produtor(id)?.filter(|p| p.ativo).ok_or_else(|| negocio("Produtor não encontrado."))?;
    let (Some(pfx_c), Some(senha_c)) = (&p.pfx_cifrado, &p.senha_cifrada) else {
        return Err(negocio("O certificado deste produtor foi removido. Cadastre-o de novo."));
    };
    let pfx = ctx.cofre.decifrar(pfx_c)?;
    let senha = String::from_utf8(ctx.cofre.decifrar(senha_c)?)?;
    let cert = Arc::new(ler_pfx(&pfx, &senha)?);
    ctx.cache_certificados().insert(id, cert.clone());
    Ok(cert)
}

/// Produtor ativo da sessão, já com o certificado aberto.
pub fn ativo_da_sessao(ctx: &Contexto, s: &SessaoAtual) -> Result<Option<Ativo>> {
    let Some(id) = s.produtor_id else { return Ok(None) };
    let Some(p) = ctx.repo().produtor(id)? else { return Ok(None) };
    if !pode_acessar(&s.usuario, &p) {
        return Ok(None);
    }
    Ok(Some(Ativo { id, cert: abrir_certificado(ctx, id)? }))
}

/// Dados do produtor ativo para a interface (certificado + emitente padrão).
pub fn produtor_json(ctx: &Contexto, id: i64) -> Result<Value> {
    let p = ctx.repo().produtor(id)?.ok_or_else(|| negocio("Produtor não encontrado."))?;
    let emitente = p.emitente_json.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
    Ok(json!({ "id": p.id, "documento": p.documento, "titular": p.titular, "tipo": p.tipo, "validoAte": p.valido_ate, "emitente": emitente }))
}
