// npm run status: testa certificado + conexão com a SEFAZ (homologação).
import 'dotenv/config';
import { carregarConfig } from '../server/fiscal/config.js';
import { lerCertificado } from '../server/fiscal/certificado.js';
import { consultarStatusServico } from '../server/fiscal/statusServico.js';

try {
  const config = carregarConfig();
  const cert = lerCertificado(config.certPath, config.certSenha);
  console.log(`Certificado: ${cert.info.titular} (${cert.info.tipo}), válido até ${cert.info.validoAte.toLocaleDateString('pt-BR')}`);
  if (!cert.info.vigente) console.warn('ATENÇÃO: certificado fora da validade.');

  console.log(`Consultando NFeStatusServico4 — ${config.uf}, homologação...`);
  const r = await consultarStatusServico(config, cert);
  console.log(`cStat ${r.cStat}: ${r.xMotivo}`);
  console.log(`verAplic ${r.verAplic} | recebido em ${r.dhRecbto} | tempo médio ${r.tMed ?? '-'}s`);
  process.exitCode = r.emOperacao ? 0 : 1;
} catch (e) {
  console.error(`Erro: ${e.message}`);
  if (e.code) console.error(`Código: ${e.code}`);
  process.exitCode = 1;
}
