//! Leitor de PKCS#12 (.pfx) tolerante a BER.
//!
//! Muitos .pfx da ICP-Brasil (exportados pelo Windows ou por ferramentas das
//! certificadoras) usam codificação BER com comprimento indefinido e algoritmos
//! antigos (3DES, RC2-40). Este leitor aceita:
//!   - MAC HMAC-SHA1/SHA-256 com a KDF do PKCS#12 (confere a senha)
//!   - pbeWithSHAAnd3-KeyTripleDES-CBC, pbeWithSHAAnd40BitRC2-CBC, pbeWithSHAAnd128BitRC2-CBC
//!   - PBES2 (PBKDF2 com HMAC-SHA1/SHA-256) + AES-128/192/256-CBC ou 3DES-CBC
//!
//! Devolve as chaves privadas (PKCS#8 em DER) e os certificados (DER) encontrados.

use anyhow::{Context, Result, bail};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockCipher, BlockDecryptMut, BlockSizeUser, InnerIvInit, KeyInit};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// BER
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct No {
    pub tag: u8,
    pub filhos: Vec<No>,
    pub valor: Vec<u8>, // só em primitivos
}

impl No {
    fn construido(&self) -> bool {
        self.tag & 0x20 != 0
    }

    /// Conteúdo de um OCTET STRING (junta os pedaços da forma construída do BER).
    pub fn octetos(&self) -> Vec<u8> {
        if self.construido() { self.filhos.iter().flat_map(|f| f.octetos()).collect() } else { self.valor.clone() }
    }

    pub fn filho(&self, i: usize) -> Result<&No> {
        self.filhos.get(i).with_context(|| format!("estrutura ASN.1 inesperada (tag {:#x}, filho {i})", self.tag))
    }

    pub fn oid(&self) -> Result<String> {
        if self.tag != 0x06 || self.valor.is_empty() {
            bail!("OID esperado");
        }
        let v = &self.valor;
        let mut partes = vec![(v[0] / 40) as u64, (v[0] % 40) as u64];
        let mut atual = 0u64;
        for &b in &v[1..] {
            atual = (atual << 7) | (b & 0x7f) as u64;
            if b & 0x80 == 0 {
                partes.push(atual);
                atual = 0;
            }
        }
        Ok(partes.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("."))
    }

    pub fn inteiro(&self) -> Result<u64> {
        if self.tag != 0x02 || self.valor.len() > 8 {
            bail!("INTEGER esperado");
        }
        Ok(self.valor.iter().fold(0u64, |a, &b| (a << 8) | b as u64))
    }

    /// Reescreve em DER (comprimentos definidos, OCTET STRING primitivo).
    pub fn der(&self) -> Vec<u8> {
        let (tag, conteudo) = if self.tag == 0x24 {
            (0x04, self.octetos())
        } else if self.construido() {
            (self.tag, self.filhos.iter().flat_map(|f| f.der()).collect())
        } else {
            (self.tag, self.valor.clone())
        };
        let mut saida = vec![tag];
        let n = conteudo.len();
        if n < 0x80 {
            saida.push(n as u8);
        } else {
            let bytes: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|&b| b == 0).collect();
            saida.push(0x80 | bytes.len() as u8);
            saida.extend(bytes);
        }
        saida.extend(conteudo);
        saida
    }
}

pub fn ler_ber(dados: &[u8]) -> Result<No> {
    let mut pos = 0;
    let no = ler_no(dados, &mut pos, 0)?;
    Ok(no)
}

