// npm run gerar: gera o XML (sem assinatura) do cenário de exemplo em saida/.
import 'dotenv/config';
import fs from 'node:fs';
import path from 'node:path';
import { carregarConfig } from '../server/fiscal/config.js';
import { lerCertificado } from '../server/fiscal/certificado.js';
import { gerarXmlNFe } from '../server/fiscal/nfe/gerarXml.js';
import { vendaSoja } from '../exemplos/venda-soja.js';

try {
  const config = carregarConfig();
  const { info } = lerCertificado(config.certPath, config.certSenha);
  if (info.tipo !== 'e-CPF') throw new Error('O cenário de exemplo espera um e-CPF.');

  const dados = vendaSoja({ cpfEmitente: info.documento, nomeEmitente: info.titular });
  const { xml, chave, vNF } = gerarXmlNFe(dados, config);

  fs.mkdirSync('saida', { recursive: true });
  const arquivo = path.join('saida', `${chave}-nfe.xml`);
  fs.writeFileSync(arquivo, '<?xml version="1.0" encoding="UTF-8"?>' + xml, 'utf8');
  console.log(`Chave: ${chave}\nValor: R$ ${vNF}\nArquivo: ${arquivo}`);
} catch (e) {
  console.error(`Erro: ${e.message}`);
  process.exitCode = 1;
}
