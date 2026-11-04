//! Chave de acesso da NF-e (44 dígitos), conforme o Manual de Orientação do Contribuinte:
//!
//!   cUF(2) AAMM(4) CNPJ/CPF(14) mod(2) serie(3) nNF(9) tpEmis(1) cNF(8) cDV(1)
//!
//! Para emitente pessoa física, o CPF ocupa as 14 posições completado com zeros à esquerda.

use super::formatos::so_digitos;
use anyhow::{Result, bail};
use rand::Rng;
use serde::Serialize;

fn campo(nome: &str, valor: &str, tamanho: usize) -> Result<String> {
    let d = so_digitos(valor);
    if d.is_empty() || d.len() > tamanho {
        bail!("{nome} inválido: \"{valor}\" (até {tamanho} dígitos)");
    }
    Ok(format!("{d:0>tamanho$}"))
}

/// Módulo 11 com pesos 2..9 aplicados da direita para a esquerda.
/// Resto 0 ou 1 → DV 0; caso contrário DV = 11 - resto.
pub fn calcular_dv(chave43: &str) -> Result<u32> {
    if chave43.len() != 43 || !chave43.bytes().all(|b| b.is_ascii_digit()) {
        bail!("A base da chave precisa ter 43 dígitos.");
    }
    let mut soma = 0;
    let mut peso = 2;
    for b in chave43.bytes().rev() {
        soma += (b - b'0') as u32 * peso;
        peso = if peso == 9 { 2 } else { peso + 1 };
    }
    let resto = soma % 11;
    Ok(if resto < 2 { 0 } else { 11 - resto })
}

/// Código numérico aleatório de 8 dígitos. A SEFAZ rejeita cNF igual ao nNF
/// e sequências óbvias, então sorteamos até obter um valor aceitável.
pub fn gerar_cnf(n_nf: &str) -> String {
    const SEQUENCIAS: [&str; 11] = [
        "00000000", "12345678", "23456789", "34567890", "45678901", "56789012", "67890123", "78901234", "89012345",
        "90123456", "01234567",
    ];
    let d = so_digitos(n_nf);
    let nnf8 = format!("{d:0>8}");
    let nnf8 = &nnf8[nnf8.len() - 8..];
    let mut rng = rand::thread_rng();
    loop {
        let c = format!("{:08}", rng.gen_range(0..100_000_000u32));
        let repetido = c.bytes().all(|b| b == c.as_bytes()[0]);
        if !SEQUENCIAS.contains(&c.as_str()) && !repetido && c != nnf8 {
            return c;
        }
    }
}

/// AAMM da emissão, usando o ano/mês escritos no texto (sem converter fuso),
/// para a chave bater com o dhEmi do XML.
fn ano_mes(dh_emi: &str) -> Result<String> {
    let b = dh_emi.as_bytes();
    let ok = b.len() >= 7 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && b[5..7].iter().all(u8::is_ascii_digit);
    if !ok {
        bail!("dhEmi inválida: \"{dh_emi}\"");
    }
    Ok(format!("{}{}", &dh_emi[2..4], &dh_emi[5..7]))
}

pub struct ParametrosChave<'a> {
    pub c_uf: &'a str,
    pub dh_emi: &'a str,
    pub documento: &'a str,
    pub serie: &'a str,
    pub n_nf: &'a str,
    /// Se omitido, é sorteado.
    pub c_nf: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct Chave {
    pub chave: String,
    pub c_nf: String,
    pub c_dv: u32,
}

pub fn montar_chave(p: &ParametrosChave) -> Result<Chave> {
    let aamm = ano_mes(p.dh_emi)?;
    let doc = so_digitos(p.documento);
    if doc.len() != 11 && doc.len() != 14 {
        bail!("Documento do emitente deve ser CPF ou CNPJ: \"{}\"", p.documento);
    }
    let n: u64 = so_digitos(p.n_nf).parse().unwrap_or(0);
    if !(1..=999_999_999).contains(&n) {
        bail!("nNF fora da faixa 1–999999999: \"{}\"", p.n_nf);
    }
    let codigo = match p.c_nf {
        Some(c) if !c.is_empty() => campo("cNF", c, 8)?,
        _ => gerar_cnf(p.n_nf),
    };
    let base = format!(
        "{}{aamm}{doc:0>14}{}{}{}{}{codigo}",
        campo("cUF", p.c_uf, 2)?,
        campo("mod", "55", 2)?,
        campo("serie", p.serie, 3)?,
        campo("nNF", p.n_nf, 9)?,
        campo("tpEmis", "1", 1)?,
    );
    let c_dv = calcular_dv(&base)?;
    Ok(Chave { chave: format!("{base}{c_dv}"), c_nf: codigo, c_dv })
}

