import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describeStep } from '../../trace/describe.ts';
import { indexTrace, nextEventStep, nextThreadStep, runToLine } from '../../trace/query.ts';
import type { MemorySnapshot, Trace } from '../../trace/types.ts';
import { layoutMemory, resolvePointer } from './memoryLayout.ts';
import { layoutScene } from './sceneLayout.ts';

const root = fileURLToPath(new URL('../../../../traces/synthetic/', import.meta.url));
const load = (name: string) => JSON.parse(readFileSync(root + name + '.json', 'utf8')) as Trace;
const all = readdirSync(root)
  .filter((f) => f.endsWith('.json'))
  .map((f) => f.replace(/\.json$/, ''));
const opts = { memOpen: () => true, inkOf: () => 'x' };

describe('resolvePointer', () => {
  const at = (addr: number, size: number, depth: number, array = false) => ({ addr, size, depth, array, freed: false, rect: { x: depth, y: 0, w: 1, h: 1 } });

  it('prefiere el struct completo antes que su primer campo', () => {
    const hit = resolvePointer([at(0x100, 8, 2), at(0x100, 8, 1)], 0x100);
    expect(hit?.depth).toBe(1);
  });

  it('un puntero al inicio de un arreglo apunta a su primer elemento', () => {
    const hit = resolvePointer([at(0x100, 16, 0, true), at(0x100, 4, 1)], 0x100);
    expect(hit?.depth).toBe(1);
  });

  it('un puntero al medio de un bloque cae en la celda más interna que lo contiene', () => {
    const hit = resolvePointer([at(0x100, 16, 0), at(0x108, 8, 1)], 0x10a);
    expect(hit?.depth).toBe(1);
  });

  it('sin coincidencia el puntero es colgante', () => {
    expect(resolvePointer([at(0x100, 4, 0)], 0x200)).toBeNull();
  });
});

describe('layoutMemory', () => {
  const trace = load('structs_heap');
  const last = trace.steps.at(-2)!;
  const snap = trace.snapshots[last.processes[0].mem!] as MemorySnapshot;
  const mem = layoutMemory(snap, [{ tid: 1000, title: '', ink: 'x' }]);

  it('dibuja la lista enlazada de izquierda a derecha siguiendo las flechas', () => {
    const heap = mem.prims.filter((p) => p.k === 'heap');
    const x = (addr: string) => heap.find((p) => p.k === 'heap' && p.addr === addr)!.r.x;
    expect(x('0x4052e0')).toBeLessThan(x('0x4052c0'));
    expect(x('0x4052c0')).toBeLessThan(x('0x4052a0'));
  });

  it('el puntero a memoria liberada se marca como peligroso', () => {
    const toFreed = mem.arrows.filter((a) => a.target === '0x4052e0');
    expect(toFreed.length).toBeGreaterThan(0);
    expect(toFreed.every((a) => a.freed)).toBe(true);
  });

  it('r.center apunta a pts[1], no a pts[1].x', () => {
    const arrow = mem.arrows.find((a) => a.target === '0x7fffffffc908')!;
    expect(arrow.to).not.toBeNull();
    // pts[1] es un struct de dos filas: su celda es más alta que una sola fila.
    expect(arrow.to!.h).toBeGreaterThan(30);
  });
});

describe.each(all)('layoutScene · %s', (name) => {
  const trace = load(name);
  const index = indexTrace(trace);

  it('es determinista', () => {
    const t = Math.floor(trace.steps.length / 2);
    expect(JSON.stringify(serializable(layoutScene(trace, t, opts, index)))).toBe(
      JSON.stringify(serializable(layoutScene(trace, t, opts, indexTrace(trace)))),
    );
  });

  it('los cuadrados nunca se superponen y cada pipe tiene tubo y cables', () => {
    trace.steps.forEach((step, t) => {
      const scene = layoutScene(trace, t, opts, index);
      const boxes = [...scene.boxes.values()];
      for (let i = 0; i < boxes.length; i++) {
        for (let j = i + 1; j < boxes.length; j++) {
          const a = boxes[i];
          const b = boxes[j];
          const overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
          expect(overlap, `t=${t}: ${a.pid} y ${b.pid}`).toBe(false);
        }
      }
      expect(scene.pipes.map((p) => p.id).sort()).toEqual(step.pipes.map((p) => p.id).sort());
      const ends = step.pipes.reduce((n, p) => n + p.readers.length + p.writers.length, 0);
      expect(scene.cables.length).toBe(ends);
    });
  });

  it('cada paso tiene una explicación', () => {
    trace.steps.forEach((_, t) => expect(describeStep(trace, index, t).length, `t=${t}`).toBeGreaterThan(0));
  });
});

describe('navegación', () => {
  const trace = load('fork_pipe');

  it('siguiente evento salta el paso sin eventos', () => {
    // t=3 es un paso de línea común; t=5 cierra un fd.
    expect(trace.steps[3].events).toEqual([]);
    expect(nextEventStep(trace, 2)).toBe(5);
  });

  it('paso del hilo avanza hasta el próximo paso de ese hilo', () => {
    const s = nextThreadStep(trace, 2, 1001);
    expect(trace.steps[s].actor?.tid).toBe(1001);
    expect(s).toBe(3);
  });

  it('ejecutar hasta una línea se detiene cuando un hilo queda en ella', () => {
    const s = runToLine(trace, 0, 22);
    const actor = trace.steps[s].actor!;
    const th = trace.steps[s].processes.find((p) => p.pid === actor.pid)!.threads[0];
    expect(th.line).toBe(22);
  });
});

function serializable(scene: ReturnType<typeof layoutScene>) {
  return { ...scene, boxes: [...scene.boxes.entries()] };
}
