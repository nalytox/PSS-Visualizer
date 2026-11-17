// Lecciones de los ejemplos y fichas de conceptos, y qué fichas corresponden a cada paso.
import { processAt } from '../trace/query.ts';
import type { Event, Step } from '../trace/types.ts';
import conceptsText from './conceptos.md?raw';
import { parseConcepts, parseLesson, type Concept, type Lesson } from './lesson.ts';

const files = import.meta.glob<string>('@examples/*.md', { query: '?raw', import: 'default', eager: true });

export const lessons = new Map<string, Lesson>(
  Object.entries(files).map(([path, text]) => {
    const id = path.split('/').pop()!.replace(/\.md$/, '');
    return [id, parseLesson(id, text)];
  }),
);

export const concepts: Map<string, Concept> = parseConcepts(conceptsText);

// Fichas que explican los eventos de un paso, en el orden en que ocurrieron y sin repetir.
export function conceptsOf(step: Step): string[] {
  const out: string[] = [];
  const add = (...ids: string[]) => ids.forEach((id) => out.includes(id) || out.push(id));
  for (const ev of step.events) conceptsOfEvent(step, ev).forEach((id) => add(id));
  return out;
}

function conceptsOfEvent(step: Step, ev: Event): string[] {
  switch (ev.type) {
    case 'call':
    case 'return':
    case 'unblock':
    case 'signalReturn':
      return [];
    case 'fork':
      return ['fork'];
    case 'threadCreate':
      return ['pthread-create'];
    case 'exec':
      return ['exec'];
    case 'exit': {
      if (ev.scope === 'thread') return ['fin-hilo'];
      const p = processAt(step, ev.pid);
      if (p?.state === 'zombie') return ['exit', 'zombie'];
      return ['exit'];
    }
    case 'wait':
      return ev.reaped === undefined ? ['wnohang'] : ['wait'];
    case 'join':
      return ['join'];
    case 'reparent':
      return ['huerfano'];
    case 'pipe':
      return ['pipe'];
    case 'dup':
      return ['dup2'];
    case 'close':
      return ev.was.kind === 'pipe' ? ['close-pipe'] : ['close'];
    case 'read':
      if (ev.eof) return ev.stdin ? ['stdin'] : ['eof'];
      return ev.stdin ? ['stdin'] : ['read-pipe'];
    case 'write':
      if (ev.epipe) return ['sigpipe'];
      return ev.terminal ? ['salida-estandar'] : ['write-pipe'];
    case 'block':
      switch (ev.reason.kind) {
        case 'read':
          return ev.reason.stdin ? ['stdin'] : ['bloqueo-io'];
        case 'write':
          return ['bloqueo-io'];
        case 'wait':
          return ['wait'];
        case 'join':
          return ['join'];
        case 'mutex':
          return ['mutex'];
        case 'cond':
          return ['cond'];
        case 'sem':
          return ['sem'];
        case 'sleep':
          return ['sleep'];
        case 'pause':
        case 'sigsuspend':
          return ['pause'];
      }
      return [];
    case 'signalSend':
      switch (ev.from.kind) {
        case 'process':
          return ['kill'];
        case 'kernel':
          return ev.signal === 'SIGCHLD' ? ['sigchld'] : ['senales'];
        case 'timer':
          return ['alarm'];
        case 'terminal':
          return ['ctrl-c'];
      }
      return [];
    case 'signalDeliver':
      if (ev.action === 'handler') return ['handler'];
      if (ev.action === 'terminate' || ev.action === 'core') return ['accion-por-defecto'];
      return ['senales'];
    case 'mutex':
      return ['mutex'];
    case 'cond':
      return ['cond'];
    case 'sem':
      return ['sem'];
    case 'malloc':
      return ['malloc'];
    case 'free':
      return ev.error ? ['free-invalido'] : ['free'];
    case 'memError':
      return ev.kind === 'useAfterFree' ? ['uso-tras-free'] : ['segfault'];
    case 'stdinNeeded':
      return ['stdin'];
    case 'deadlock':
      return step.pipes.some((p) => p.warnings.length > 0) ? ['pipe-sin-cerrar', 'deadlock'] : ['deadlock'];
    case 'truncated':
      return ['limites'];
  }
}

// Todas las fichas a las que puede llevar un evento (las pruebas comprueban que existan).
export const CONCEPT_IDS = [
  'proceso',
  'fork',
  'pthread-create',
  'exec',
  'fin-hilo',
  'exit',
  'zombie',
  'wnohang',
  'wait',
  'join',
  'huerfano',
  'pipe',
  'dup2',
  'close-pipe',
  'close',
  'stdin',
  'eof',
  'read-pipe',
  'sigpipe',
  'salida-estandar',
  'write-pipe',
  'bloqueo-io',
  'mutex',
  'cond',
  'sem',
  'sleep',
  'pause',
  'kill',
  'sigchld',
  'senales',
  'alarm',
  'ctrl-c',
  'handler',
  'accion-por-defecto',
  'malloc',
  'free-invalido',
  'free',
  'uso-tras-free',
  'segfault',
  'pipe-sin-cerrar',
  'deadlock',
  'limites',
];
