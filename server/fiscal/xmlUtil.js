// Utilitários para ler as respostas da SEFAZ.
import { XMLParser } from 'fast-xml-parser';

export const parser = new XMLParser({ removeNSPrefix: true, ignoreAttributes: false, parseTagValue: false });

// Busca uma tag em qualquer profundidade (o envelope SOAP varia entre UFs).
export function encontrar(no, nome) {
  if (!no || typeof no !== 'object') return null;
  if (no[nome] !== undefined) return no[nome];
  for (const valor of Object.values(no)) {
    const achado = encontrar(valor, nome);
    if (achado) return achado;
  }
  return null;
}

// Recorta o texto original de um elemento (ex.: protNFe), preservando-o byte a byte.
// Necessário para montar o nfeProc sem alterar o que a SEFAZ assinou/devolveu.
export function recortarElemento(xml, nome) {
  const inicio = xml.search(new RegExp(`<(\\w+:)?${nome}[\\s>]`));
  if (inicio < 0) return null;
  const fim = new RegExp(`</(\\w+:)?${nome}>`).exec(xml.slice(inicio));
  return fim ? xml.slice(inicio, inicio + fim.index + fim[0].length) : null;
}