fn ler_no(d: &[u8], pos: &mut usize, profundidade: usize) -> Result<No> {
    if profundidade > 64 {
        bail!("ASN.1 aninhado demais");
    }
    let byte = |p: usize| d.get(p).copied().context("ASN.1 truncado");
    let tag = byte(*pos)?;
    if tag & 0x1f == 0x1f {
        bail!("tag ASN.1 de número alto não suportada");
    }
    *pos += 1;
    let b = byte(*pos)?;
    *pos += 1;
    let tamanho: Option<usize> = if b == 0x80 {
        None
    } else if b < 0x80 {
        Some(b as usize)
    } else {
        let n = (b & 0x7f) as usize;
        if n > 4 {
            bail!("comprimento ASN.1 grande demais");
        }
        let mut t = 0usize;
        for _ in 0..n {
            t = (t << 8) | byte(*pos)? as usize;
            *pos += 1;
        }
        Some(t)
    };
    let mut no = No { tag, filhos: vec![], valor: vec![] };
    if tag & 0x20 != 0 {
        match tamanho {
            Some(t) => {
                let fim = pos.checked_add(t).filter(|&f| f <= d.len()).context("ASN.1 truncado")?;
                while *pos < fim {
                    no.filhos.push(ler_no(d, pos, profundidade + 1)?);
                }
            }
            None => loop {
                if byte(*pos)? == 0 && byte(*pos + 1)? == 0 {
                    *pos += 2;
                    break;
                }
                no.filhos.push(ler_no(d, pos, profundidade + 1)?);
            },
        }
    } else {
        let t = tamanho.context("comprimento indefinido em valor primitivo")?;
        let fim = pos.checked_add(t).filter(|&f| f <= d.len()).context("ASN.1 truncado")?;
        no.valor = d[*pos..fim].to_vec();
        *pos = fim;
    }
    Ok(no)
}

// ---------------------------------------------------------------------------
// Derivação de chaves
// ---------------------------------------------------------------------------

/// Senha como BMPString (UTF-16BE) com terminador, como exige o PKCS#12.
fn senha_bmp(senha: &str) -> Vec<u8> {
    let mut v: Vec<u8> = senha.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    v.extend([0, 0]);
    v
}

/// KDF do PKCS#12 (RFC 7292, apêndice B.2). id: 1 = chave, 2 = IV, 3 = MAC.
fn kdf_pkcs12<D: Digest>(senha: &[u8], sal: &[u8], iteracoes: u64, id: u8, n: usize) -> Vec<u8> {
    let u = <D as Digest>::output_size();
    let v = 64;
    let repetir = |s: &[u8]| -> Vec<u8> {
        if s.is_empty() {
            return vec![];
        }
        let tam = v * s.len().div_ceil(v);
        s.iter().cycle().take(tam).copied().collect()
    };
    let mut i: Vec<u8> = repetir(sal);
    i.extend(repetir(senha));
    let d = vec![id; v];
    let mut saida = Vec::with_capacity(n + u);
    while saida.len() < n {
        let mut a = D::new().chain_update(&d).chain_update(&i).finalize().to_vec();
        for _ in 1..iteracoes {
            a = D::digest(&a).to_vec();
        }
        saida.extend_from_slice(&a);
        if saida.len() >= n {
            break;
        }
        // I_j = (I_j + B + 1) mod 2^(v*8), com B = A repetido até v bytes
        let b: Vec<u8> = a.iter().cycle().take(v).copied().collect();
        for bloco in i.chunks_mut(v) {
            let mut vai = 1u16;
            for k in (0..v).rev() {
                let soma = bloco[k] as u16 + b[k] as u16 + vai;
                bloco[k] = soma as u8;
                vai = soma >> 8;
            }
        }
    }
    saida.truncate(n);
    saida
}

fn decifrar_cbc<C>(cifra: C, iv: &[u8], dados: &[u8]) -> Result<Vec<u8>>
where
    C: BlockCipher + BlockDecryptMut + BlockSizeUser,
{
    let mut buf = dados.to_vec();
    let dec = cbc::Decryptor::<C>::inner_iv_slice_init(cifra, iv).map_err(|_| anyhow::anyhow!("IV inválido"))?;
    let texto = dec.decrypt_padded_mut::<Pkcs7>(&mut buf).map_err(|_| anyhow::anyhow!("falha ao decifrar (senha incorreta?)"))?;
    Ok(texto.to_vec())
}

const OID_PBE_SHA1_3DES: &str = "1.2.840.113549.1.12.1.3";
const OID_PBE_SHA1_RC2_128: &str = "1.2.840.113549.1.12.1.5";
const OID_PBE_SHA1_RC2_40: &str = "1.2.840.113549.1.12.1.6";
const OID_PBES2: &str = "1.2.840.113549.1.5.13";
const OID_PBKDF2: &str = "1.2.840.113549.1.5.12";
const OID_HMAC_SHA1: &str = "1.2.840.113549.2.7";
const OID_HMAC_SHA256: &str = "1.2.840.113549.2.9";
const OID_AES128_CBC: &str = "2.16.840.1.101.3.4.1.2";
const OID_AES192_CBC: &str = "2.16.840.1.101.3.4.1.22";
const OID_AES256_CBC: &str = "2.16.840.1.101.3.4.1.42";
const OID_DES_EDE3_CBC: &str = "1.2.840.113549.3.7";
const OID_SHA1: &str = "1.3.14.3.2.26";
const OID_SHA256: &str = "2.16.840.1.101.3.4.2.1";

