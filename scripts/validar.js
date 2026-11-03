// npm run validar -- <arquivo.xml>: valida um XML de NF-e contra o XSD.
import fs from 'node:fs';
import { validarXsd } from '../server/fiscal/nfe/validarXsd.js';

const arquivo = process.argv[2];
if (!arquivo) {
  console.error('Uso: npm run validar -- <arquivo.xml>');
  process.exit(1);
}
const { valido, erros } = await validarXsd(fs.readFileSync(arquivo, 'utf8'));
console.log(valido ? 'XML válido no XSD.' : `XML inválido (${erros.length} erro(s)):`);
erros.forEach((e) => console.log(`  - ${e}`));
process.exitCode = valido ? 0 : 1;
