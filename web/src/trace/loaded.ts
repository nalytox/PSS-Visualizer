// Una traza cargada, de la versión 1 (todos los pasos completos) o de la 2 (compacta, ver
// schema/compact.schema.json). El resto de la interfaz no distingue: pide el paso t con step(t),
// las instantáneas con snapshot(id) y usa las columnas por paso para recorrer la traza entera sin
// abrir cada paso. De una traza compacta solo se descomprimen los bloques que se miran.
import { gunzipSync } from 'fflate';
import type { CompactTrace } from './compact-types.ts';
import type { Event, MemorySnapshot, Step, Trace } from './types.ts';

// Mismo orden que trace_model::compact::FLAGS.
export const FLAGS = [
  'call',
  'return',
  'fork',
  'threadCreate',
  'exec',
  'exitProcess',
  'exitThread',
  'wait',
  'waitReaped',
  'join',
  'reparent',
  'pipe',
  'dup',
  'close',
  'read',
  'write',
  'writePipe',
  'block',
  'unblock',
  'signalSend',
  'signalDeliver',
  'signalReturn',
  'mutex',
  'mutexAcquired',
  'mutexReleased',
  'cond',
  'sem',
  'malloc',
  'free',
  'memError',
  'deadlock',
  'other',
] as const;

export type Flag = (typeof FLAGS)[number];

export function flag(name: Flag): number {
  return (1 << FLAGS.indexOf(name)) >>> 0;
}

export function eventFlags(events: Event[]): number {
  let f = 0;
  for (const ev of events) {
    switch (ev.type) {
      case 'exit':
        f |= flag(ev.scope === 'process' ? 'exitProcess' : 'exitThread');
        break;
      case 'wait':
        f |= flag('wait') | (ev.reaped !== undefined ? flag('waitReaped') : 0);
        break;
      case 'write':
        f |= flag('write') | (ev.pipe ? flag('writePipe') : 0);
        break;
      case 'mutex':
        f |= flag('mutex') | (ev.result === 'acquired' ? flag('mutexAcquired') : ev.result === 'released' ? flag('mutexReleased') : 0);
        break;
      case 'stdinNeeded':
      case 'truncated':
        f |= flag('other');
        break;
      default:
        f |= flag(ev.type);
    }
  }
  return f >>> 0;
}

export interface StepColumns {
  actorPid: Uint32Array; // 0 = sin actor
  actorTid: Uint32Array;
  actorLine: Uint32Array; // próxima línea del actor; 0 = ninguna
  flags: Uint32Array;
}

export interface Appearance {
  pids: number[];
  threads: { pid: number; tid: number }[];
  forks: { parent: number; child: number }[];
  mutexes: string[];
}

type Header = Omit<Trace, 'version' | 'steps' | 'snapshots'>;

interface DecodedChunk {
  steps: Step[];
  defs: Record<string, unknown>;
  snaps: Map<string, MemorySnapshot>;
}

// Bloques descomprimidos que se guardan a la vez: los alrededor del paso actual.
const CACHED_CHUNKS = 8;

export class LoadedTrace implements Header {
  readonly version: 1 | 2;
  readonly arch: Trace['arch'];
  readonly source: string;
  readonly stdin: string;
  readonly run: Trace['run'];
  readonly compile: Trace['compile'];
  readonly outcome: Trace['outcome'];
  readonly truncated: boolean;
  readonly truncatedReason?: Trace['truncatedReason'];
  readonly output: Trace['output'];
  readonly summary: Trace['summary'];
  readonly length: number;
  readonly columns: StepColumns;
  readonly appearance: Appearance;

  private readonly full: Trace | null;
  private readonly compact: CompactTrace | null;
  private readonly cache = new Map<number, DecodedChunk>();

  constructor(data: Trace | CompactTrace) {
    this.version = data.version;
    this.arch = data.arch;
    this.source = data.source;
    this.stdin = data.stdin;
    this.run = data.run;
    this.compile = data.compile;
    this.outcome = data.outcome;
    this.truncated = data.truncated;
    if (data.truncatedReason !== undefined) this.truncatedReason = data.truncatedReason;
    this.output = data.output;
    this.summary = data.summary;
    if (data.version === 2) {
      this.full = null;
      this.compact = data;
      this.length = data.stepCount;
      const ix = data.index;
      this.columns = {
        actorPid: column(ix.actorPid, data.stepCount),
        actorTid: column(ix.actorTid, data.stepCount),
        actorLine: column(ix.actorLine, data.stepCount),
        flags: column(ix.flags, data.stepCount),
      };
      this.appearance = { pids: ix.pids, threads: ix.threads, forks: ix.forks, mutexes: ix.mutexes };
    } else {
      this.full = data;
      this.compact = null;
      this.length = data.steps.length;
      this.columns = columnsOf(data.steps);
      this.appearance = appearanceOf(data.steps);
    }
  }

  get last(): number {
    return this.length - 1;
  }

  step(t: number): Step {
    if (this.full) return this.full.steps[t];
    const c = chunkOfStep(this.compact!, t);
    const chunk = this.chunk(c);
    return chunk.steps[t - this.compact!.chunks[c].start];
  }

  snapshot(id: string | null | undefined): MemorySnapshot | null {
    if (!id) return null;
    if (this.full) return this.full.snapshots[id] ?? null;
    const n = Number(id.slice(1));
    const chunks = this.compact!.chunks;
    // El último bloque cuyas instantáneas empiezan en n o antes.
    let lo = 0;
    let hi = chunks.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (chunks[mid].firstSnapshot <= n) lo = mid;
      else hi = mid - 1;
    }
    return chunks.length ? resolveSnapshot(this.chunk(lo), id) : null;
  }

  private chunk(c: number): DecodedChunk {
    const hit = this.cache.get(c);
    if (hit) {
      this.cache.delete(c);
      this.cache.set(c, hit);
      return hit;
    }
    const decoded = decodeChunk(this.compact!.chunks[c].data);
    this.cache.set(c, decoded);
    if (this.cache.size > CACHED_CHUNKS) this.cache.delete(this.cache.keys().next().value!);
    return decoded;
  }
}

