// Cliente SOAP 1.2 com autenticação mútua (o certificado A1 é apresentado na conexão TLS).

import fs from 'node:fs';
import https from 'node:https';

export function envelopeSoap(servico, xmlDados) {
  return (
    '<?xml version="1.0" encoding="utf-8"?>' +
    '<soap12:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" ' +
    'xmlns:xsd="http://www.w3.org/2001/XMLSchema" ' +
    'xmlns:soap12="http://www.w3.org/2003/05/soap-envelope">' +
    '<soap12:Body>' +
    `<nfeDadosMsg xmlns="http://www.portalfiscal.inf.br/nfe/wsdl/${servico}">${xmlDados}</nfeDadosMsg>` +
    '</soap12:Body>' +
    '</soap12:Envelope>'
  );
}

export function enviarSoap({ url, servico, metodo, xmlDados, certificado, timeoutMs = 30000 }) {
  const caBundle = process.env.NFE_CA_BUNDLE ? fs.readFileSync(process.env.NFE_CA_BUNDLE, 'utf8') : undefined;
  const corpo = envelopeSoap(servico, xmlDados);
  const destino = new URL(url);

  const opcoes = {
    method: 'POST',
    hostname: destino.hostname,
    port: destino.port || 443,
    path: destino.pathname + destino.search,
    key: certificado.keyPem,
    cert: [certificado.certPem, ...certificado.cadeiaPem].join('\n'),
    // Os servidores da SEFAZ usam certificados da ICP-Brasil, que não estão na
    // lista padrão do Node. NFE_CA_BUNDLE (caminho de um .pem) informa a cadeia;
    // sem ela a verificação do servidor fica desligada (aceitável só em homologação).
    ca: caBundle,
    rejectUnauthorized: Boolean(caBundle),
    headers: {
      'Content-Type': `application/soap+xml; charset=utf-8; action="http://www.portalfiscal.inf.br/nfe/wsdl/${servico}/${metodo}"`,
      'Content-Length': Buffer.byteLength(corpo),
    },
    timeout: timeoutMs,
  };

  return new Promise((resolve, reject) => {
    const req = https.request(opcoes, (res) => {
      const partes = [];
      res.on('data', (d) => partes.push(d));
      res.on('end', () => {
        const texto = Buffer.concat(partes).toString('utf8');
        if (res.statusCode >= 400) {
          reject(new Error(`HTTP ${res.statusCode} de ${url}\n${texto.slice(0, 1000)}`));
        } else {
          resolve(texto);
        }
      });
    });
    req.on('timeout', () => req.destroy(new Error(`Tempo esgotado (${timeoutMs} ms) em ${url}`)));
    req.on('error', reject);
    req.end(corpo);
  });
}
