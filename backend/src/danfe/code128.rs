//! Código de barras Code-128, conjunto C (pares de dígitos), usado na chave de acesso do DANFE.

use anyhow::{Result, bail};

/// Larguras (barra, espaço, barra, espaço, barra, espaço) de cada símbolo, em módulos.
pub const PADROES: [&str; 107] = [
    "212222", "222122", "222221", "121223", "121322", "131222", "122213", "122312", "132212", "221213", "221312", "231212",
    "112232", "122132", "122231", "113222", "123122", "123221", "223211", "221132", "221231", "213212", "223112", "312131",
    "311222", "321122", "321221", "312212", "322112", "322211", "212123", "212321", "232121", "111323", "131123", "131321",
    "112313", "132113", "132311", "211313", "231113", "231311", "112133", "112331", "132131", "113123", "113321", "133121",
    "313121", "211331", "231131", "213113", "213311", "213131", "311123", "311321", "331121", "312113", "312311", "332111",
    "314111", "221411", "431111", "111224", "111422", "121124", "121421", "141122", "141221", "112214", "112412", "122114",
    "122411", "142112", "142211", "241211", "221114", "413111", "241112", "134111", "111242", "121142", "121241", "114212",
    "124112", "124211", "411212", "421112", "421211", "212141", "214121", "412121", "111143", "111341", "131141", "114113",
    "114311", "411113", "411311", "113141", "114131", "311141", "411131", "211412", "211214", "211232", "2331112",
];
const START_C: usize = 105;
const STOP: usize = 106;

/// Valores dos símbolos (start, dados, checksum, stop) para uma string de dígitos de tamanho par.
pub fn simbolos_c(digitos: &str) -> Result<Vec<usize>> {
    if digitos.is_empty() || !digitos.len().is_multiple_of(2) || !digitos.bytes().all(|b| b.is_ascii_digit()) {
        bail!("Code-128C exige quantidade par de dígitos.");
    }
    let dados: Vec<usize> = digitos.as_bytes().chunks(2).map(|p| ((p[0] - b'0') * 10 + (p[1] - b'0')) as usize).collect();
    let soma = dados.iter().enumerate().fold(START_C, |s, (i, v)| s + v * (i + 1));
    let mut s = vec![START_C];
    s.extend(&dados);
    s.extend([soma % 103, STOP]);
    Ok(s)
}

/// Barras (x, largura) em módulos e a largura total em módulos.
pub fn barras(digitos: &str) -> Result<(Vec<(u32, u32)>, u32)> {
    let mut lista = vec![];
    let mut x = 0;
    for s in simbolos_c(digitos)? {
        for (i, w) in PADROES[s].bytes().enumerate() {
            let largura = (w - b'0') as u32;
            if i % 2 == 0 {
                lista.push((x, largura));
            }
            x += largura;
        }
    }
    Ok((lista, x))
}

/// Decodificador (usado nos testes para conferir a codificação de ponta a ponta).
pub fn decodificar(lista: &[(u32, u32)], modulos: u32) -> Result<String> {
    let mut larguras = vec![];
    let mut pos = 0;
    for &(x, l) in lista {
        if x > pos {
            larguras.push(x - pos);
        } else if !larguras.is_empty() {
            bail!("barras sobrepostas");
        }
        larguras.push(l);
        pos = x + l;
    }
    if pos != modulos {
        bail!("largura total inconsistente");
    }
    let mut simbolos = vec![];
    let mut i = 0;
    while i < larguras.len() {
        let tam = if larguras.len() - i == 7 { 7 } else { 6 };
        let padrao: String = larguras[i..(i + tam).min(larguras.len())].iter().map(|w| w.to_string()).collect();
        let Some(v) = PADROES.iter().position(|p| *p == padrao) else { bail!("padrão desconhecido: {padrao}") };
        simbolos.push(v);
        i += tam;
    }
    if simbolos.len() < 3 || simbolos[0] != START_C || *simbolos.last().unwrap() != STOP {
        bail!("start/stop inválidos");
    }
    let dados = &simbolos[1..simbolos.len() - 2];
    let check = simbolos[simbolos.len() - 2];
    let soma = dados.iter().enumerate().fold(START_C, |s, (i, v)| s + v * (i + 1));
    if soma % 103 != check {
        bail!("checksum inválido");
    }
    Ok(dados.iter().map(|v| format!("{v:02}")).collect())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tabela_code128() {
        for (i, p) in PADROES.iter().enumerate() {
            let soma: u32 = p.bytes().map(|b| (b - b'0') as u32).sum();
            assert_eq!(soma, if i == 106 { 13 } else { 11 }, "símbolo {i}");
            let barras: u32 = p.bytes().step_by(2).map(|b| (b - b'0') as u32).sum();
            assert_eq!(barras % 2, 0, "paridade do símbolo {i}");
        }
        let unicos: std::collections::HashSet<_> = PADROES.iter().collect();
        assert_eq!(unicos.len(), 107);
        let bits = |p: &str| -> String {
            p.bytes().enumerate().map(|(i, w)| (if i % 2 == 0 { "1" } else { "0" }).repeat((w - b'0') as usize)).collect()
        };
        assert_eq!(bits(PADROES[0]), "11011001100");
        assert_eq!(bits(PADROES[105]), "11010011100");
        assert_eq!(bits(PADROES[106]), "1100011101011");
    }

    #[test]
    fn checksum_e_ida_e_volta() {
        // "1234" → Start C(105), 12, 34, checksum (105 + 12×1 + 34×2) % 103 = 82, Stop
        assert_eq!(simbolos_c("1234").unwrap(), vec![105, 12, 34, 82, 106]);
        let chave = "52261112345678000195550010000000011482017365";
        let (b, m) = barras(chave).unwrap();
        assert_eq!(m, 11 * (1 + 22 + 1) + 13);
        assert_eq!(decodificar(&b, m).unwrap(), chave);
        assert!(simbolos_c("123").is_err());
    }
}