/// Decifra conforme o AlgorithmIdentifier (PBES1 do PKCS#12 ou PBES2).
fn decifrar(algoritmo: &No, senha: &str, dados: &[u8]) -> Result<Vec<u8>> {
    let oid = algoritmo.filho(0)?.oid()?;
    let params = algoritmo.filho(1)?;
    match oid.as_str() {
        OID_PBE_SHA1_3DES | OID_PBE_SHA1_RC2_40 | OID_PBE_SHA1_RC2_128 => {
            let sal = params.filho(0)?.octetos();
            let it = params.filho(1)?.inteiro()?;
            let p = senha_bmp(senha);
            let iv = kdf_pkcs12::<Sha1>(&p, &sal, it, 2, 8);
            match oid.as_str() {
                OID_PBE_SHA1_3DES => {
                    let chave = kdf_pkcs12::<Sha1>(&p, &sal, it, 1, 24);
                    decifrar_cbc(des::TdesEde3::new_from_slice(&chave).unwrap(), &iv, dados)
                }
                _ => {
                    let bits = if oid == OID_PBE_SHA1_RC2_40 { 40 } else { 128 };
                    let chave = kdf_pkcs12::<Sha1>(&p, &sal, it, 1, bits / 8);
                    decifrar_cbc(rc2::Rc2::new_with_eff_key_len(&chave, bits), &iv, dados)
                }
            }
        }
        OID_PBES2 => {
            let kdf = params.filho(0)?;
            let esquema = params.filho(1)?;
            if kdf.filho(0)?.oid()? != OID_PBKDF2 {
                bail!("PBES2 com KDF não suportada");
            }
            let kp = kdf.filho(1)?;
            let sal = kp.filho(0)?.octetos();
            let it = kp.filho(1)?.inteiro()? as u32;
            // keyLength (opcional) e prf (opcional, padrão HMAC-SHA1)
            let prf = kp.filhos.iter().skip(2).find(|n| n.tag == 0x30).map(|n| n.filho(0).and_then(|o| o.oid())).transpose()?;
            let alg = esquema.filho(0)?.oid()?;
            let iv = esquema.filho(1)?.octetos();
            let tam = match alg.as_str() {
                OID_AES128_CBC => 16,
                OID_AES192_CBC | OID_DES_EDE3_CBC => 24,
                OID_AES256_CBC => 32,
                outro => bail!("Cifra PBES2 não suportada: {outro}"),
            };
            let mut chave = vec![0u8; tam];
            match prf.as_deref().unwrap_or(OID_HMAC_SHA1) {
                OID_HMAC_SHA1 => pbkdf2::pbkdf2_hmac::<Sha1>(senha.as_bytes(), &sal, it, &mut chave),
                OID_HMAC_SHA256 => pbkdf2::pbkdf2_hmac::<Sha256>(senha.as_bytes(), &sal, it, &mut chave),
                outro => bail!("PRF do PBKDF2 não suportada: {outro}"),
            }
            match alg.as_str() {
                OID_AES128_CBC => decifrar_cbc(aes::Aes128::new_from_slice(&chave).unwrap(), &iv, dados),
                OID_AES192_CBC => decifrar_cbc(aes::Aes192::new_from_slice(&chave).unwrap(), &iv, dados),
                OID_AES256_CBC => decifrar_cbc(aes::Aes256::new_from_slice(&chave).unwrap(), &iv, dados),
                _ => decifrar_cbc(des::TdesEde3::new_from_slice(&chave).unwrap(), &iv, dados),
            }
        }
        outro => bail!("Algoritmo de criptografia do .pfx não suportado: {outro}"),
    }
}

// ---------------------------------------------------------------------------
// PFX
// ---------------------------------------------------------------------------

const OID_DATA: &str = "1.2.840.113549.1.7.1";
const OID_ENCRYPTED_DATA: &str = "1.2.840.113549.1.7.6";
const OID_KEY_BAG: &str = "1.2.840.113549.1.12.10.1.1";
const OID_SHROUDED_KEY_BAG: &str = "1.2.840.113549.1.12.10.1.2";
const OID_CERT_BAG: &str = "1.2.840.113549.1.12.10.1.3";
const OID_SAFE_CONTENTS_BAG: &str = "1.2.840.113549.1.12.10.1.6";
const OID_X509: &str = "1.2.840.113549.1.9.22.1";

