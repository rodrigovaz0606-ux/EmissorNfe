import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import forge from 'node-forge';
import { assinarNFe, verificarAssinatura } from './assinar.js';
import { gerarXmlNFe } from './gerarXml.js';
import { validarXsd } from './validarXsd.js';
import { vendaSoja } from '../../../exemplos/venda-soja.js';

// Certificado autoassinado só para teste (não depende do .pfx real).
function certificadoDeTeste() {
  const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', { modulusLength: 2048 });
  const keyPem = privateKey.export({ type: 'pkcs8', format: 'pem' });
  const cert = forge.pki.createCertificate();
  cert.publicKey = forge.pki.publicKeyFromPem(publicKey.export({ type: 'spki', format: 'pem' }));
  cert.serialNumber = '01';
  cert.validity.notBefore = new Date();
  cert.validity.notAfter = new Date(Date.now() + 86_400_000);
  const attrs = [{ name: 'commonName', value: 'PRODUTOR TESTE:52998224725' }];
  cert.setSubject(attrs);
  cert.setIssuer(attrs);
  cert.sign(forge.pki.privateKeyFromPem(keyPem), forge.md.sha256.create());
  return { keyPem, certPem: forge.pki.certificateToPem(cert), cadeiaPem: [] };
}

const certificado = certificadoDeTeste();
const { xml, chave } = gerarXmlNFe(
  vendaSoja({ cpfEmitente: '52998224725', nomeEmitente: 'PRODUTOR TESTE' }),
  { cUF: 52, tpAmb: 2 },
);
const assinado = assinarNFe(xml, certificado);

test('XML assinado passa no XSD sem nenhum erro', async () => {
  const { valido, erros } = await validarXsd(assinado);
  assert.ok(valido, erros.join('\n'));
});

test('assinatura tem a estrutura exigida pela SEFAZ', () => {
  assert.match(assinado, /<\/infNFe><Signature xmlns="http:\/\/www\.w3\.org\/2000\/09\/xmldsig#"><SignedInfo>/);
  assert.match(assinado, new RegExp(`<Reference URI="#NFe${chave}">`));
  assert.match(assinado, /<CanonicalizationMethod Algorithm="http:\/\/www\.w3\.org\/TR\/2001\/REC-xml-c14n-20010315"\/>/);
  assert.match(assinado, /<SignatureMethod Algorithm="http:\/\/www\.w3\.org\/2000\/09\/xmldsig#rsa-sha1"\/>/);
  assert.match(assinado, /<Transform Algorithm="http:\/\/www\.w3\.org\/2000\/09\/xmldsig#enveloped-signature"\/><Transform Algorithm="http:\/\/www\.w3\.org\/TR\/2001\/REC-xml-c14n-20010315"\/>/);
  assert.match(assinado, /<DigestMethod Algorithm="http:\/\/www\.w3\.org\/2000\/09\/xmldsig#sha1"\/>/);
  assert.match(assinado, /<KeyInfo><X509Data><X509Certificate>[A-Za-z0-9+/=]+<\/X509Certificate><\/X509Data><\/KeyInfo><\/Signature><\/NFe>$/);
  assert.doesNotMatch(assinado, /ds:Signature/); // SEFAZ rejeita prefixo
});

test('assinatura confere', () => {
  assert.deepEqual(verificarAssinatura(assinado), { valida: true });
});

test('qualquer alteração no conteúdo invalida a assinatura', () => {
  const adulterado = assinado.replace('<vNF>66250.00</vNF>', '<vNF>1.00</vNF>');
  assert.notEqual(adulterado, assinado);
  assert.equal(verificarAssinatura(adulterado).valida, false);
});
