//! Cofre dos certificados: o .pfx e a senha de cada produtor ficam no banco cifrados
//! com AES-256-GCM. A chave-mestra fica FORA do banco:
//!
//! - `NFE_CHAVE_MESTRA` (64 dígitos hexadecimais), se definida; senão
//! - o arquivo `chave-mestra.key` na mesma pasta do banco, criado na primeira execução.
//!
//! Sem a chave-mestra, os certificados guardados não podem ser lidos (nem por quem
//! copiar o banco). Perder a chave obriga a cadastrar os certificados de novo.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{Context, Result, bail};
use rand::RngCore;
use std::path::Path;

pub struct Cofre {
    cifra: Aes256Gcm,
}

const TAM_NONCE: usize = 12;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn de_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

impl Cofre {
    pub fn com_chave(chave: &[u8; 32]) -> Self {
        Cofre { cifra: Aes256Gcm::new(chave.into()) }
    }

    /// Chave fixa, só para testes.
    pub fn de_teste() -> Self {
        Self::com_chave(&[7u8; 32])
    }

    fn de_texto(texto: &str, origem: &str) -> Result<Self> {
        match de_hex(texto) {
            Some(b) if b.len() == 32 => Ok(Self::com_chave(b.as_slice().try_into().expect("32 bytes"))),
            _ => bail!("Chave-mestra inválida em {origem}: precisa ter 64 dígitos hexadecimais."),
        }
    }

    /// Lê a chave de `NFE_CHAVE_MESTRA` ou do arquivo (criando-o se não existir).
    pub fn carregar(variavel: Option<String>, arquivo: &Path) -> Result<Self> {
        if let Some(v) = variavel {
            return Self::de_texto(&v, "NFE_CHAVE_MESTRA");
        }
        if arquivo.exists() {
            let texto = std::fs::read_to_string(arquivo).with_context(|| format!("lendo {}", arquivo.display()))?;
            return Self::de_texto(&texto, &arquivo.display().to_string());
        }
        let mut chave = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut chave);
        if let Some(pasta) = arquivo.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(pasta)?;
        }
        std::fs::write(arquivo, hex(&chave)).with_context(|| format!("criando {}", arquivo.display()))?;
        println!("Chave-mestra criada em {} — faça backup junto com o banco.", arquivo.display());
        Ok(Self::com_chave(&chave))
    }

    /// nonce (12 bytes) || texto cifrado com etiqueta.
    pub fn cifrar(&self, dados: &[u8]) -> Result<Vec<u8>> {
        let mut nonce = [0u8; TAM_NONCE];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let cifrado = self.cifra.encrypt(Nonce::from_slice(&nonce), dados).map_err(|_| anyhow::anyhow!("falha ao cifrar"))?;
        let mut saida = nonce.to_vec();
        saida.extend(cifrado);
        Ok(saida)
    }

    pub fn decifrar(&self, dados: &[u8]) -> Result<Vec<u8>> {
        if dados.len() < TAM_NONCE + 16 {
            bail!("dado cifrado truncado");
        }
        let (nonce, cifrado) = dados.split_at(TAM_NONCE);
        self.cifra
            .decrypt(Nonce::from_slice(nonce), cifrado)
            .map_err(|_| anyhow::anyhow!("Não foi possível abrir o certificado guardado: a chave-mestra não confere."))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta_e_chave_errada() {
        let c = Cofre::de_teste();
        let x = c.cifrar(b"segredo").unwrap();
        assert_ne!(&x[TAM_NONCE..], b"segredo");
        assert_eq!(c.decifrar(&x).unwrap(), b"segredo");
        assert_ne!(c.cifrar(b"segredo").unwrap(), x, "nonce aleatório");
        assert!(Cofre::com_chave(&[8u8; 32]).decifrar(&x).is_err());
        let mut adulterado = x.clone();
        *adulterado.last_mut().unwrap() ^= 1;
        assert!(c.decifrar(&adulterado).is_err());
    }

    #[test]
    fn arquivo_de_chave_e_criado_e_reutilizado() {
        let pasta = std::env::temp_dir().join(format!("cofre-{}", std::process::id()));
        let arq = pasta.join("chave-mestra.key");
        let _ = std::fs::remove_file(&arq);
        let x = Cofre::carregar(None, &arq).unwrap().cifrar(b"abc").unwrap();
        assert_eq!(std::fs::read_to_string(&arq).unwrap().len(), 64);
        assert_eq!(Cofre::carregar(None, &arq).unwrap().decifrar(&x).unwrap(), b"abc");
        assert!(Cofre::carregar(Some("123".into()), &arq).is_err());
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
