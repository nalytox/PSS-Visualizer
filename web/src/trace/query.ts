// Consultas sobre la traza. Todo se deriva del paso actual; nada de esto muta la traza.
import { flag, type Flag, type LoadedTrace } from './loaded.ts';
import type { MemorySnapshot, Process, Step, TaskRef, Thread } from './types.ts';

export const PROC_HUES = 8;

export function processAt(step: Step, pid: number): Process | undefined {
  return step.processes.find((p) => p.pid === pid);
}

export function threadAt(step: Step, pid: number, tid: number): Thread | undefined {
  return processAt(step, pid)?.threads.find((th) => th.tid === tid);
}

export function snapshotOf(trace: LoadedTrace, proc: Process | undefined): MemorySnapshot | null {
  if (!proc || proc.mem === null) return null;
  return trace.snapshot(proc.mem);
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

export function indexTrace(trace: LoadedTrace): TraceIndex {
  const hue = new Map<number, number>();
  const parent = new Map<number, number>();
  const threadOrder = new Map<number, number>();
  const perProcess = new Map<number, number>();
  const mutexHue = new Map<string, number>();
  const seen = trace.appearance;
  for (const pid of seen.pids) if (!hue.has(pid)) hue.set(pid, hue.size % PROC_HUES);
  for (const { pid, tid } of seen.threads) {
    if (threadOrder.has(tid)) continue;
    const n = perProcess.get(pid) ?? 0;
    threadOrder.set(tid, n);
    perProcess.set(pid, n + 1);
  }
  for (const f of seen.forks) parent.set(f.child, f.parent);
  for (const id of seen.mutexes) if (!mutexHue.has(id)) mutexHue.set(id, mutexHue.size % 4);
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

// Todo evento cuenta salvo las llamadas y retornos, que ocurren en casi cada paso.
const QUIET = flag('call') | flag('return');

export function hasFlag(trace: LoadedTrace, t: number, name: Flag): boolean {
  return (trace.columns.flags[t] & flag(name)) !== 0;
}

export function nextEventStep(trace: LoadedTrace, t: number): number {
  const flags = trace.columns.flags;
  for (let s = t + 1; s < trace.length; s++) if ((flags[s] & ~QUIET) !== 0) return s;
  return trace.length - 1;
}

export function prevEventStep(trace: LoadedTrace, t: number): number {
  const flags = trace.columns.flags;
  for (let s = t - 1; s > 0; s--) if ((flags[s] & ~QUIET) !== 0) return s;
  return 0;
}

export function nextThreadStep(trace: LoadedTrace, t: number, tid: number): number {
  const tids = trace.columns.actorTid;
  for (let s = t + 1; s < trace.length; s++) if (tids[s] === tid) return s;
  return t;
}

// Avanza hasta que algún hilo quede detenido en `line` (a punto de ejecutarla).
export function runToLine(trace: LoadedTrace, t: number, line: number): number {
  const { actorPid, actorLine } = trace.columns;
  for (let s = t + 1; s < trace.length; s++) if (actorPid[s] !== 0 && actorLine[s] === line) return s;
  return t;
}

// Quién avanzó en cada paso hasta t (los pasos del kernel no cuentan).
export function actorsUpTo(trace: LoadedTrace, t: number): TaskRef[] {
  const { actorPid, actorTid } = trace.columns;
  const out: TaskRef[] = [];
  for (let s = 1; s <= t && s < trace.length; s++) if (actorPid[s] !== 0) out.push({ pid: actorPid[s], tid: actorTid[s] });
  return out;
}

export function outputUpTo(trace: LoadedTrace, t: number, pid?: number) {
  return trace.output.filter((c) => c.t <= t && (pid === undefined || c.pid === pid));
}

// Procesos que existen en el paso t, incluidos zombies y recogidos (conservan su lugar en el árbol).
export function visibleProcesses(step: Step): Process[] {
  return step.processes;
}
