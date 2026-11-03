// npm run gerar: gera, assina e valida (XSD) o XML do cenário de exemplo em saida/.
import 'dotenv/config';
import fs from 'node:fs';
import path from 'node:path';
import { carregarConfig } from '../server/fiscal/config.js';
import { lerCertificado } from '../server/fiscal/certificado.js';
import { gerarXmlNFe } from '../server/fiscal/nfe/gerarXml.js';
import { assinarNFe, verificarAssinatura } from '../server/fiscal/nfe/assinar.js';
import { validarXsd } from '../server/fiscal/nfe/validarXsd.js';
import { vendaSoja } from '../exemplos/venda-soja.js';

try {
  const config = carregarConfig();
  const certificado = lerCertificado(config.certPath, config.certSenha);
  const { info } = certificado;
  if (info.tipo !== 'e-CPF') throw new Error('O cenário de exemplo espera um e-CPF.');

  const dados = vendaSoja({ cpfEmitente: info.documento, nomeEmitente: info.titular });
  const { xml, chave, vNF } = gerarXmlNFe(dados, config);
  const assinado = assinarNFe(xml, certificado);

  fs.mkdirSync('saida', { recursive: true });
  const arquivo = path.join('saida', `${chave}-nfe.xml`);
  fs.writeFileSync(arquivo, '<?xml version="1.0" encoding="UTF-8"?>' + assinado, 'utf8');
  console.log(`Chave: ${chave}\nValor: R$ ${vNF}\nArquivo: ${arquivo}`);

  const assinatura = verificarAssinatura(assinado);
  console.log(`Assinatura: ${assinatura.valida ? 'válida' : `INVÁLIDA (${assinatura.motivo ?? ''})`}`);
  const { valido, erros } = await validarXsd(assinado);
  console.log(valido ? 'XSD: válido' : `XSD: ${erros.length} erro(s)`);
  erros.forEach((e) => console.log(`  - ${e}`));
  if (!valido || !assinatura.valida) process.exitCode = 1;
} catch (e) {
  console.error(`Erro: ${e.message}`);
  process.exitCode = 1;
}