export function loadTrace(data: Trace | CompactTrace): LoadedTrace {
  return new LoadedTrace(data);
}

function chunkOfStep(c: CompactTrace, t: number): number {
  let lo = 0;
  let hi = c.chunks.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (c.chunks[mid].start <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function columnsOf(steps: Step[]): StepColumns {
  const n = steps.length;
  const cols: StepColumns = {
    actorPid: new Uint32Array(n),
    actorTid: new Uint32Array(n),
    actorLine: new Uint32Array(n),
    flags: new Uint32Array(n),
  };
  steps.forEach((s, t) => {
    if (s.actor) {
      cols.actorPid[t] = s.actor.pid;
      cols.actorTid[t] = s.actor.tid;
      const p = s.processes.find((x) => x.pid === s.actor!.pid);
      cols.actorLine[t] = p?.threads.find((th) => th.tid === s.actor!.tid)?.line ?? 0;
    }
    cols.flags[t] = eventFlags(s.events);
  });
  return cols;
}

function appearanceOf(steps: Step[]): Appearance {
  const out: Appearance = { pids: [], threads: [], forks: [], mutexes: [] };
  const pids = new Set<number>();
  const tids = new Set<string>();
  const mutexes = new Set<string>();
  for (const s of steps) {
    for (const p of s.processes) {
      if (!pids.has(p.pid)) {
        pids.add(p.pid);
        out.pids.push(p.pid);
      }
      for (const th of p.threads) {
        const key = `${p.pid}:${th.tid}`;
        if (!tids.has(key)) {
          tids.add(key);
          out.threads.push({ pid: p.pid, tid: th.tid });
        }
      }
    }
    for (const ev of s.events) if (ev.type === 'fork') out.forks.push({ parent: ev.parent, child: ev.child });
    for (const o of s.sync) {
      if (o.kind === 'mutex' && !mutexes.has(o.id)) {
        mutexes.add(o.id);
        out.mutexes.push(o.id);
      }
    }
  }
  return out;
}

// ---------- decodificación ----------

function base64(text: string): Uint8Array {
  const fromBase64 = (Uint8Array as unknown as { fromBase64?: (s: string) => Uint8Array }).fromBase64;
  if (fromBase64) return fromBase64(text);
  const bin = atob(text);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function column(text: string, n: number): Uint32Array {
  const bytes = gunzipSync(base64(text));
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const out = new Uint32Array(n);
  for (let i = 0; i < n && i * 4 + 3 < bytes.length; i++) out[i] = view.getUint32(i * 4, true);
  return out;
}

const utf8 = new TextDecoder();

function decodeChunk(data: string): DecodedChunk {
  const body = JSON.parse(utf8.decode(gunzipSync(base64(data)))) as { steps: unknown[]; snapshots: Record<string, unknown> };
  const steps: Step[] = [];
  let prev: Record<string, unknown> | null = null;
  for (const entry of body.steps) {
    const step = (prev === null ? entry : applyPatch({ ...prev, events: [] }, entry)) as Record<string, unknown>;
    steps.push(step as unknown as Step);
    prev = step;
  }
  return { steps, defs: body.snapshots, snaps: new Map() };
}

function resolveSnapshot(c: DecodedChunk, id: string): MemorySnapshot | null {
  // La cadena de parches hasta una instantánea completa o ya resuelta.
  const chain: string[] = [];
  let base: unknown = undefined;
  for (let cur: string | undefined = id; cur !== undefined; ) {
    const hit = c.snaps.get(cur);
    if (hit) {
      base = hit;
      break;
    }
    const def = c.defs[cur] as Record<string, unknown> | undefined;
    if (!def) return null;
    chain.push(cur);
    cur = typeof def.$from === 'string' ? def.$from : undefined;
  }
  for (let k = chain.length - 1; k >= 0; k--) {
    const def = c.defs[chain[k]] as Record<string, unknown>;
    const value = (typeof def.$from === 'string' ? applyPatch(base, def.$p) : def) as MemorySnapshot;
    c.snaps.set(chain[k], value);
    base = value;
  }
  return c.snaps.get(id) ?? null;
}

// Aplica un parche de trace_model::delta. Lo que no cambia se comparte con el valor anterior.
export function applyPatch(a: unknown, p: unknown): unknown {
  if (p === null || typeof p !== 'object' || Array.isArray(p)) return p;
  const pm = p as Record<string, unknown>;
  if ('$s' in pm) return pm.$s;
  if (Array.isArray(a)) {
    const out = a.slice();
    if (typeof pm.$a === 'number') out.length = pm.$a;
    for (const k in pm) {
      if (k === '$a') continue;
      const i = Number(k);
      out[i] = applyPatch(out[i], pm[k]);
    }
    return out;
  }
  if (a !== null && typeof a === 'object') {
    const out: Record<string, unknown> = { ...(a as Record<string, unknown>) };
    for (const k in pm) {
      const v = pm[k];
      if (isDeleted(v)) delete out[k];
      else out[k] = applyPatch(out[k], v);
    }
    return out;
  }
  return p;
}

function isDeleted(v: unknown): boolean {
  if (v === null || typeof v !== 'object' || Array.isArray(v)) return false;
  const keys = Object.keys(v);
  return keys.length === 1 && keys[0] === '$d';
}
