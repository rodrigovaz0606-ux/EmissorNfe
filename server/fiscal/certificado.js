// Leitura do certificado A1 (.pfx) com node-forge.
// Converte para PEM para não depender do suporte do OpenSSL 3 a algoritmos
// legados (RC2/3DES) que alguns .pfx da ICP-Brasil ainda usam.

import fs from 'node:fs';
import forge from 'node-forge';

export function lerCertificado(caminho, senha) {
  if (!caminho) throw new Error('CERT_PFX_PATH não definido no .env');
  if (!fs.existsSync(caminho)) throw new Error(`Certificado não encontrado: ${caminho}`);

  const der = fs.readFileSync(caminho).toString('binary');
  let p12;
  try {
    p12 = forge.pkcs12.pkcs12FromAsn1(forge.asn1.fromDer(der), senha ?? '');
  } catch (e) {
    throw new Error('Não foi possível abrir o .pfx (senha incorreta ou arquivo inválido).');
  }

  const chaves = [
    ...(p12.getBags({ bagType: forge.pki.oids.pkcs8ShroudedKeyBag })[forge.pki.oids.pkcs8ShroudedKeyBag] || []),
    ...(p12.getBags({ bagType: forge.pki.oids.keyBag })[forge.pki.oids.keyBag] || []),
  ];
  if (!chaves.length) throw new Error('O .pfx não contém chave privada.');
  const chavePrivada = chaves[0].key;

  const certs = (p12.getBags({ bagType: forge.pki.oids.certBag })[forge.pki.oids.certBag] || []).map((b) => b.cert);

  // O certificado do titular é o que tem a mesma chave pública da chave privada.
  const modulo = chavePrivada.n.toString(16);
  const certificado = certs.find((c) => c.publicKey.n?.toString(16) === modulo);
  if (!certificado) throw new Error('Certificado correspondente à chave privada não encontrado no .pfx.');
  const cadeia = certs.filter((c) => c !== certificado);

  return {
    keyPem: forge.pki.privateKeyToPem(chavePrivada),
    certPem: forge.pki.certificateToPem(certificado),
    cadeiaPem: cadeia.map((c) => forge.pki.certificateToPem(c)),
    info: descreverCertificado(certificado),
  };
}

export function descreverCertificado(cert) {
  const cn = cert.subject.getField('CN')?.value || '';
  const [nome, documento] = cn.split(':');
  const agora = new Date();
  return {
    titular: nome,
    documento: documento || null,
    tipo: documento?.length === 14 ? 'e-CNPJ' : documento?.length === 11 ? 'e-CPF' : 'desconhecido',
    emissor: cert.issuer.getField('CN')?.value || '',
    validoDe: cert.validity.notBefore,
    validoAte: cert.validity.notAfter,
    vigente: agora >= cert.validity.notBefore && agora <= cert.validity.notAfter,
  };
}
