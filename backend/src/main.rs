//! Emissor de NF-e (homologação). Uso:
//!
//!   emissor-nfe [servidor]        API + interface em http://127.0.0.1:3335
//!   emissor-nfe status            testa certificado + conexão com a SEFAZ
//!   emissor-nfe cert              mostra os dados do certificado (sem rede)
//!   emissor-nfe gerar             gera, assina e valida o XML de exemplo em saida/
//!   emissor-nfe validar <xml>     valida um XML de NF-e no XSD
//!   emissor-nfe enviar            gera, assina, valida e ENVIA o exemplo (homologação)
//!   emissor-nfe consultar <chave> situação de uma NF-e na SEFAZ (somente leitura)
//!   emissor-nfe gerar-xml         lê os dados da nota (JSON) da entrada padrão e escreve o XML sem assinatura

use anyhow::{Context, Result, bail};
use emissor_nfe::exemplo::venda_soja;
use emissor_nfe::fiscal::assinatura::{assinar_nfe, verificar_assinatura};
use emissor_nfe::fiscal::certificado::{Certificado, ler_certificado};
use emissor_nfe::fiscal::config::{Config, carregar_config};
use emissor_nfe::fiscal::formatos::so_digitos;
use emissor_nfe::fiscal::nfe::gerar_xml_nfe;
use emissor_nfe::fiscal::servicos;
use emissor_nfe::fiscal::xsd::{Pacote, validar_xsd};
use emissor_nfe::raiz_do_projeto;
use std::path::PathBuf;
use std::process::ExitCode;

fn env(nome: &str) -> Option<String> {
    std::env::var(nome).ok().filter(|v| !v.is_empty())
}

fn configurar() -> Result<(Config, Certificado)> {
    let config = carregar_config(env)?; // falha se não for homologação
    let cert = ler_certificado(config.cert_path.as_deref(), config.cert_senha.as_deref())?;
    Ok((config, cert))
}

fn salvar(nome: &str, conteudo: &str) -> Result<PathBuf> {
    let pasta = raiz_do_projeto().join("saida");
    std::fs::create_dir_all(&pasta)?;
    let arq = pasta.join(nome);
    std::fs::write(&arq, conteudo)?;
    Ok(arq)
}

fn data_br(iso: &str) -> String {
    format!("{}/{}/{}", &iso[8..10], &iso[5..7], &iso[0..4])
}

fn status() -> Result<bool> {
    let (config, cert) = configurar()?;
    let i = &cert.info;
    println!("Certificado: {} ({}), válido até {}", i.titular, i.tipo, data_br(&i.valido_ate));
    if !i.vigente {
        eprintln!("ATENÇÃO: certificado fora da validade.");
    }
    println!("Consultando NFeStatusServico4 — {}, homologação...", config.uf);
    let r = servicos::consultar_status_servico(&config, &cert)?;
    println!("cStat {}: {}", r.cStat.unwrap_or_default(), r.xMotivo.unwrap_or_default());
    println!(
        "verAplic {} | recebido em {} | tempo médio {}s",
        r.verAplic.unwrap_or_default(),
        r.dhRecbto.unwrap_or_default(),
        r.tMed.unwrap_or_else(|| "-".into())
    );
    Ok(r.emOperacao)
}

fn cert() -> Result<bool> {
    let config = carregar_config(env)?;
    let c = ler_certificado(config.cert_path.as_deref(), config.cert_senha.as_deref())?;
    println!("{}", serde_json::to_string_pretty(&c.info)?);
    println!("certificados na cadeia: {}", c.cadeia_der.len());
    Ok(true)
}

/// Gera, assina e confere o cenário de exemplo com o documento do certificado.
fn exemplo_assinado(config: &Config, cert: &Certificado) -> Result<(String, String, String)> {
    let i = &cert.info;
    if i.tipo != "e-CPF" {
        bail!("O cenário de exemplo espera um e-CPF.");
    }
    let dados = venda_soja(i.documento.as_deref().unwrap_or(""), &i.titular);
    let g = gerar_xml_nfe(&dados, config)?;
    let assinado = assinar_nfe(&g.xml, cert)?;
    Ok((g.chave, g.v_nf, assinado))
}

