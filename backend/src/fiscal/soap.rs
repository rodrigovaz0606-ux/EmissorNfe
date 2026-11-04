//! Cliente SOAP 1.2 com autenticação mútua: o certificado A1 é apresentado na conexão TLS.
//!
//! HTTP/1.1 mínimo sobre native-tls (Schannel no Windows): POST com Content-Length,
//! leitura da resposta com Content-Length, chunked ou até o fim da conexão.

use super::certificado::Certificado;
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// Falha de comunicação com a SEFAZ (rede, TLS, HTTP ou resposta inesperada).
/// A API devolve 502 para este erro; os demais são 400.
#[derive(Debug)]
pub struct ErroRede(pub String);

impl fmt::Display for ErroRede {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ErroRede {}

pub fn erro_rede(msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(ErroRede(msg.into()))
}

pub fn envelope_soap(servico: &str, xml_dados: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <soap12:Envelope xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" \
         xmlns:soap12=\"http://www.w3.org/2003/05/soap-envelope\">\
         <soap12:Body>\
         <nfeDadosMsg xmlns=\"http://www.portalfiscal.inf.br/nfe/wsdl/{servico}\">{xml_dados}</nfeDadosMsg>\
         </soap12:Body>\
         </soap12:Envelope>"
    )
}

pub struct Requisicao<'a> {
    pub url: &'a str,
    pub servico: &'a str,
    pub metodo: &'a str,
    pub xml_dados: &'a str,
    pub certificado: &'a Certificado,
    pub tempo_limite: Duration,
}

fn conector(cert: &Certificado) -> anyhow::Result<native_tls::TlsConnector> {
    let identidade = native_tls::Identity::from_pkcs12(&cert.pfx, &cert.senha)
        .map_err(|e| erro_rede(format!("O sistema não aceitou o certificado para a conexão TLS: {e}")))?;
    let mut b = native_tls::TlsConnector::builder();
    b.identity(identidade).min_protocol_version(Some(native_tls::Protocol::Tlsv12));
    // Os servidores da SEFAZ usam certificados da ICP-Brasil. NFE_CA_BUNDLE (um .pem com a
    // cadeia) liga a verificação do servidor; sem ela fica desligada (aceitável só em homologação).
    match std::env::var("NFE_CA_BUNDLE").ok().filter(|c| !c.is_empty()) {
        Some(caminho) => {
            let pem = std::fs::read_to_string(&caminho).map_err(|e| erro_rede(format!("NFE_CA_BUNDLE ({caminho}): {e}")))?;
            for bloco in pem.split("-----END CERTIFICATE-----").filter(|b| b.contains("-----BEGIN CERTIFICATE-----")) {
                let c = native_tls::Certificate::from_pem(format!("{bloco}-----END CERTIFICATE-----\n").as_bytes())
                    .map_err(|e| erro_rede(format!("Certificado inválido em NFE_CA_BUNDLE: {e}")))?;
                b.add_root_certificate(c);
            }
        }
        None => {
            b.danger_accept_invalid_certs(true);
        }
    }
    b.build().map_err(|e| erro_rede(format!("Falha ao preparar TLS: {e}")))
}

fn separar_url(url: &str) -> anyhow::Result<(String, u16, String)> {
    let resto = url.strip_prefix("https://").ok_or_else(|| erro_rede(format!("URL precisa ser https: {url}")))?;
    let (host_porta, caminho) = match resto.find('/') {
        Some(i) => (&resto[..i], &resto[i..]),
        None => (resto, "/"),
    };
    let (host, porta) = match host_porta.split_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| erro_rede(format!("Porta inválida em {url}")))?),
        None => (host_porta, 443),
    };
    Ok((host.to_string(), porta, caminho.to_string()))
}

pub fn enviar_soap(r: &Requisicao) -> anyhow::Result<String> {
    let corpo = envelope_soap(r.servico, r.xml_dados);
    let (host, porta, caminho) = separar_url(r.url)?;
    let tempo = |e: std::io::Error| -> anyhow::Error {
        if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock) {
            erro_rede(format!("Tempo esgotado ({} ms) em {}", r.tempo_limite.as_millis(), r.url))
        } else {
            erro_rede(format!("Falha de comunicação com {}: {e}", r.url))
        }
    };

    let endereco = (host.as_str(), porta)
        .to_socket_addrs()
        .map_err(|e| erro_rede(format!("Não foi possível resolver {host}: {e}")))?
        .next()
        .ok_or_else(|| erro_rede(format!("Sem endereço para {host}")))?;
    let tcp = TcpStream::connect_timeout(&endereco, r.tempo_limite).map_err(tempo)?;
    tcp.set_read_timeout(Some(r.tempo_limite)).ok();
    tcp.set_write_timeout(Some(r.tempo_limite)).ok();
    let mut tls = conector(r.certificado)?
        .connect(&host, tcp)
        .map_err(|e| erro_rede(format!("Falha no TLS com {host}: {e}")))?;

    let cabecalho = format!(
        "POST {caminho} HTTP/1.1\r\nHost: {host}\r\n\
         Content-Type: application/soap+xml; charset=utf-8; action=\"http://www.portalfiscal.inf.br/nfe/wsdl/{}/{}\"\r\n\
         Content-Length: {}\r\nConnection: close\r\nUser-Agent: EmissorNfe\r\n\r\n",
        r.servico,
        r.metodo,
        corpo.len()
    );
    tls.write_all(cabecalho.as_bytes()).map_err(tempo)?;
    tls.write_all(corpo.as_bytes()).map_err(tempo)?;
    tls.flush().map_err(tempo)?;

    let (status, texto) = ler_resposta(&mut tls).map_err(tempo)?;
    if status >= 400 {
        let inicio: String = texto.chars().take(1000).collect();
        return Err(erro_rede(format!("HTTP {status} de {}\n{inicio}", r.url)));
    }
    Ok(texto)
}

