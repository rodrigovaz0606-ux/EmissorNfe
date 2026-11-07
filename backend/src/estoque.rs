//! Controle de estoque por produto (opcional em cada um).
//!
//! O saldo nunca é digitado direto: é a soma de movimentos, então sempre dá para ver de onde
//! veio. Movimentos manuais (entrada, saída, ajuste de contagem) e automáticos:
//! - nota autorizada de saída baixa o estoque, e a de entrada aumenta;
//! - cancelamento registrado devolve exatamente o que a nota movimentou.
//!
//! Complementar e ajuste (finNFe 2 e 3) não movem mercadoria e ficam de fora. A falta de saldo
//! só gera aviso na nota: quem decide emitir é a pessoa, e o estoque pode estar desatualizado.

use crate::db::{ESCALA_ESTOQUE, NovoMovimento, TipoMovimento};
use crate::emissao::{Ativo, Contexto, ErroNegocio};
use crate::produtos::ler_numero;
use anyhow::Result;
use serde_json::{Value, json};

/// O leiaute da NF-e aceita até 11 dígitos inteiros em qCom.
const QUANTIDADE_MAXIMA: f64 = 99_999_999_999.0;

fn negocio(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroNegocio(msg.into()))
}

fn numero(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => ler_numero(s),
        _ => None,
    }
}

/// Quantidade em décimos de milésimo: inteiros somam sem erro de arredondamento.
pub fn para_unidades(q: f64) -> i64 {
    (q * ESCALA_ESTOQUE).round() as i64
}

/// Quantidade digitada: número finito, sem passar do limite do leiaute.
pub fn ler_quantidade(texto: &str) -> Option<f64> {
    ler_numero(texto).filter(|q| *q >= 0.0 && *q <= QUANTIDADE_MAXIMA)
}

