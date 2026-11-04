//! Validação de CPF e CNPJ (dígitos verificadores). A SEFAZ rejeita documentos inválidos.

use super::formatos::so_digitos;

fn todos_iguais(d: &[u32]) -> bool {
    d.iter().all(|&x| x == d[0])
}

pub fn cpf_valido(valor: &str) -> bool {
    let c: Vec<u32> = so_digitos(valor).chars().map(|c| c.to_digit(10).unwrap()).collect();
    if c.len() != 11 || todos_iguais(&c) {
        return false;
    }
    let dv = |n: usize| {
        let soma: u32 = (0..n).map(|i| c[i] * (n as u32 + 1 - i as u32)).sum();
        let r = (soma * 10) % 11;
        if r == 10 { 0 } else { r }
    };
    dv(9) == c[9] && dv(10) == c[10]
}

pub fn cnpj_valido(valor: &str) -> bool {
    let c: Vec<u32> = so_digitos(valor).chars().map(|c| c.to_digit(10).unwrap()).collect();
    if c.len() != 14 || todos_iguais(&c) {
        return false;
    }
    let dv = |n: usize| {
        let pesos: &[u32] = if n == 12 {
            &[5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]
        } else {
            &[6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]
        };
        let soma: u32 = pesos.iter().enumerate().map(|(i, p)| c[i] * p).sum();
        let r = soma % 11;
        if r < 2 { 0 } else { 11 - r }
    };
    dv(12) == c[12] && dv(13) == c[13]
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn documentos() {
        assert!(cpf_valido("529.982.247-25"));
        assert!(!cpf_valido("529.982.247-24"));
        assert!(!cpf_valido("111.111.111-11"));
        assert!(cnpj_valido("12.345.678/0001-95"));
        assert!(!cnpj_valido("12.345.678/0001-96"));
        assert!(!cnpj_valido("123"));
    }
}
