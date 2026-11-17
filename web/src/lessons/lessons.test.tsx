import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { Trace } from '../trace/types.ts';
import { CONCEPT_IDS, concepts, conceptsOf, lessons } from './index.ts';
import { noteFor, parseConcepts, parseLesson } from './lesson.ts';
import { Markdown } from './Markdown.tsx';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const exampleIds = readdirSync(root + 'examples')
  .filter((f) => f.endsWith('.c'))
  .map((f) => f.replace(/\.c$/, ''));

// Líneas que la traza de referencia no recorre porque necesitan intervenir: Ctrl+C, enviar EOF, o
// una planificación en la que el buffer se llena o se vacía.
const NEEDS_INTERACTION: Record<string, number[]> = {
  '11_sigint': [9, 10, 20],
  '14_productor_consumidor': [17, 31],
  entrada_estandar: [10],
};

describe('lecciones de los ejemplos', () => {
  it('cada ejemplo tiene su lección', () => {
    expect([...lessons.keys()].sort()).toEqual([...exampleIds].sort());
  });

  describe.each(exampleIds)('%s', (id) => {
    const lesson = lessons.get(id)!;
    const source = readFileSync(`${root}examples/${id}.c`, 'utf8').split('\n');
    const trace = JSON.parse(readFileSync(`${root}traces/reference/${id}.json`, 'utf8')) as Trace;
    const ran = new Set(trace.steps.map((s) => s.executed?.line));

    it('tiene título, objetivo, teoría, cierre y ejercicios', () => {
      expect(lesson.title).not.toBe(id);
      expect(lesson.goal).toMatch(/^\*\*Objetivo:\*\*/);
      expect(lesson.intro.length).toBeGreaterThan(200);
      expect(lesson.lines.length).toBeGreaterThan(0);
      expect(lesson.wrapUp).toMatch(/^- /);
      expect(lesson.practice).toMatch(/^- /);
    });

    it('cada nota cita el código de su línea', () => {
      for (const n of lesson.lines) expect(source[n.line - 1], `línea ${n.line}`).toContain(n.code);
    });

    it('cada nota corresponde a una línea que se ejecuta', () => {
      const missing = lesson.lines.map((n) => n.line).filter((l) => !ran.has(l));
      expect(missing).toEqual(NEEDS_INTERACTION[id] ?? []);
    });

    it('no repite líneas y cada nota tiene explicación', () => {
      const lines = lesson.lines.map((n) => n.line);
      expect(new Set(lines).size).toBe(lines.length);
      for (const n of lesson.lines) expect(n.body.length, `línea ${n.line}`).toBeGreaterThan(20);
    });
  });
});

describe('fichas de conceptos', () => {
  it('existe una ficha por cada concepto, y ninguna sobra', () => {
    expect([...concepts.keys()].sort()).toEqual([...CONCEPT_IDS].sort());
    for (const c of concepts.values()) expect(c.body.length, c.id).toBeGreaterThan(100);
  });

  it('todo evento de las trazas de referencia y sintéticas lleva a fichas que existen', () => {
    const seen = new Set<string>();
    for (const dir of ['traces/reference', 'traces/synthetic']) {
      for (const f of readdirSync(root + dir).filter((f) => f.endsWith('.json'))) {
        const trace = JSON.parse(readFileSync(`${root}${dir}/${f}`, 'utf8')) as Trace;
        for (const step of trace.steps) for (const id of conceptsOf(step)) seen.add(id);
      }
    }
    for (const id of seen) expect(concepts.has(id), id).toBe(true);
    // Las trazas cubren lo principal de cada tema.
    for (const id of ['fork', 'pthread-create', 'exec', 'wait', 'join', 'pipe', 'dup2', 'kill', 'handler', 'mutex', 'cond', 'malloc', 'free', 'deadlock']) {
      expect(seen.has(id), id).toBe(true);
    }
  });

  it('un paso con fork y wait muestra sus fichas en orden y sin repetir', () => {
    const trace = JSON.parse(readFileSync(`${root}traces/reference/03_fork_simple.json`, 'utf8')) as Trace;
    const forkStep = trace.steps.find((s) => s.events.some((e) => e.type === 'fork'))!;
    expect(conceptsOf(forkStep)).toEqual(['fork']);
    const reap = trace.steps.find((s) => s.events.some((e) => e.type === 'wait' && e.reaped))!;
    expect(conceptsOf(reap)).toEqual(['wait']);
  });
});

describe('formato de las lecciones', () => {
  it('lee título, objetivo, introducción y secciones', () => {
    const l = parseLesson('x', '# Título\n\n> **Objetivo:** algo\n> más\n\nTeoría.\n\n## Línea 3 · `int x;`\n\nNota.\n\n## Al terminar\n\n- a\n\n## Para practicar\n\n- b\n');
    expect(l).toEqual({
      id: 'x',
      title: 'Título',
      goal: '**Objetivo:** algo\nmás',
      intro: 'Teoría.',
      lines: [{ line: 3, code: 'int x;', body: 'Nota.' }],
      wrapUp: '- a',
      practice: '- b',
    });
    expect(noteFor(l, 3)?.body).toBe('Nota.');
    expect(noteFor(l, 4)).toBeUndefined();
    expect(noteFor(l, null)).toBeUndefined();
  });

  it('rechaza secciones que no conoce', () => {
    expect(() => parseLesson('x', '# T\n\n## Línea 3 sin código\n')).toThrow(/sección desconocida/);
  });

  it('lee las fichas', () => {
    expect([...parseConcepts('# x\n\n## fork · fork()\n\nTexto.\n\n## Otra cosa\n').values()]).toEqual([{ id: 'fork', title: 'fork()', body: 'Texto.' }]);
  });
});

describe('Markdown', () => {
  const html = (text: string) => renderToStaticMarkup(<Markdown text={text} />);

  it('arma párrafos, listas con continuación y bloques de código', () => {
    expect(html('Uno\ndos.\n\n- a\n  sigue\n- b\n\n```\nint x;\n```')).toBe(
      '<p>Uno dos.</p><ul><li>a sigue</li><li>b</li></ul><pre class="md-code"><code>int x;</code></pre>',
    );
  });

  it('marca código, negrita y cursiva dentro del texto', () => {
    expect(html('Usa `p->x`, **siempre** y *casi*.')).toBe('<p>Usa <code>p-&gt;x</code>, <strong>siempre</strong> y <em>casi</em>.</p>');
    expect(html('`*p` y `a * b`')).toBe('<p><code>*p</code> y <code>a * b</code></p>');
    expect(html('**cambia `x`** y *usa `p`*')).toBe('<p><strong>cambia <code>x</code></strong> y <em>usa <code>p</code></em></p>');
  });

  it('nunca inserta HTML', () => {
    expect(html('<img src=x onerror=alert(1)>')).toBe('<p>&lt;img src=x onerror=alert(1)&gt;</p>');
  });

  it('las lecciones no dejan asteriscos sueltos', () => {
    // Un * fuera de `código` que no forma negrita ni cursiva aparecería tal cual en el panel.
    const all = [...lessons.values()].flatMap((l) => [l.goal, l.intro, l.wrapUp, l.practice, ...l.lines.map((n) => n.body)]);
    all.push(...[...concepts.values()].map((c) => c.body));
    for (const text of all) {
      const plain = html(text).replace(/<code>.*?<\/code>/g, '');
      expect(plain, text.slice(0, 60)).not.toContain('*');
      expect(plain, text.slice(0, 60)).not.toContain('`');
    }
  });
});