#[derive(Debug, Default)]
pub struct ConteudoPfx {
    /// Chaves privadas em PKCS#8 (DER).
    pub chaves: Vec<Vec<u8>>,
    /// Certificados X.509 (DER).
    pub certificados: Vec<Vec<u8>>,
}

/// Erro de senha: separado para a mensagem ao usuário ser clara.
#[derive(Debug)]
pub struct SenhaIncorreta;

impl std::fmt::Display for SenhaIncorreta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("senha incorreta")
    }
}

impl std::error::Error for SenhaIncorreta {}

fn conferir_mac(mac_data: &No, conteudo: &[u8], senha: &str) -> Result<()> {
    let digest_info = mac_data.filho(0)?;
    let alg = digest_info.filho(0)?.filho(0)?.oid()?;
    let esperado = digest_info.filho(1)?.octetos();
    let sal = mac_data.filho(1)?.octetos();
    let it = mac_data.filhos.get(2).map(|n| n.inteiro()).transpose()?.unwrap_or(1);
    let p = senha_bmp(senha);
    let ok = match alg.as_str() {
        OID_SHA1 => {
            let k = kdf_pkcs12::<Sha1>(&p, &sal, it, 3, 20);
            let mut m = <Hmac<Sha1> as Mac>::new_from_slice(&k).unwrap();
            m.update(conteudo);
            m.verify_slice(&esperado).is_ok()
        }
        OID_SHA256 => {
            let k = kdf_pkcs12::<Sha256>(&p, &sal, it, 3, 32);
            let mut m = <Hmac<Sha256> as Mac>::new_from_slice(&k).unwrap();
            m.update(conteudo);
            m.verify_slice(&esperado).is_ok()
        }
        outro => bail!("Algoritmo de MAC do .pfx não suportado: {outro}"),
    };
    if !ok {
        return Err(SenhaIncorreta.into());
    }
    Ok(())
}

fn ler_safe_contents(dados: &[u8], senha: &str, saida: &mut ConteudoPfx) -> Result<()> {
    let sc = ler_ber(dados)?;
    for bag in &sc.filhos {
        let tipo = bag.filho(0)?.oid()?;
        let valor = bag.filho(1)?.filho(0)?; // [0] EXPLICIT
        match tipo.as_str() {
            OID_KEY_BAG => saida.chaves.push(valor.der()),
            OID_SHROUDED_KEY_BAG => {
                let claro = decifrar(valor.filho(0)?, senha, &valor.filho(1)?.octetos())?;
                saida.chaves.push(ler_ber(&claro)?.der());
            }
            OID_CERT_BAG => {
                if valor.filho(0)?.oid()? == OID_X509 {
                    saida.certificados.push(valor.filho(1)?.filho(0)?.octetos());
                }
            }
            OID_SAFE_CONTENTS_BAG => ler_safe_contents(&valor.der(), senha, saida)?,
            _ => {} // CRL, segredos etc.: ignorados
        }
    }
    Ok(())
}

