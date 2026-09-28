// Consultas sobre la traza. Todo se deriva del paso actual; nada de esto muta la traza.
import type { Event, MemorySnapshot, Process, Step, Thread, Trace } from './types.ts';

export const PROC_HUES = 8;

export function processAt(step: Step, pid: number): Process | undefined {
  return step.processes.find((p) => p.pid === pid);
}

export function threadAt(step: Step, pid: number, tid: number): Thread | undefined {
  return processAt(step, pid)?.threads.find((th) => th.tid === tid);
}

export function snapshotOf(trace: Trace, proc: Process | undefined): MemorySnapshot | null {
  if (!proc || proc.mem === null) return null;
  return trace.snapshots[proc.mem] ?? null;
}

export interface TraceIndex {
  // Tono de cada proceso (0..7), en orden de aparición.
  hue: Map<number, number>;
  // Padre genealógico (quien hizo fork), estable aunque el proceso quede huérfano.
  parent: Map<number, number>;
  // Número de orden de cada hilo dentro de su proceso (0 = principal).
  threadOrder: Map<number, number>;
  mutexHue: Map<string, number>;
  lines: string[];
}

export function indexTrace(trace: Trace): TraceIndex {
  const hue = new Map<number, number>();
  const parent = new Map<number, number>();
  const threadOrder = new Map<number, number>();
  const perProcess = new Map<number, number>();
  const mutexHue = new Map<string, number>();
  for (const step of trace.steps) {
    for (const p of step.processes) {
      if (!hue.has(p.pid)) hue.set(p.pid, hue.size % PROC_HUES);
      for (const th of p.threads) {
        if (threadOrder.has(th.tid)) continue;
        const n = perProcess.get(p.pid) ?? 0;
        threadOrder.set(th.tid, n);
        perProcess.set(p.pid, n + 1);
      }
    }
    for (const ev of step.events) if (ev.type === 'fork') parent.set(ev.child, ev.parent);
    for (const s of step.sync) if (s.kind === 'mutex' && !mutexHue.has(s.id)) mutexHue.set(s.id, mutexHue.size % 4);
  }
  return { hue, parent, threadOrder, mutexHue, lines: trace.source.split('\n') };
}

export function threadLabel(index: TraceIndex, tid: number): string {
  const n = index.threadOrder.get(tid) ?? 0;
  return n === 0 ? 'principal' : `T${n}`;
}

// Extracto corto de una línea del código, para los nodos de los carriles.
export function excerpt(index: TraceIndex, line: number | null | undefined, max = 22): string {
  if (!line) return '';
  const text = (index.lines[line - 1] ?? '').trim();
  return text.length > max ? text.slice(0, max - 1) + '…' : text;
}

// ---------- navegación ----------

const QUIET: Event['type'][] = ['call', 'return'];

export function isNotable(ev: Event): boolean {
  return !QUIET.includes(ev.type);
}

export function nextEventStep(trace: Trace, t: number): number {
  for (let s = t + 1; s < trace.steps.length; s++) if (trace.steps[s].events.some(isNotable)) return s;
  return trace.steps.length - 1;
}

export function prevEventStep(trace: Trace, t: number): number {
  for (let s = t - 1; s > 0; s--) if (trace.steps[s].events.some(isNotable)) return s;
  return 0;
}

export function nextThreadStep(trace: Trace, t: number, tid: number): number {
  for (let s = t + 1; s < trace.steps.length; s++) if (trace.steps[s].actor?.tid === tid) return s;
  return t;
}

// Avanza hasta que algún hilo quede detenido en `line` (a punto de ejecutarla).
export function runToLine(trace: Trace, t: number, line: number): number {
  for (let s = t + 1; s < trace.steps.length; s++) {
    const step = trace.steps[s];
    if (!step.actor) continue;
    if (threadAt(step, step.actor.pid, step.actor.tid)?.line === line) return s;
  }
  return t;
}

export function outputUpTo(trace: Trace, t: number, pid?: number) {
  return trace.output.filter((c) => c.t <= t && (pid === undefined || c.pid === pid));
}

// Procesos que existen en el paso t, incluidos zombies y recogidos (conservan su lugar en el árbol).
export function visibleProcesses(step: Step): Process[] {
  return step.processes;
}
