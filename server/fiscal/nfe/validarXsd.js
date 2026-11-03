// Validação de XML contra os schemas oficiais (XSD) usando libxml2 em WebAssembly.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateXML } from 'xmllint-wasm';

const RAIZ = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const PASTA_SCHEMAS = path.join(RAIZ, 'schemas');

// Pacotes de schema usados por cada documento.
export const PACOTES = {
  nfe: { pasta: 'PL_009g_V4_00_NT_2020_005_v120', schema: 'nfe_v4.00.xsd' },
  inutilizacao: { pasta: 'PL_009g_V4_00_NT_2020_005_v120', schema: 'inutNFe_v4.00.xsd' },
  cancelamento: { pasta: 'Evento_Canc_PL_v1.01', schema: 'envEventoCancNFe_v1.00.xsd' },
  cce: { pasta: 'Evento_CCe_PL_v1.01', schema: 'envCCe_v1.00.xsd' },
};

const cache = new Map();
function carregarPasta(pasta) {
  if (!cache.has(pasta)) {
    const dir = path.join(PASTA_SCHEMAS, pasta);
    cache.set(
      pasta,
      fs.readdirSync(dir).filter((f) => f.endsWith('.xsd')).map((f) => ({ fileName: f, contents: fs.readFileSync(path.join(dir, f), 'utf8') })),
    );
  }
  return cache.get(pasta);
}

/**
 * @param {string} xml
 * @param {keyof PACOTES} [tipo='nfe']
 * @returns {Promise<{ valido: boolean, erros: string[] }>}
 */
export async function validarXsd(xml, tipo = 'nfe') {
  const pacote = PACOTES[tipo];
  if (!pacote) throw new Error(`Tipo de schema desconhecido: ${tipo}`);
  const todos = carregarPasta(pacote.pasta);
  const principal = todos.find((s) => s.fileName === pacote.schema);
  if (!principal) throw new Error(`Schema ${pacote.schema} não encontrado em schemas/${pacote.pasta}`);

  const resultado = await validateXML({
    xml: [{ fileName: 'documento.xml', contents: xml }],
    schema: [principal],
    preload: todos.filter((s) => s !== principal),
  });

  return {
    valido: resultado.valid,
    erros: resultado.errors.map((e) => limparMensagem(e.message ?? e.rawMessage)),
  };
}

// Tira o prefixo de namespace das mensagens do libxml2 para ficarem legíveis.
function limparMensagem(msg) {
  return String(msg)
    .replace(/\{http:\/\/www\.portalfiscal\.inf\.br\/nfe\}/g, '')
    .replace(/\{http:\/\/www\.w3\.org\/2000\/09\/xmldsig#\}/g, 'ds:')
    .trim();
}
