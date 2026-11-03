// npm run cert: só abre o .pfx e mostra os dados do certificado (sem rede).
import 'dotenv/config';
import { lerCertificado } from '../server/fiscal/certificado.js';

try {
  const { info, cadeiaPem } = lerCertificado(process.env.CERT_PFX_PATH, process.env.CERT_PFX_SENHA);
  console.log({ ...info, certificadosNaCadeia: cadeiaPem.length });
} catch (e) {
  console.error(`Erro: ${e.message}`);
  process.exitCode = 1;
}
