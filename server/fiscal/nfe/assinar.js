// Assinatura XMLDSig da NF-e, no padrão exigido pela SEFAZ (MOC 4.00):
//   - assinatura "enveloped" referenciando o infNFe pelo atributo Id (#NFe + chave)
//   - canonicalização C14N 1.0 (sem comentários)
//   - transforms: enveloped-signature + C14N
//   - digest SHA-1 e assinatura RSA-SHA1
//   - KeyInfo com o certificado X509 do emitente
//   - <Signature> sem prefixo, logo após </infNFe>, dentro de <NFe>

import { SignedXml } from 'xml-crypto';

const C14N = 'http://www.w3.org/TR/2001/REC-xml-c14n-20010315';
const ENVELOPED = 'http://www.w3.org/2000/09/xmldsig#enveloped-signature';
const RSA_SHA1 = 'http://www.w3.org/2000/09/xmldsig#rsa-sha1';
const SHA1 = 'http://www.w3.org/2000/09/xmldsig#sha1';

// O mesmo esquema serve para outros documentos (eventos: infEvento, inutilização: infInut).
export function assinarXml(xml, certificado, elemento = 'infNFe') {
  const xpath = `//*[local-name(.)='${elemento}']`;
  const sig = new SignedXml({
    privateKey: certificado.keyPem,
    publicCert: certificado.certPem,
    signatureAlgorithm: RSA_SHA1,
    canonicalizationAlgorithm: C14N,
  });
  sig.addReference({ xpath, transforms: [ENVELOPED, C14N], digestAlgorithm: SHA1 });
  sig.computeSignature(xml, { location: { reference: xpath, action: 'after' } });
  return sig.getSignedXml();
}

export const assinarNFe = (xml, certificado) => assinarXml(xml, certificado, 'infNFe');

// Confere uma assinatura usando o certificado embutido no próprio XML (KeyInfo).
export function verificarAssinatura(xml) {
  const sig = new SignedXml();
  const m = xml.match(/<X509Certificate>([^<]+)<\/X509Certificate>/);
  if (!m) return { valida: false, motivo: 'X509Certificate não encontrado' };
  const signature = xml.match(/<Signature[\s>][\s\S]*<\/Signature>/)?.[0];
  if (!signature) return { valida: false, motivo: 'Signature não encontrada' };

  sig.publicCert = `-----BEGIN CERTIFICATE-----\n${m[1].match(/.{1,64}/g).join('\n')}\n-----END CERTIFICATE-----\n`;
  sig.loadSignature(signature);
  try {
    return { valida: sig.checkSignature(xml) };
  } catch (e) {
    return { valida: false, motivo: e.message };
  }
}
