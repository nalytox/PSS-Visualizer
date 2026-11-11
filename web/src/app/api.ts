// Cliente del servidor local (crates/server).
import type { CompactTrace } from '../trace/compact-types.ts';
import { loadTrace, type LoadedTrace } from '../trace/loaded.ts';
import type { TaskRef, Trace } from '../trace/types.ts';

type Policy = Trace['run']['policy'];

export interface RunRequest {
  source: string;
  stdin: string;
  stdinEof: boolean;
  injections?: { t: number; signal: string }[];
  policy?: Policy;
  seed?: number;
  schedule?: TaskRef[];
}

export interface RunCallbacks {
  // Identificador de la ejecución, para detenerla con stopRun.
  onStart?: (id: string) => void;
  // Pasos recorridos hasta ahora.
  onProgress?: (steps: number) => void;
  signal?: AbortSignal;
}

export async function serverAvailable(): Promise<boolean> {
  try {
    const res = await fetch('/api/health', { cache: 'no-store' });
    if (!res.ok) return false;
    const body = await res.json();
    return body.ok === true && body.canRun === true;
  } catch {
    return false;
  }
}

// Tramos de la misma tarea: una planificación manual larga viaja comprimida.
export function compressSchedule(list: TaskRef[]): (TaskRef & { n: number })[] {
  const out: (TaskRef & { n: number })[] = [];
  for (const r of list) {
    const last = out.at(-1);
    if (last && last.pid === r.pid && last.tid === r.tid) last.n++;
    else out.push({ pid: r.pid, tid: r.tid, n: 1 });
  }
  return out;
}

// Ejecuta un programa. La respuesta llega por líneas (NDJSON): el id, el avance y al final la traza.
export async function runProgram(req: RunRequest, cb: RunCallbacks = {}): Promise<LoadedTrace> {
  const body = { ...req, schedule: req.schedule ? compressSchedule(req.schedule) : undefined };
  const res = await fetch('/api/run/stream', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
    signal: cb.signal,
  });
  if (!res.ok || !res.body) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? `error ${res.status}`);
  }
  const reader = res.body.pipeThrough(new TextDecoderStream()).getReader();
  // La última línea (la traza) puede pesar decenas de MB: se junta por partes.
  let pending: string[] = [];
  const handle = (line: string): LoadedTrace | null => {
    if (!line.trim()) return null;
    const msg = JSON.parse(line) as
      | { type: 'started'; id: string }
      | { type: 'progress'; steps: number }
      | { type: 'trace'; trace: Trace | CompactTrace }
      | { type: 'error'; error: string };
    switch (msg.type) {
      case 'started':
        cb.onStart?.(msg.id);
        return null;
      case 'progress':
        cb.onProgress?.(msg.steps);
        return null;
      case 'trace':
        return loadTrace(msg.trace);
      case 'error':
        throw new Error(msg.error || 'el tracer falló');
    }
  };
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    let rest = value;
    for (let nl = rest.indexOf('\n'); nl >= 0; nl = rest.indexOf('\n')) {
      pending.push(rest.slice(0, nl));
      const line = pending.join('');
      pending = [];
      rest = rest.slice(nl + 1);
      const trace = handle(line);
      if (trace) return trace;
    }
    if (rest) pending.push(rest);
  }
  const trace = handle(pending.join(''));
  if (trace) return trace;
  throw new Error('la ejecución terminó sin traza');
}

// Pide detener la ejecución: el servidor responde igual con la traza de lo recorrido.
export async function stopRun(id: string): Promise<void> {
  await fetch(`/api/run/${encodeURIComponent(id)}/stop`, { method: 'POST' }).catch(() => undefined);
}