fn gerar() -> Result<bool> {
    let (config, cert) = configurar()?;
    let (chave, v_nf, assinado) = exemplo_assinado(&config, &cert)?;
    let arq = salvar(&format!("{chave}-nfe.xml"), &format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{assinado}"))?;
    println!("Chave: {chave}\nValor: R$ {v_nf}\nArquivo: {}", arq.display());
    let a = verificar_assinatura(&assinado);
    println!("Assinatura: {}", if a.valida { "válida".into() } else { format!("INVÁLIDA ({})", a.motivo.unwrap_or_default()) });
    let x = validar_xsd(&assinado, Pacote::Nfe)?;
    println!("{}", if x.valido { "XSD: válido".into() } else { format!("XSD: {} erro(s)", x.erros.len()) });
    for e in &x.erros {
        println!("  - {e}");
    }
    Ok(x.valido && a.valida)
}

fn validar(arquivo: Option<String>) -> Result<bool> {
    let arquivo = arquivo.context("Uso: emissor-nfe validar <arquivo.xml>")?;
    let xml = std::fs::read_to_string(&arquivo).with_context(|| format!("Erro ao ler {arquivo}"))?;
    let r = validar_xsd(&xml, Pacote::Nfe)?;
    if r.valido {
        println!("XSD: válido");
    } else {
        println!("XSD: {} erro(s)", r.erros.len());
        for e in &r.erros {
            println!("  - {e}");
        }
    }
    Ok(r.valido)
}

fn enviar() -> Result<bool> {
    let (config, cert) = configurar()?;
    let (chave, v_nf, assinado) = exemplo_assinado(&config, &cert)?;
    if !verificar_assinatura(&assinado).valida {
        bail!("Assinatura não confere; envio cancelado.");
    }
    let x = validar_xsd(&assinado, Pacote::Nfe)?;
    if !x.valido {
        bail!("XML inválido no XSD; envio cancelado:\n  - {}", x.erros.join("\n  - "));
    }
    println!("Chave {chave} | R$ {v_nf} | assinatura e XSD OK");
    println!("Enviando para NFeAutorizacao4 — {}, HOMOLOGAÇÃO...", config.uf);
    let r = servicos::autorizar_nfe(&config, &cert, &assinado)?;
    salvar(&format!("{chave}-enviNFe.xml"), &r.enviado)?;
    let arq = salvar(&format!("{chave}-resposta.xml"), &r.resposta_soap)?;
    println!("\nLote: cStat {} — {}", r.lote.cStat.unwrap_or_default(), r.lote.xMotivo.unwrap_or_default());
    if let Some(rec) = &r.recibo {
        println!("Recibo: {rec}");
    }
    if let Some(p) = &r.protocolo {
        println!("Nota: cStat {} — {}", p.cStat.clone().unwrap_or_default(), p.xMotivo.clone().unwrap_or_default());
        if let (true, Some(xml)) = (p.autorizada, &p.xml) {
            let proc = salvar(&format!("{chave}-procNFe.xml"), &servicos::montar_nfe_proc(&assinado, xml))?;
            println!("AUTORIZADA. Protocolo {}\nXML autorizado: {}", p.nProt.clone().unwrap_or_default(), proc.display());
        }
    }
    println!("Resposta completa: {}", arq.display());
    Ok(true)
}

fn consultar(chave: Option<String>) -> Result<bool> {
    let chave = so_digitos(&chave.unwrap_or_default());
    let (config, cert) = configurar()?;
    let r = servicos::consultar_situacao(&config, &cert, &chave)?;
    println!("cStat {}: {}", r.cStat.unwrap_or_default(), r.xMotivo.unwrap_or_default());
    if let Some(p) = r.protocolo {
        println!("Protocolo {} ({} {})", p.nProt.unwrap_or_default(), p.cStat.unwrap_or_default(), p.xMotivo.unwrap_or_default());
    }
    for e in r.eventos {
        println!(
            "Evento {} {}: {} protocolo {}",
            e.tpEvento.unwrap_or_default(),
            e.xEvento.unwrap_or_default(),
            e.cStat.unwrap_or_default(),
            e.nProt.unwrap_or_default()
        );
    }
    Ok(true)
}

/// Dados do formulário (JSON na entrada padrão) → XML sem assinatura. Não usa certificado.
fn gerar_xml() -> Result<bool> {
    let mut entrada = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut entrada)?;
    let dados: emissor_nfe::fiscal::nfe::DadosNFe = serde_json::from_str(&entrada).context("JSON inválido na entrada")?;
    print!("{}", gerar_xml_nfe(&dados, &Config::teste())?.xml);
    Ok(true)
}

fn main() -> ExitCode {
    emissor_nfe::carregar_env(&raiz_do_projeto().join(".env"));
    let mut args = std::env::args().skip(1);
    let comando = args.next().unwrap_or_else(|| "servidor".into());
    let resultado = match comando.as_str() {
        "servidor" => emissor_nfe::servidor::iniciar(),
        "status" => status(),
        "cert" => cert(),
        "gerar" => gerar(),
        "validar" => validar(args.next()),
        "enviar" => enviar(),
        "consultar" => consultar(args.next()),
        "gerar-xml" => gerar_xml(),
        outro => Err(anyhow::anyhow!("Comando desconhecido: {outro}. Use: servidor, status, cert, gerar, validar, enviar, consultar, gerar-xml")),
    };
    match resultado {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("Erro: {e:#}");
            ExitCode::FAILURE
        }
    }
}