/// Lê uma resposta HTTP/1.1 completa: (status, corpo como texto).
pub fn ler_resposta(fluxo: &mut impl Read) -> std::io::Result<(u16, String)> {
    let mut buf = Vec::new();
    let mut bloco = [0u8; 16 * 1024];
    let invalida = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());

    // Cabeçalho
    let fim_cab = loop {
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p;
        }
        let n = fluxo.read(&mut bloco)?;
        if n == 0 {
            return Err(invalida("conexão fechada antes do cabeçalho HTTP"));
        }
        buf.extend_from_slice(&bloco[..n]);
    };
    let cab = String::from_utf8_lossy(&buf[..fim_cab]).to_string();
    let mut corpo = buf[fim_cab + 4..].to_vec();
    let status: u16 = cab.split_whitespace().nth(1).and_then(|s| s.parse().ok()).ok_or_else(|| invalida("status HTTP inválido"))?;
    let valor = |nome: &str| {
        cab.lines().skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(nome).then(|| v.trim().to_ascii_lowercase())
        })
    };

    let mut ler_mais = |corpo: &mut Vec<u8>| -> std::io::Result<bool> {
        let n = fluxo.read(&mut bloco)?;
        corpo.extend_from_slice(&bloco[..n]);
        Ok(n > 0)
    };

    if valor("transfer-encoding").is_some_and(|v| v.contains("chunked")) {
        let mut saida = Vec::new();
        let mut pos = 0;
        loop {
            // tamanho do pedaço em hexadecimal
            let linha_fim = loop {
                if let Some(p) = corpo[pos..].windows(2).position(|w| w == b"\r\n") {
                    break pos + p;
                }
                if !ler_mais(&mut corpo)? {
                    return Err(invalida("resposta chunked incompleta"));
                }
            };
            let linha = String::from_utf8_lossy(&corpo[pos..linha_fim]).to_string();
            let tam = usize::from_str_radix(linha.split(';').next().unwrap_or("").trim(), 16).map_err(|_| invalida("tamanho de chunk inválido"))?;
            pos = linha_fim + 2;
            if tam == 0 {
                break;
            }
            while corpo.len() < pos + tam + 2 {
                if !ler_mais(&mut corpo)? {
                    return Err(invalida("resposta chunked incompleta"));
                }
            }
            saida.extend_from_slice(&corpo[pos..pos + tam]);
            pos += tam + 2;
        }
        corpo = saida;
    } else if let Some(tam) = valor("content-length").and_then(|v| v.parse::<usize>().ok()) {
        while corpo.len() < tam {
            if !ler_mais(&mut corpo)? {
                return Err(invalida("resposta menor que o Content-Length"));
            }
        }
        corpo.truncate(tam);
    } else {
        while ler_mais(&mut corpo)? {}
    }
    Ok((status, String::from_utf8_lossy(&corpo).to_string()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_respostas_http() {
        let mut r = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nabcdeXX".as_bytes();
        assert_eq!(ler_resposta(&mut r).unwrap(), (200, "abcde".into()));
        let mut r = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n2;x=1\r\nde\r\n0\r\n\r\n".as_bytes();
        assert_eq!(ler_resposta(&mut r).unwrap(), (200, "abcde".into()));
        let mut r = "HTTP/1.1 500 Erro\r\nConnection: close\r\n\r\nfalhou".as_bytes();
        assert_eq!(ler_resposta(&mut r).unwrap(), (500, "falhou".into()));
        assert_eq!(separar_url("https://h.gov.br:8443/a/b").unwrap(), ("h.gov.br".into(), 8443, "/a/b".into()));
    }

    #[test]
    fn envelope() {
        let e = envelope_soap("NFeStatusServico4", "<x/>");
        assert!(e.contains("<nfeDadosMsg xmlns=\"http://www.portalfiscal.inf.br/nfe/wsdl/NFeStatusServico4\"><x/></nfeDadosMsg>"));
    }
}