pub fn ler_pfx(pfx: &[u8], senha: &str) -> Result<ConteudoPfx> {
    let raiz = ler_ber(pfx).context(".pfx inválido")?;
    let auth_safe = raiz.filho(1)?;
    if auth_safe.filho(0)?.oid()? != OID_DATA {
        bail!(".pfx assinado com chave pública não é suportado");
    }
    let conteudo = auth_safe.filho(1)?.filho(0)?.octetos();
    // Sem MAC, a senha é conferida ao decifrar.
    if let Some(mac) = raiz.filhos.get(2) {
        conferir_mac(mac, &conteudo, senha)?;
    }
    let mut saida = ConteudoPfx::default();
    for ci in &ler_ber(&conteudo)?.filhos {
        let tipo = ci.filho(0)?.oid()?;
        let corpo = ci.filho(1)?.filho(0)?;
        match tipo.as_str() {
            OID_DATA => ler_safe_contents(&corpo.octetos(), senha, &mut saida)?,
            OID_ENCRYPTED_DATA => {
                let eci = corpo.filho(1)?;
                let cifrado = eci.filho(2)?; // [0] IMPLICIT OCTET STRING (primitivo ou construído)
                let bytes = if cifrado.construido() { cifrado.filhos.iter().flat_map(|f| f.octetos()).collect() } else { cifrado.valor.clone() };
                let claro = decifrar(eci.filho(1)?, senha, &bytes)?;
                ler_safe_contents(&claro, senha, &mut saida)?;
            }
            _ => {}
        }
    }
    Ok(saida)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ber_com_comprimento_indefinido_vira_der() {
        // SEQUENCE indefinida { OCTET STRING construída { "ab", "c" }, INTEGER 5 }
        let ber = [0x30, 0x80, 0x24, 0x80, 0x04, 0x02, b'a', b'b', 0x04, 0x01, b'c', 0x00, 0x00, 0x02, 0x01, 0x05, 0x00, 0x00];
        let no = ler_ber(&ber).unwrap();
        assert_eq!(no.filho(0).unwrap().octetos(), b"abc");
        assert_eq!(no.filho(1).unwrap().inteiro().unwrap(), 5);
        assert_eq!(no.der(), vec![0x30, 0x08, 0x04, 0x03, b'a', b'b', b'c', 0x02, 0x01, 0x05]);
    }

    #[test]
    fn kdf_pkcs12_vetor_conhecido() {
        // Vetor de teste clássico (BouncyCastle PKCS12Test): senha "smeg", sal 0A58CF64530D823F, 1 iteração.
        let sal = [0x0A, 0x58, 0xCF, 0x64, 0x53, 0x0D, 0x82, 0x3F];
        let chave = kdf_pkcs12::<Sha1>(&senha_bmp("smeg"), &sal, 1, 1, 24);
        assert_eq!(chave, [
            0x8A, 0xAA, 0xE6, 0x29, 0x7B, 0x6C, 0xB0, 0x46, 0x42, 0xAB, 0x5B, 0x07, 0x78, 0x51, 0x28, 0x4E, 0xB7, 0x12, 0x8F, 0x1A,
            0x2A, 0x7F, 0xBC, 0xA3
        ]);
        let iv = kdf_pkcs12::<Sha1>(&senha_bmp("smeg"), &sal, 1, 2, 8);
        assert_eq!(iv, [0x79, 0x99, 0x3D, 0xFE, 0x04, 0x8D, 0x3B, 0x76]);
    }

    /// Reescreve em BER "à moda Windows": construídos com comprimento indefinido e
    /// OCTET STRING longos em pedaços. O MAC continua válido (cobre só o conteúdo).
    fn para_ber(no: &No, saida: &mut Vec<u8>) {
        if no.construido() {
            saida.extend([no.tag, 0x80]);
            for f in &no.filhos {
                para_ber(f, saida);
            }
            saida.extend([0, 0]);
        } else if no.tag == 0x04 && no.valor.len() > 100 {
            saida.extend([0x24, 0x80]);
            for pedaco in no.valor.chunks(100) {
                saida.extend(No { tag: 0x04, filhos: vec![], valor: pedaco.to_vec() }.der());
            }
            saida.extend([0, 0]);
        } else {
            saida.extend(no.der());
        }
    }

    #[test]
    fn le_pfx_em_ber_com_comprimento_indefinido() {
        let der = include_bytes!("../../tests/fixtures/teste.pfx");
        let mut ber = vec![];
        para_ber(&ler_ber(der).unwrap(), &mut ber);
        assert_ne!(&ber[..], &der[..]);
        assert_eq!(ber[1], 0x80); // comprimento indefinido no topo
        let c = ler_pfx(&ber, "teste").unwrap();
        assert_eq!((c.chaves.len(), c.certificados.len()), (1, 1));
        assert_eq!(c.chaves, ler_pfx(der, "teste").unwrap().chaves);
    }

    #[test]
    fn le_fixtures() {
        for arq in [
            &include_bytes!("../../tests/fixtures/teste.pfx")[..],         // OpenSSL 3: PBES2/AES-256, MAC SHA-256
            &include_bytes!("../../tests/fixtures/teste-legado.pfx")[..],  // 3DES, MAC SHA-1
            &include_bytes!("../../tests/fixtures/teste-windows.pfx")[..], // exportado pelo Windows/.NET
        ] {
            let c = ler_pfx(arq, "teste").unwrap();
            assert_eq!((c.chaves.len(), c.certificados.len()), (1, 1));
            assert!(ler_pfx(arq, "outra").unwrap_err().downcast_ref::<SenhaIncorreta>().is_some());
        }
    }
}