fn formatar(unidades: i64) -> String {
    let q = unidades as f64 / ESCALA_ESTOQUE;
    let t = format!("{q:.4}");
    t.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

/// Saldo e histórico de um produto.
pub fn consultar(ctx: &Contexto, ativo: &Ativo, produto_id: i64) -> Result<Value> {
    let repo = ctx.repo();
    let Some(p) = repo.produto_estoque(ativo.id, produto_id)? else { return Err(negocio("Produto não encontrado.")) };
    let movimentos = repo.movimentos_do_produto(ativo.id, produto_id, 100)?;
    Ok(json!({ "saldo": p.saldo as f64 / ESCALA_ESTOQUE, "controla": p.controla, "uCom": p.u_com, "minimo": p.minimo, "movimentos": movimentos }))
}

/// Entrada, saída ou ajuste de contagem (a quantidade do ajuste é o saldo contado).
pub fn movimentar_manual(ctx: &Contexto, ativo: &Ativo, usuario_id: i64, produto_id: i64, dados: &Value) -> Result<Value> {
    let tipo = dados.get("tipo").and_then(Value::as_str).unwrap_or("");
    let texto = match dados.get("quantidade") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let Some(quantidade) = ler_quantidade(&texto) else { return Err(negocio("Quantidade inválida.")) };
    let observacao = dados.get("observacao").and_then(Value::as_str).map(str::trim).filter(|o| !o.is_empty()).map(String::from);
    if observacao.as_ref().is_some_and(|o| o.chars().count() > 200) {
        return Err(negocio("Observação: até 200 caracteres."));
    }
    let q = para_unidades(quantidade);

    let repo = ctx.repo(); // leitura do saldo e gravação sob a mesma trava
    let Some(p) = repo.produto_estoque(ativo.id, produto_id)? else { return Err(negocio("Produto não encontrado.")) };
    if !p.controla {
        return Err(negocio("Este produto não está com o controle de estoque ligado."));
    }
    let (tipo_movimento, delta) = match tipo {
        "entrada" if q > 0 => (TipoMovimento::Entrada, q),
        "saida" if q > 0 => {
            if q > p.saldo {
                return Err(negocio(format!(
                    "Saída maior que o saldo ({} {}). Se a contagem real é outra, use o ajuste.",
                    formatar(p.saldo),
                    p.u_com
                )));
            }
            (TipoMovimento::Saida, -q)
        }
        "entrada" | "saida" => return Err(negocio("A quantidade precisa ser maior que zero.")),
        "ajuste" => {
            if q == p.saldo {
                return Err(negocio("O saldo já é esse: nada a ajustar."));
            }
            (TipoMovimento::Ajuste, q - p.saldo)
        }
        _ => return Err(negocio("Tipo de movimento inválido.")),
    };
    repo.registrar_movimentos(&[NovoMovimento {
        produtor_id: ativo.id,
        produto_id,
        tipo: tipo_movimento,
        quantidade: delta,
        nota_id: None,
        usuario_id: Some(usuario_id),
        observacao,
    }])?;
    Ok(json!({ "saldo": (p.saldo + delta) as f64 / ESCALA_ESTOQUE }))
}

/// Move o estoque pelos itens de uma nota recém-autorizada. Item sem cadastro ou sem controle
/// não conta; item com outra unidade fica de fora e é avisado, porque somar sacos com quilos
/// daria um saldo errado sem ninguém notar.
pub fn movimentar_por_nota(ctx: &Contexto, ativo: &Ativo, nota_id: i64, dados: &Value) -> Result<Value> {
    let fin = numero(dados.pointer("/ide/finNFe")).unwrap_or(1.0);
    if fin != 1.0 && fin != 4.0 {
        return Ok(json!({ "movimentados": 0, "ignorados": [] }));
    }
    let entrada = numero(dados.pointer("/ide/tpNF")) == Some(0.0);
    let repo = ctx.repo();
    let mut movimentos = vec![];
    let mut ignorados = vec![];
    for it in dados.get("itens").and_then(Value::as_array).into_iter().flatten() {
        let codigo = it.get("cProd").map(|c| c.as_str().map(String::from).unwrap_or_else(|| c.to_string())).unwrap_or_default();
        let Some(p) = repo.produto_estoque_por_codigo(ativo.id, codigo.trim())? else { continue };
        if !p.controla {
            continue;
        }
        let unidade = it.get("uCom").and_then(Value::as_str).unwrap_or("").trim().to_uppercase();
        if !unidade.is_empty() && unidade != p.u_com {
            ignorados.push(format!("{}: a nota usa {unidade} e o estoque usa {}. Ajuste o saldo manualmente.", p.c_prod, p.u_com));
            continue;
        }
        let Some(q) = numero(it.get("qCom")).filter(|q| *q > 0.0) else { continue };
        let q = para_unidades(q);
        movimentos.push(NovoMovimento {
            produtor_id: ativo.id,
            produto_id: p.id,
            tipo: TipoMovimento::Nota,
            quantidade: if entrada { q } else { -q },
            nota_id: Some(nota_id),
            usuario_id: None,
            observacao: None,
        });
    }
    repo.registrar_movimentos(&movimentos)?;
    Ok(json!({ "movimentados": movimentos.len(), "ignorados": ignorados }))
}

/// Cancelamento registrado: devolve ao estoque o que a nota movimentou (uma vez só).
pub fn estornar_nota(ctx: &Contexto, ativo: &Ativo, nota_id: i64) -> Result<Value> {
    let repo = ctx.repo();
    let originais = repo.movimentos_da_nota(ativo.id, nota_id)?;
    if originais.iter().any(|m| m.tipo == TipoMovimento::Estorno) {
        return Ok(json!({ "movimentados": 0, "ignorados": [] }));
    }
    let estornos: Vec<NovoMovimento> = originais
        .iter()
        .filter(|m| m.tipo == TipoMovimento::Nota)
        .map(|m| NovoMovimento {
            produtor_id: ativo.id,
            produto_id: m.produto_id,
            tipo: TipoMovimento::Estorno,
            quantidade: -m.quantidade,
            nota_id: Some(nota_id),
            usuario_id: None,
            observacao: None,
        })
        .collect();
    repo.registrar_movimentos(&estornos)?;
    Ok(json!({ "movimentados": estornos.len(), "ignorados": [] }))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn quantidades_sem_erro_de_arredondamento() {
        let total: i64 = (0..1000).map(|_| para_unidades(0.1)).sum();
        assert_eq!(total, para_unidades(100.0));
        assert_eq!(para_unidades(1.2345), 12345);
        assert_eq!(formatar(1_250_000), "125");
        assert_eq!(formatar(12_500), "1,25");
        assert_eq!(formatar(-5_000), "-0,5");
    }

    #[test]
    fn quantidade_digitada() {
        assert_eq!(ler_quantidade("1.234,5"), Some(1234.5));
        assert_eq!(ler_quantidade("-1"), None);
        assert_eq!(ler_quantidade("abc"), None);
        assert_eq!(ler_quantidade("1e30"), None);
    }
}
