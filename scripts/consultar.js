// npm run consultar -- <chave>: situação de uma NF-e na SEFAZ (somente leitura).
import 'dotenv/config';
import { carregarConfig } from '../server/fiscal/config.js';
import { lerCertificado } from '../server/fiscal/certificado.js';
import { consultarSituacao } from '../server/fiscal/consulta.js';

try {
  const chave = (process.argv[2] ?? '').replace(/\D/g, '');
  const config = carregarConfig();
  const r = await consultarSituacao(config, lerCertificado(config.certPath, config.certSenha), chave);
  console.log(`cStat ${r.cStat}: ${r.xMotivo}`);
  if (r.protocolo) console.log(`Protocolo ${r.protocolo.nProt} (${r.protocolo.cStat} ${r.protocolo.xMotivo})`);
  r.eventos.forEach((e) => console.log(`Evento ${e.tpEvento} ${e.xEvento ?? ''}: ${e.cStat} protocolo ${e.nProt}`));
} catch (e) {
  console.error(`Erro: ${e.message}`);
  process.exitCode = 1;
}
