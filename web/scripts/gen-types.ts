// Genera src/trace/types.ts desde schema/trace.schema.json (el contrato es la fuente de verdad).
import { compile } from 'json-schema-to-typescript';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const schema = JSON.parse(readFileSync(root + 'schema/trace.schema.json', 'utf8'));

const banner = '/* Generado por scripts/gen-types.ts desde schema/trace.schema.json. No editar a mano. */';
const ts = await compile(schema, 'Trace', {
  bannerComment: banner,
  additionalProperties: false,
  unreachableDefinitions: true,
  style: { singleQuote: true, printWidth: 110 },
});
const out = root + 'web/src/trace/types.ts';
const check = process.argv.includes('--check');
if (check) {
  if (readFileSync(out, 'utf8') !== ts) {
    console.error('src/trace/types.ts está desactualizado: ejecuta npm run gen:types');
    process.exit(1);
  }
} else {
  writeFileSync(out, ts);
}