pub fn validar_chave(chave: &str) -> bool {
    let c = so_digitos(chave);
    c.len() == 44 && calcular_dv(&c[..43]).ok() == c[43..].parse().ok()
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct ChaveDecomposta {
    pub cUF: String,
    pub aamm: String,
    pub documento: String,
    pub r#mod: String,
    pub serie: String,
    pub nNF: String,
    pub tpEmis: String,
    pub cNF: String,
    pub cDV: String,
    pub valida: bool,
}

/// Separa os campos de uma chave (útil para conferência e para o DANFE).
pub fn decompor_chave(chave: &str) -> Result<ChaveDecomposta> {
    let c = so_digitos(chave);
    if c.len() != 44 {
        bail!("Chave precisa ter 44 dígitos.");
    }
    Ok(ChaveDecomposta {
        cUF: c[0..2].into(),
        aamm: c[2..6].into(),
        documento: c[6..20].into(),
        r#mod: c[20..22].into(),
        serie: c[22..25].into(),
        nNF: c[25..34].into(),
        tpEmis: c[34..35].into(),
        cNF: c[35..43].into(),
        cDV: c[43..].into(),
        valida: validar_chave(&c),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn base<'a>(documento: &'a str, n_nf: &'a str, c_nf: &'a str) -> ParametrosChave<'a> {
        ParametrosChave { c_uf: "52", dh_emi: "2026-11-03T10:00:00-03:00", documento, serie: "1", n_nf, c_nf: Some(c_nf) }
    }

    #[test]
    fn dv_confere_com_calculo_independente() {
        assert_eq!(calcular_dv("5226111234567800019555001000000001148201736").unwrap(), 5);
        assert_eq!(calcular_dv("5226110001234567890955001000000123190817263").unwrap(), 2);
    }

    #[test]
    fn monta_chave_com_cnpj() {
        let c = montar_chave(&base("12.345.678/0001-95", "1", "48201736")).unwrap();
        assert_eq!(c.chave, "52261112345678000195550010000000011482017365");
        assert_eq!(c.c_dv, 5);
    }

    #[test]
    fn cpf_completado_com_zeros() {
        let c = montar_chave(&base("123.456.789-09", "123", "90817263")).unwrap();
        assert_eq!(c.chave, "52261100012345678909550010000001231908172632");
        assert_eq!(decompor_chave(&c.chave).unwrap().documento, "00012345678909");
    }

    #[test]
    fn aamm_vem_do_texto_sem_fuso() {
        let mut p = base("12345678000195", "1", "48201736");
        p.dh_emi = "2026-11-01T00:30:00-03:00";
        assert_eq!(&montar_chave(&p).unwrap().chave[2..6], "2611");
    }

    #[test]
    fn valida_chave() {
        assert!(validar_chave("52261112345678000195550010000000011482017365"));
        assert!(!validar_chave("52261112345678000195550010000000011482017364"));
        assert!(!validar_chave("123"));
    }

    #[test]
    fn cnf_sorteado() {
        for _ in 0..2000 {
            let c = gerar_cnf("12345");
            assert_eq!(c.len(), 8);
            assert_ne!(c, "00012345");
            assert_ne!(c, "12345678");
        }
    }

    #[test]
    fn rejeita_entradas_invalidas() {
        let erro = |p: ParametrosChave| montar_chave(&p).unwrap_err().to_string();
        assert!(erro(base("123", "1", "")).contains("CPF ou CNPJ"));
        assert!(erro(base("12345678000195", "0", "")).contains("nNF"));
        let mut p = base("12345678000195", "1", "");
        p.serie = "1000";
        assert!(erro(p).contains("serie"));
        let mut p = base("12345678000195", "1", "");
        p.dh_emi = "ontem";
        assert!(erro(p).contains("dhEmi"));
    }
}
