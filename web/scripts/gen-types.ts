// Genera los tipos de src/trace desde los esquemas (el contrato es la fuente de verdad):
// types.ts desde schema/trace.schema.json y compact-types.ts desde schema/compact.schema.json.
import { compile } from 'json-schema-to-typescript';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const check = process.argv.includes('--check');
let stale = false;

for (const [schemaFile, name, outFile] of [
  ['trace.schema.json', 'Trace', 'types.ts'],
  ['compact.schema.json', 'CompactTrace', 'compact-types.ts'],
]) {
  const schema = JSON.parse(readFileSync(root + 'schema/' + schemaFile, 'utf8'));
  const banner = `/* Generado por scripts/gen-types.ts desde schema/${schemaFile}. No editar a mano. */`;
  const ts = await compile(schema, name, {
    bannerComment: banner,
    additionalProperties: false,
    unreachableDefinitions: true,
    style: { singleQuote: true, printWidth: 110 },
  });
  const out = root + 'web/src/trace/' + outFile;
  if (check) {
    if (readFileSync(out, 'utf8') !== ts) {
      console.error(`src/trace/${outFile} está desactualizado: ejecuta npm run gen:types`);
      stale = true;
    }
  } else {
    writeFileSync(out, ts);
  }
}
if (stale) process.exit(1);
