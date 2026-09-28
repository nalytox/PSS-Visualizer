import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { CHAPTERS } from './chapters.tsx';
import { captionAt, keyframes, timeline, tw, typed } from './engine.ts';

describe('motor de la introducción', () => {
  const tl = timeline([tw('a', 'x', 0, 100, 1, 3, 'linear'), tw('a', 'x', 100, 50, 5, 6, 'linear')], 10);

  it('antes, durante y después de cada tween', () => {
    const v = (t: number) => tl.values(t)('a', 'x');
    expect(v(0)).toBe(0);
    expect(v(2)).toBe(50);
    expect(v(4)).toBe(100);
    expect(v(5.5)).toBe(75);
    expect(v(9)).toBe(50);
    expect(tl.values(2)('b', 'x', 7)).toBe(7);
  });

  it('es determinista: el mismo t da el mismo valor sin importar el orden en que se pide', () => {
    const forward = [0, 1, 2, 3, 4, 5, 6].map((t) => tl.values(t)('a', 'x'));
    const backward = [6, 5, 4, 3, 2, 1, 0].map((t) => tl.values(t)('a', 'x')).reverse();
    expect(forward).toEqual(backward);
  });

  it('escribe texto letra a letra', () => {
    expect(typed('hola', 0.5)).toBe('ho');
    expect(typed('hola', 2)).toBe('hola');
  });
});

describe('capítulos (sección 11.2)', () => {
  const ranges: Record<string, [number, number]> = { forks: [25, 35], threads: [30, 40], pipes: [25, 35], signals: [20, 30] };

  it('son los cuatro del spec y enlazan a 03, 12, 06 y 09', () => {
    expect(CHAPTERS.map((c) => c.id)).toEqual(['forks', 'threads', 'pipes', 'signals']);
    expect(CHAPTERS.map((c) => c.example)).toEqual(['03_fork_simple', '12_hilos_carrera', '06_pipe_padre_hijo', '09_sigusr1']);
  });

  it.each(CHAPTERS.map((c) => [c.id, c] as const))('%s dura lo que pide el spec y sus subtítulos cubren todo', (id, c) => {
    const [lo, hi] = ranges[id];
    expect(c.timeline.duration).toBeGreaterThanOrEqual(lo);
    expect(c.timeline.duration).toBeLessThanOrEqual(hi);
    for (let t = 0; t < c.timeline.duration; t += 0.5) expect(captionAt(c.captions, t)?.text, `t=${t}`).toBeTruthy();
    expect(keyframes(c.captions)).toHaveLength(c.captions.length);
  });

  it('dibujar un fotograma no depende de los anteriores', () => {
    for (const c of CHAPTERS) {
      const a = JSON.stringify(c.render(c.timeline.values(12.3), 12.3), (k, v) => (k === '_owner' || k === '_store' ? undefined : v));
      c.render(c.timeline.values(3), 3);
      const b = JSON.stringify(c.render(c.timeline.values(12.3), 12.3), (k, v) => (k === '_owner' || k === '_store' ? undefined : v));
      expect(a).toBe(b);
    }
  });

  it('el código de la introducción pesa menos de 150 KB', () => {
    const dir = new URL('./', import.meta.url);
    const bytes = readdirSync(dir)
      .filter((f) => !f.endsWith('.test.ts'))
      .reduce((n, f) => n + readFileSync(new URL(f, dir)).length, 0);
    expect(bytes).toBeLessThan(150 * 1024);
  });
});
