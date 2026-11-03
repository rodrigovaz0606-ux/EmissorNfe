// Certificado autoassinado para testes automatizados (não depende do .pfx real).
import crypto from 'node:crypto';
import forge from 'node-forge';

export function certificadoDeTeste(cn = 'PRODUTOR TESTE:52998224725') {
  const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
  const keyPem = privateKey.export({ type: 'pkcs8', format: 'pem' });
  const cert = forge.pki.createCertificate();
  cert.publicKey = forge.pki.publicKeyFromPem(publicKey.export({ type: 'spki', format: 'pem' }));
  cert.serialNumber = '01';
  cert.validity.notBefore = new Date();
  cert.validity.notAfter = new Date(Date.now() + 86_400_000);
  const attrs = [{ name: 'commonName', value: cn }];
  cert.setSubject(attrs);
  cert.setIssuer(attrs);
  cert.sign(forge.pki.privateKeyFromPem(keyPem), forge.md.sha256.create());
  return { keyPem, certPem: forge.pki.certificateToPem(cert), cadeiaPem: [] };
}
