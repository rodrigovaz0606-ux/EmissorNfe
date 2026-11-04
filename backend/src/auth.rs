//! Login: senhas (Argon2id), tokens de sessão e bloqueio após tentativas erradas.
//!
//! - A senha nunca é guardada: só o hash Argon2id com sal aleatório.
//! - O cookie leva um token aleatório de 256 bits; o banco guarda só o SHA-256 dele.
//! - Depois de 5 senhas erradas seguidas para o mesmo login, ele fica bloqueado por 5 minutos.

use crate::emissao::ErroNegocio;
use anyhow::Result;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const NOME_COOKIE: &str = "emissor_sessao";
/// Duração da sessão sem uso (renovada a cada requisição).
pub const DURACAO_SESSAO: i64 = 8 * 60 * 60;
pub const MAX_TENTATIVAS: u32 = 5;
pub const BLOQUEIO: Duration = Duration::from_secs(5 * 60);
pub const SENHA_MIN: usize = 8;

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

pub fn gerar_hash_senha(senha: &str) -> Result<String> {
    let sal = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(senha.as_bytes(), &sal)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("falha ao gerar hash da senha: {e}"))
}

pub fn senha_confere(senha: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(senha.as_bytes(), &h).is_ok())
}

/// Hash de uma senha qualquer, para gastar o mesmo tempo quando o login não existe
/// (não revela quais logins existem pelo tempo de resposta).
pub fn hash_ficticio() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| gerar_hash_senha("senha-que-ninguem-usa").unwrap_or_default())
}

pub fn gerar_token() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|x| format!("{x:02x}")).collect()
}

pub fn agora() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Login: 3 a 40 caracteres, letras/números/ponto/hífen/sublinhado. Volta em minúsculas.
pub fn validar_login(login: &str) -> Result<String> {
    let l = login.trim().to_lowercase();
    let ok = (3..=40).contains(&l.chars().count()) && l.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if ok { Ok(l) } else { Err(negocio("Login deve ter de 3 a 40 caracteres: letras, números, ponto, hífen ou sublinhado.")) }
}

pub fn validar_nome(nome: &str) -> Result<String> {
    let n = nome.trim();
    if n.is_empty() || n.chars().count() > 80 {
        return Err(negocio("Informe o nome (até 80 caracteres)."));
    }
    Ok(n.to_string())
}

pub fn validar_senha(senha: &str, login: &str) -> Result<()> {
    let tam = senha.chars().count();
    if tam < SENHA_MIN {
        return Err(negocio(format!("A senha precisa ter pelo menos {SENHA_MIN} caracteres.")));
    }
    if tam > 200 {
        return Err(negocio("Senha longa demais."));
    }
    if senha.trim().to_lowercase() == login.trim().to_lowercase() {
        return Err(negocio("A senha não pode ser igual ao login."));
    }
    Ok(())
}

/// Contagem de senhas erradas por login (em memória).
#[derive(Default)]
pub struct Tentativas {
    mapa: Mutex<HashMap<String, (u32, Option<Instant>)>>,
}

impl Tentativas {
    /// Tempo restante de bloqueio, se o login estiver bloqueado.
    pub fn bloqueado(&self, login: &str) -> Option<Duration> {
        let mapa = self.mapa.lock().unwrap_or_else(|e| e.into_inner());
        let (_, ate) = mapa.get(login)?;
        ate.and_then(|t| t.checked_duration_since(Instant::now()))
    }

    pub fn falhou(&self, login: &str) {
        let mut mapa = self.mapa.lock().unwrap_or_else(|e| e.into_inner());
        if mapa.len() > 10_000 {
            mapa.clear(); // não deixa crescer sem limite com logins inventados
        }
        let e = mapa.entry(login.to_string()).or_insert((0, None));
        if e.1.is_some_and(|t| t <= Instant::now()) {
            *e = (0, None);
        }
        e.0 += 1;
        if e.0 >= MAX_TENTATIVAS {
            e.1 = Some(Instant::now() + BLOQUEIO);
        }
    }

    pub fn acertou(&self, login: &str) {
        self.mapa.lock().unwrap_or_else(|e| e.into_inner()).remove(login);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn senha_com_argon2() {
        let h = gerar_hash_senha("uma senha boa").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(senha_confere("uma senha boa", &h));
        assert!(!senha_confere("outra senha", &h));
        assert_ne!(gerar_hash_senha("uma senha boa").unwrap(), h, "sal aleatório");
        assert!(!senha_confere("x", "lixo"));
    }

    #[test]
    fn regras_de_login_e_senha() {
        assert_eq!(validar_login(" Rodrigo.Vaz ").unwrap(), "rodrigo.vaz");
        assert!(validar_login("ab").is_err());
        assert!(validar_login("com espaço").is_err());
        assert!(validar_senha("curta", "x").is_err());
        assert!(validar_senha("rodrigo.vaz", "rodrigo.vaz").is_err());
        assert!(validar_senha("senha-forte-123", "rodrigo").is_ok());
    }

    #[test]
    fn token_e_hash() {
        let t = gerar_token();
        assert_eq!(t.len(), 64);
        assert_ne!(gerar_token(), t);
        assert_eq!(hash_token(&t).len(), 64);
        assert_ne!(hash_token(&t), t);
    }

    #[test]
    fn bloqueio_apos_cinco_erros() {
        let t = Tentativas::default();
        for _ in 0..MAX_TENTATIVAS - 1 {
            t.falhou("a");
        }
        assert!(t.bloqueado("a").is_none());
        t.falhou("a");
        assert!(t.bloqueado("a").is_some());
        assert!(t.bloqueado("b").is_none());
        t.acertou("a");
        assert!(t.bloqueado("a").is_none());
    }
}
