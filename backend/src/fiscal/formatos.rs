//! Formatação de valores no padrão do leiaute da NF-e.
//!
//! Dinheiro é tratado em centavos inteiros (i64). As funções de arredondamento
//! reproduzem o comportamento do leiaute (meio para cima) de forma exata.

use anyhow::{Result, bail};
use chrono::{DateTime, FixedOffset, Utc};
use regex::Regex;
use std::sync::LazyLock;

static QUEBRAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\r\n\t]+").unwrap());
static ESPACOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());

/// Texto: a SEFAZ rejeita espaços no início/fim, espaços duplos e quebras de linha.
pub fn texto(valor: &str) -> String {
    let s = QUEBRAS.replace_all(valor, " ");
    ESPACOS.replace_all(&s, " ").trim().to_string()
}

pub fn escapar_xml(s: &str) -> String {
    let mut saida = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '"' => saida.push_str("&quot;"),
            '\'' => saida.push_str("&#39;"),
            _ => saida.push(c),
        }
    }
    saida
}

pub fn so_digitos(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Arredonda para o inteiro mais próximo; empate vai para cima (como no leiaute).
pub fn arredondar(x: f64) -> i64 {
    let piso = x.floor();
    (if x - piso >= 0.5 { piso + 1.0 } else { piso }) as i64
}

/// Converte texto ou número em centavos ("1,005" → 101). Vazio vale zero.
pub fn para_centavos_texto(valor: &str) -> Result<i64> {
    let v = valor.trim();
    if v.is_empty() {
        return Ok(0);
    }
    let n: f64 = v.replace(',', ".").parse().map_err(|_| anyhow::anyhow!("Valor inválido: \"{valor}\""))?;
    Ok(para_centavos(n))
}

pub fn para_centavos(n: f64) -> i64 {
    // ±1e-7 compensa o erro de ponto flutuante (ex.: 1.005 × 100 = 100.4999…).
    arredondar(n * 100.0 + if n >= 0.0 { 1e-7 } else { -1e-7 })
}

/// vProd = quantidade × unitário, arredondado em centavos.
pub fn multiplicar_em_centavos(qtd: f64, unit: f64) -> i64 {
    arredondar(qtd * unit * 100.0 + 1e-7)
}

/// Centavos → "1234.56".
pub fn centavos(c: i64) -> String {
    let sinal = if c < 0 { "-" } else { "" };
    let abs = c.unsigned_abs();
    format!("{sinal}{}.{:02}", abs / 100, abs % 100)
}

/// Número com casas fixas, arredondando o valor exato do número para cima nos empates.
pub fn fixo(valor: f64, casas: usize) -> String {
    if !valor.is_finite() {
        return valor.to_string();
    }
    // Expansão decimal exata do double (até 1100 casas cobre qualquer valor).
    let exato = format!("{:.1100}", valor.abs());
    let (int, dec) = exato.split_once('.').unwrap_or((&exato, ""));
    let mut digitos: Vec<u8> = int.bytes().chain(dec.bytes().take(casas)).map(|b| b - b'0').collect();
    let proximo = dec.as_bytes().get(casas).map(|b| b - b'0').unwrap_or(0);
    if proximo >= 5 {
        let mut i = digitos.len();
        loop {
            if i == 0 {
                digitos.insert(0, 1);
                break;
            }
            i -= 1;
            if digitos[i] == 9 {
                digitos[i] = 0;
            } else {
                digitos[i] += 1;
                break;
            }
        }
    }
    let texto: String = digitos.iter().map(|d| (d + b'0') as char).collect();
    let corte = texto.len() - casas;
    let (i, d) = texto.split_at(corte);
    let zero = digitos.iter().all(|&d| d == 0);
    let sinal = if valor < 0.0 && !zero { "-" } else { "" };
    if casas == 0 { format!("{sinal}{i}") } else { format!("{sinal}{i}.{d}") }
}

/// Valor unitário: até 10 casas, sem zeros sobrando, mas com pelo menos 2.
pub fn unitario(valor: f64) -> String {
    let s = fixo(valor, 10);
    let s = s.trim_end_matches('0');
    let (int, dec) = s.split_once('.').unwrap_or((s, ""));
    format!("{int}.{dec:0<2}")
}

/// Número como o JavaScript escreveria (132.5 → "132.5", 500 → "500").
pub fn numero_js(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

/// Data/hora AAAA-MM-DDThh:mm:ss-03:00 (GO = -03:00).
pub fn data_hora(agora: DateTime<Utc>) -> String {
    let fuso = FixedOffset::west_opt(3 * 3600).unwrap();
    agora.with_timezone(&fuso).format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

pub fn agora() -> String {
    data_hora(Utc::now())
}

pub fn exigir(cond: bool, msg: impl Into<String>) -> Result<()> {
    if !cond {
        bail!(msg.into());
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formatos() {
        assert_eq!(centavos(123456), "1234.56");
        assert_eq!(centavos(5), "0.05");
        assert_eq!(centavos(-5), "-0.05");
        assert_eq!(multiplicar_em_centavos(3.0, 0.335), 101);
        assert_eq!(para_centavos_texto("1,005").unwrap(), 101);
        assert_eq!(para_centavos_texto("").unwrap(), 0);
        assert!(para_centavos_texto("abc").is_err());
        assert_eq!(unitario(132.5), "132.50");
        assert_eq!(unitario(0.1234567), "0.1234567");
        assert_eq!(unitario(7.0), "7.00");
        assert_eq!(texto(" a \n  b "), "a b");
        assert_eq!(escapar_xml("A&B <\"x\">'"), "A&amp;B &lt;&quot;x&quot;&gt;&#39;");
        let dh = DateTime::parse_from_rfc3339("2026-11-03T13:00:00Z").unwrap().with_timezone(&Utc);
        assert_eq!(data_hora(dh), "2026-11-03T10:00:00-03:00");
    }

    #[test]
    fn fixo_arredonda_como_o_leiaute() {
        assert_eq!(fixo(500.0, 4), "500.0000");
        assert_eq!(fixo(1.65, 4), "1.6500");
        assert_eq!(fixo(0.125, 2), "0.13"); // empate exato vai para cima
        assert_eq!(fixo(2.5, 0), "3");
        assert_eq!(fixo(9.999, 2), "10.00");
        assert_eq!(fixo(1.005, 2), "1.00"); // 1.005 é 1.00499999… em binário
        assert_eq!(fixo(-1.5, 0), "-2");
        assert_eq!(fixo(0.0, 2), "0.00");
        assert_eq!(numero_js(132.5), "132.5");
        assert_eq!(numero_js(500.0), "500");
    }
}
