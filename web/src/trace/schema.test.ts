import { describe, expect, it } from 'vitest';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { checkInvariants } from './invariants.ts';
import type { Trace } from './types.ts';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const schema = JSON.parse(readFileSync(root + 'schema/trace.schema.json', 'utf8'));
const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
const validate = ajv.compile(schema);

const dirs = ['traces/synthetic', 'traces/reference'];
const files = dirs.flatMap((d) =>
  readdirSync(root + d)
    .filter((f) => f.endsWith('.json'))
    .map((f) => `${d}/${f}`),
);

describe('trazas', () => {
  it('hay trazas que validar', () => {
    expect(files.length).toBeGreaterThan(0);
  });

  for (const file of files) {
    describe(file, () => {
      const trace = JSON.parse(readFileSync(root + file, 'utf8')) as Trace;

      it('valida contra trace.schema.json', () => {
        const ok = validate(trace);
        expect(ok ? [] : validate.errors?.slice(0, 5)).toEqual([]);
      });

      it('cumple las invariantes', () => {
        expect(checkInvariants(trace)).toEqual([]);
      });
    });
  }
});

describe('esquema', () => {
  it('rechaza bytes fuera de U+0000–U+00FF', () => {
    const trace = JSON.parse(readFileSync(root + files[0], 'utf8')) as Trace;
    trace.stdin = 'año€';
    expect(validate(trace)).toBe(false);
  });

  it('rechaza un fd de pipe sin extremo', () => {
    const trace = JSON.parse(readFileSync(root + files[0], 'utf8')) as Trace;
    (trace.steps[0].processes[0].fds as Record<string, unknown>)['9'] = { kind: 'pipe', pipe: 'p0' };
    expect(validate(trace)).toBe(false);
  });
});

describe('invariantes', () => {
  it('detectan un fd de pipe que el pipe no registra', () => {
    const trace = JSON.parse(readFileSync(root + 'traces/synthetic/fork_pipe.json', 'utf8')) as Trace;
    const step = trace.steps.find((s) => s.pipes.length > 0)!;
    step.pipes[0].readers = [];
    expect(checkInvariants(trace).length).toBeGreaterThan(0);
  });

  it('detectan un hilo bloqueado sin motivo', () => {
    const trace = JSON.parse(readFileSync(root + 'traces/synthetic/threads_mutex.json', 'utf8')) as Trace;
    trace.steps[3].processes[0].threads[0].state = 'blocked';
    expect(checkInvariants(trace).some((e) => e.includes('sin motivo'))).toBe(true);
  });
});

describe('trazas sintéticas', () => {
  const synthetic = files.filter((f) => f.startsWith('traces/synthetic'));
  it.each(synthetic)('%s: la salida es UTF-8 válido', (file) => {
    const trace = JSON.parse(readFileSync(root + file, 'utf8')) as Trace;
    const decoder = new TextDecoder('utf-8', { fatal: true });
    for (const chunk of trace.output) {
      const bytes = Uint8Array.from(chunk.bytes, (c) => c.charCodeAt(0));
      expect(() => decoder.decode(bytes), JSON.stringify(chunk.bytes)).not.toThrow();
    }
  });
});
