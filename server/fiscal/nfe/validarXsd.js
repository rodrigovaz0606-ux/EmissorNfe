// Validação do XML contra os schemas oficiais (XSD) usando libxml2 em WebAssembly.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { validateXML } from 'xmllint-wasm';

const RAIZ = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
export const PACOTE_SCHEMAS = path.join(RAIZ, 'schemas', 'PL_009g_V4_00_NT_2020_005_v120');

let cacheSchemas = null;
function carregarSchemas() {
  if (!cacheSchemas) {
    cacheSchemas = fs
      .readdirSync(PACOTE_SCHEMAS)
      .filter((f) => f.endsWith('.xsd'))
      .map((f) => ({ fileName: f, contents: fs.readFileSync(path.join(PACOTE_SCHEMAS, f), 'utf8') }));
  }
  return cacheSchemas;
}

/**
 * @param {string} xml
 * @param {string} [schemaPrincipal='nfe_v4.00.xsd']
 * @returns {Promise<{ valido: boolean, erros: string[] }>}
 */
export async function validarXsd(xml, schemaPrincipal = 'nfe_v4.00.xsd') {
  const todos = carregarSchemas();
  const principal = todos.find((s) => s.fileName === schemaPrincipal);
  if (!principal) throw new Error(`Schema ${schemaPrincipal} não encontrado em ${PACOTE_SCHEMAS}`);

  const resultado = await validateXML({
    xml: [{ fileName: 'nfe.xml', contents: xml }],
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
  return String(msg).replace(/\{http:\/\/www\.portalfiscal\.inf\.br\/nfe\}/g, '').replace(/\{http:\/\/www\.w3\.org\/2000\/09\/xmldsig#\}/g, 'ds:').trim();
}
