// npm run enviar: gera, assina, valida e ENVIA o cenário de exemplo para a SEFAZ (homologação).
import 'dotenv/config';
import fs from 'node:fs';
import path from 'node:path';
import { carregarConfig } from '../server/fiscal/config.js';
import { lerCertificado } from '../server/fiscal/certificado.js';
import { gerarXmlNFe } from '../server/fiscal/nfe/gerarXml.js';
import { assinarNFe, verificarAssinatura } from '../server/fiscal/nfe/assinar.js';
import { validarXsd } from '../server/fiscal/nfe/validarXsd.js';
import { autorizarNFe, montarNfeProc } from '../server/fiscal/autorizacao.js';
import { vendaSoja } from '../exemplos/venda-soja.js';

const salvar = (nome, conteudo) => {
  fs.mkdirSync('saida', { recursive: true });
  const arquivo = path.join('saida', nome);
  fs.writeFileSync(arquivo, conteudo, 'utf8');
  return arquivo;
};

try {
  const config = carregarConfig(); // falha se não for homologação
  const certificado = lerCertificado(config.certPath, config.certSenha);
  const { info } = certificado;

  const dados = vendaSoja({ cpfEmitente: info.documento, nomeEmitente: info.titular });
  const { xml, chave, vNF } = gerarXmlNFe(dados, config);
  const assinado = assinarNFe(xml, certificado);

  if (!verificarAssinatura(assinado).valida) throw new Error('Assinatura não confere; envio cancelado.');
  const xsd = await validarXsd(assinado);
  if (!xsd.valido) throw new Error(`XML inválido no XSD; envio cancelado:\n  - ${xsd.erros.join('\n  - ')}`);

  console.log(`Chave ${chave} | R$ ${vNF} | assinatura e XSD OK`);
  console.log(`Enviando para NFeAutorizacao4 — ${config.uf}, HOMOLOGAÇÃO...`);

  const r = await autorizarNFe(config, certificado, assinado);
  salvar(`${chave}-enviNFe.xml`, r.enviado);
  const arqResposta = salvar(`${chave}-resposta.xml`, r.respostaSoap);

  console.log(`\nLote: cStat ${r.lote.cStat} — ${r.lote.xMotivo}`);
  if (r.recibo) console.log(`Recibo: ${r.recibo}`);
  if (r.protocolo) {
    const p = r.protocolo;
    console.log(`Nota: cStat ${p.cStat} — ${p.xMotivo}`);
    if (p.autorizada) {
      const arq = salvar(`${chave}-procNFe.xml`, montarNfeProc(assinado, p.xml));
      console.log(`AUTORIZADA. Protocolo ${p.nProt}\nXML autorizado: ${arq}`);
    }
  }
  console.log(`Resposta completa: ${arqResposta}`);
} catch (e) {
  console.error(`Erro: ${e.message}`);
  process.exitCode = 1;
}
