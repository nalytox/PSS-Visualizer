import { describe, expect, it } from 'vitest';
import { Ajv2020 } from 'ajv/dist/2020.js';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { CompactTrace } from './compact-types.ts';
import { applyPatch, eventFlags, flag, loadTrace, type LoadedTrace } from './loaded.ts';
import { indexTrace, nextEventStep, runToLine } from './query.ts';
import type { Trace } from './types.ts';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const read = <T>(path: string) => JSON.parse(readFileSync(root + path, 'utf8')) as T;
const compactNames = readdirSync(root + 'traces/compact')
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.replace(/\.json$/, ''));

// Cada paso con su memoria en lugar del nombre de la instantánea: los nombres cambian al compactar.
function resolved(trace: LoadedTrace) {
  return Array.from({ length: trace.length }, (_, t) => {
    const step = trace.step(t);
    return { ...step, processes: step.processes.map((p) => ({ ...p, mem: trace.snapshot(p.mem) })) };
  });
}

describe('applyPatch', () => {
  it('cambia solo lo indicado y comparte el resto', () => {
    const a = { t: 4, processes: [{ pid: 1000, threads: [{ line: 7 }], fds: { 0: { kind: 'stdin' } } }] };
    const b = applyPatch(a, { t: 5, processes: { '0': { threads: { '0': { line: 8 } } } } }) as typeof a;
    expect(b).toEqual({ t: 5, processes: [{ pid: 1000, threads: [{ line: 8 }], fds: { 0: { kind: 'stdin' } } }] });
    expect(b.processes[0].fds).toBe(a.processes[0].fds);
    expect(a.processes[0].threads[0].line).toBe(7);
  });

  it('agrega, borra y reemplaza', () => {
    expect(applyPatch({ a: 1, b: { c: 2 } }, { a: { $d: 1 }, e: null, b: { d: { $s: [1, 2] } } })).toEqual({ b: { c: 2, d: [1, 2] }, e: null });
    expect(applyPatch([1, 2, 3], { $a: 1 })).toEqual([1]);
    expect(applyPatch([1], { $a: 3, '1': { $s: { x: 1 } }, '2': { $s: [2] } })).toEqual([1, { x: 1 }, [2]]);
    expect(applyPatch({ mem: null }, { mem: 'm1' })).toEqual({ mem: 'm1' });
  });
});

describe.each(compactNames)('traza compacta %s', (name) => {
  const compact = read<CompactTrace>(`traces/compact/${name}.json`);
  const full = read<Trace>(`traces/reference/${name}.json`);
  const v2 = loadTrace(compact);
  const v1 = loadTrace(full);

  it('valida contra compact.schema.json', () => {
    const ajv = new Ajv2020({ allErrors: true, strict: false });
    const validate = ajv.compile(read<object>('schema/compact.schema.json'));
    expect(validate(compact) ? [] : validate.errors?.slice(0, 5)).toEqual([]);
  });

  it('cada paso y cada memoria son los de la traza completa', () => {
    expect(v2.length).toBe(full.steps.length);
    expect(resolved(v2)).toEqual(resolved(v1));
  });

  it('las columnas del tracer coinciden con las calculadas en la interfaz', () => {
    expect([...v2.columns.flags]).toEqual([...v1.columns.flags]);
    expect([...v2.columns.actorPid]).toEqual([...v1.columns.actorPid]);
    expect([...v2.columns.actorTid]).toEqual([...v1.columns.actorTid]);
    expect([...v2.columns.actorLine]).toEqual([...v1.columns.actorLine]);
    expect(v2.appearance).toEqual(v1.appearance);
    expect(indexTrace(v2)).toEqual(indexTrace(v1));
  });

  it('se puede saltar a cualquier paso en cualquier orden', () => {
    const order = [...Array(full.steps.length).keys()].sort((a, b) => ((a * 7919) % 101) - ((b * 7919) % 101));
    const fresh = loadTrace(compact);
    for (const t of order) expect(fresh.step(t).t).toBe(t);
  });

  it('la navegación da lo mismo en las dos versiones', () => {
    for (let t = 0; t < v1.length; t += 3) {
      expect(nextEventStep(v2, t)).toBe(nextEventStep(v1, t));
      expect(runToLine(v2, t, 10)).toBe(runToLine(v1, t, 10));
    }
  });
});

describe('banderas', () => {
  it('distinguen lo que la línea de tiempo dibuja distinto', () => {
    expect(eventFlags([{ type: 'exit', pid: 1, scope: 'thread', tid: 2 }])).toBe(flag('exitThread'));
    expect(eventFlags([{ type: 'wait', pid: 1, target: -1 }])).toBe(flag('wait'));
    expect(eventFlags([{ type: 'wait', pid: 1, target: -1, reaped: 2 }])).toBe(flag('wait') | flag('waitReaped'));
    expect(eventFlags([{ type: 'truncated', reason: 'x' }]) >>> 0).toBe(flag('other'));
    expect(flag('other')).toBe(2 ** 31);
  });
});
