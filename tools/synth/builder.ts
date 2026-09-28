// Constructor de trazas sintéticas. Mantiene un estado mutable del "sistema" y, en cada paso,
// congela una copia completa. Solo se usa para escribir trazas a mano (fase 0) y en pruebas.
import type {
  BlockReason,
  Event,
  EventJoin,
  EventRead,
  EventSignalDeliver,
  Fd,
  Frame,
  HeapBlock,
  InFlightSignal,
  Leak,
  Limits,
  MemorySnapshot,
  OutputChunk,
  Pipe,
  Process,
  SignalName,
  Step,
  SyncObject,
  TaskRef,
  Thread,
  Trace,
  Value,
  Var,
} from '../../web/src/trace/types.ts';

export const DEFAULT_LIMITS: Limits = {
  maxProcesses: 32,
  maxThreadsPerProcess: 16,
  maxSteps: 5000,
  wallTimeMs: 10000,
  memoryBytes: 268435456,
  outputBytesPerProcess: 65536,
  traceBytesCompressed: 20971520,
};

// Cuántos pasos sigue visible un bloque liberado antes de retirarlo del heap.
const FREED_VISIBLE_STEPS = 3;

// ---------- valores ----------

export const scalar = (value: number | string | boolean, repr?: string): Value =>
  repr === undefined ? { kind: 'scalar', value } : { kind: 'scalar', value, repr };

export const ptr = (target: string | null, fn?: string): Value =>
  fn === undefined ? { kind: 'pointer', target } : { kind: 'pointer', target, fn };

export const opaque = (note: string): Value => ({ kind: 'opaque', note });

export const hex = (n: number): string => '0x' + n.toString(16);

// Texto → bytes UTF-8 en la representación de la traza (un carácter U+0000–U+00FF por byte),
// que es lo que realmente viaja por write() cuando un programa imprime "ó".
export function utf8(text: string): string {
  return String.fromCharCode(...new TextEncoder().encode(text));
}

export function charArray(len: number, text: string): Value {
  const raw = utf8(text);
  const items: Value[] = [];
  for (let i = 0; i < len; i++) {
    const code = i < raw.length ? raw.charCodeAt(i) : 0;
    items.push(scalar(code, charRepr(code)));
  }
  return { kind: 'array', length: len, items, text: raw };
}

function charRepr(code: number): string {
  const esc: Record<number, string> = { 0: '\\0', 10: '\\n', 9: '\\t', 13: '\\r' };
  return "'" + (esc[code] ?? String.fromCharCode(code)) + "'";
}

export function intArray(values: number[]): Value {
  return { kind: 'array', length: values.length, items: values.map((v) => scalar(v)) };
}

export interface FieldSpec {
  name: string;
  type: string;
  size: number;
  value: (addr: number) => Value;
  uninit?: boolean;
  offset?: number; // por defecto, justo después del campo anterior
}

export function struct(base: number, fields: FieldSpec[]): Value {
  let off = 0;
  return {
    kind: 'struct',
    fields: fields.map((f) => {
      const addr = base + (f.offset ?? off);
      off = (f.offset ?? off) + f.size;
      const field = { name: f.name, type: f.type, addr: hex(addr), size: f.size, value: f.value(addr) };
      return f.uninit ? { ...field, uninit: true as const } : field;
    }),
  };
}

export function variable(name: string, type: string, addr: number, size: number, value: Value, uninit = false): Var {
  const v: Var = { name, type, addr: hex(addr), size, value };
  if (uninit) v.uninit = true;
  return v;
}

// ---------- constructor ----------

interface Options {
  source: string;
  stdin?: string;
  stdinEof?: boolean;
  policy?: 'round_robin' | 'random' | 'manual';
  seed?: number;
}

export class TraceBuilder {
  private source: string;
  private opts: Options;
  private t = 0;
  private clock = 0;
  private steps: Step[] = [];
  private procs: Process[] = [];
  private mems = new Map<number, MemorySnapshot>();
  private pipes: Pipe[] = [];
  private sync: SyncObject[] = [];
  private inflight: InFlightSignal[] = [];
  private output: OutputChunk[] = [];
  private stdinState = { size: 0, consumed: 0, eof: false };
  private events: Event[] = [];
  private snapshots: Record<string, MemorySnapshot> = {};
  private snapshotIds = new Map<string, string>();
  private pipeCount = 0;
  private leaks: Leak[] = [];

  constructor(opts: Options) {
    this.opts = opts;
    this.source = opts.source;
    const stdin = opts.stdin ?? '';
    this.stdinState = { size: stdin.length, consumed: 0, eof: opts.stdinEof ?? true };
  }

  // Número de línea (1-based) de la n-ésima línea que contiene el fragmento.
  L(snippet: string, nth = 1): number {
    const lines = this.source.split('\n');
    let seen = 0;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].includes(snippet) && ++seen === nth) return i + 1;
    }
    throw new Error(`línea no encontrada: ${snippet} (#${nth})`);
  }

  get now(): number {
    return this.t;
  }

  // ----- acceso al estado -----

  proc(pid: number): Process {
    const p = this.procs.find((x) => x.pid === pid);
    if (!p) throw new Error(`no existe el proceso ${pid}`);
    return p;
  }

  thread(pid: number, tid: number): Thread {
    const th = this.proc(pid).threads.find((x) => x.tid === tid);
    if (!th) throw new Error(`no existe el hilo ${tid} en ${pid}`);
    return th;
  }

  mem(pid: number): MemorySnapshot {
    const m = this.mems.get(pid);
    if (!m) throw new Error(`el proceso ${pid} no tiene memoria`);
    return m;
  }

  frame(pid: number, tid: number): Frame {
    const f = this.mem(pid).stacks[String(tid)]?.[0];
    if (!f) throw new Error(`el hilo ${tid} no tiene frames`);
    return f;
  }

  local(pid: number, tid: number, name: string): Var {
    const f = this.frame(pid, tid);
    const v = [...f.params, ...f.locals].find((x) => x.name === name);
    if (!v) throw new Error(`no existe la variable ${name} en ${f.fn}`);
    return v;
  }

  global(pid: number, name: string): Var {
    const v = this.mem(pid).globals.find((x) => x.name === name);
    if (!v) throw new Error(`no existe la global ${name}`);
    return v;
  }

  heapBlock(pid: number, addr: number): HeapBlock {
    const b = this.mem(pid).heap.find((x) => x.addr === hex(addr));
    if (!b) throw new Error(`no existe el bloque ${hex(addr)}`);
    return b;
  }

  // Las variables aparecen en el frame cuando la ejecución llega a su declaración.
  declare(pid: number, tid: number, v: Var): Var {
    this.frame(pid, tid).locals.push(v);
    return v;
  }

  // Al salir de un bloque { } sus variables dejan de estar en el alcance.
  undeclare(pid: number, tid: number, name: string): void {
    const f = this.frame(pid, tid);
    f.locals = f.locals.filter((x) => x.name !== name);
  }

  set(v: Var, value: Value): void {
    v.value = value;
    delete v.uninit;
  }

  emit(ev: Event): void {
    this.events.push(ev);
  }

  // ----- creación -----

  spawn(pid: number, opts: { line: number; locals?: Var[]; params?: Var[]; globals?: Var[]; path?: string }): void {
    this.procs.push({
      pid,
      ppid: null,
      pgid: pid,
      state: 'running',
      createdAt: 0,
      image: { kind: 'user', path: opts.path ?? 'prog' },
      fds: { '0': { kind: 'stdin' }, '1': { kind: 'terminal' }, '2': { kind: 'terminal' } },
      signals: { mask: [], pending: [], actions: {} },
      threads: [{ tid: pid, main: true, state: 'running', line: opts.line, fn: 'main', holds: [] }],
      mem: null,
    });
    this.mems.set(pid, {
      globals: opts.globals ?? [],
      stacks: { [String(pid)]: [{ fn: 'main', line: opts.line, params: opts.params ?? [], locals: opts.locals ?? [] }] },
      heap: [],
    });
  }

  // Estado inicial (t = 0): detenido en la primera línea de main.
  initial(): void {
    this.commit(null, undefined, []);
  }

  // Un paso de la tarea (pid, tid). `next` es la línea que queda por ejecutar; undefined = no mover.
  step(
    pid: number,
    tid: number,
    next: number | undefined,
    body?: () => void,
    opts: { executed?: boolean } = {},
  ): void {
    const choices = this.runnable();
    const th = this.thread(pid, tid);
    const executed =
      opts.executed === false || th.line === null || th.fn === null ? undefined : { line: th.line, fn: th.fn };
    this.t++;
    body?.();
    if (next !== undefined) this.moveTo(pid, tid, next);
    this.commit({ pid, tid }, executed, choices);
  }

  // Paso sin actor: el kernel hace algo (vence un temporizador, avanza el reloj).
  kernelStep(body: () => void, clock?: number): void {
    const choices = this.runnable();
    this.t++;
    if (clock !== undefined) this.clock = clock;
    body();
    this.commit(null, undefined, choices);
  }

  moveTo(pid: number, tid: number, line: number): void {
    const th = this.thread(pid, tid);
    th.line = line;
    const stack = this.mems.get(pid)?.stacks[String(tid)];
    if (stack && stack[0]) stack[0].line = line;
  }

  // ----- funciones del usuario -----

  pushFrame(pid: number, tid: number, fn: string, line: number, params: Var[], locals: Var[], signal?: SignalName): void {
    const stacks = this.mem(pid).stacks;
    const key = String(tid);
    const frame: Frame = { fn, line, params, locals };
    if (signal) frame.signal = signal;
    stacks[key] = [frame, ...(stacks[key] ?? [])];
    const th = this.thread(pid, tid);
    th.fn = fn;
    th.line = line;
  }

  popFrame(pid: number, tid: number, returnValue?: Value): Frame {
    const stacks = this.mem(pid).stacks;
    const [top, ...rest] = stacks[String(tid)];
    stacks[String(tid)] = rest;
    const th = this.thread(pid, tid);
    th.fn = rest[0]?.fn ?? null;
    th.line = rest[0]?.line ?? null;
    this.emit(returnValue ? { type: 'return', fn: top.fn, value: returnValue } : { type: 'return', fn: top.fn });
    return top;
  }

  call(fn: string, summary?: string): void {
    this.emit(summary ? { type: 'call', fn, summary } : { type: 'call', fn });
  }

  // ----- salida -----

  print(pid: number, tid: number, text: string, fd = 1): void {
    const stream = fd === 2 ? 'stderr' : 'stdout';
    const bytes = utf8(text);
    this.output.push({ t: this.t, pid, fd, stream, bytes });
    this.emit({ type: 'write', pid, tid, fd, terminal: true, bytes, n: bytes.length });
  }

  // ----- heap -----

  malloc(pid: number, addr: number, size: number, type: string, value: Value, line: number, fn = 'malloc'): void {
    this.mem(pid).heap.push({ addr: hex(addr), size, type, allocAt: this.t, allocLine: line, value });
    this.emit({ type: 'malloc', pid, fn, addr: hex(addr), size });
  }

  free(pid: number, addr: number): void {
    this.heapBlock(pid, addr).freedAt = this.t;
    this.emit({ type: 'free', pid, addr: hex(addr) });
  }

  // ----- procesos -----

  fork(parent: number, child: number): void {
    const p = this.proc(parent);
    const caller = p.threads.find((x) => x.state === 'running' || x.state === 'ready') ?? p.threads[0];
    const c: Process = structuredClone(p);
    c.pid = child;
    c.ppid = parent;
    c.createdAt = this.t;
    c.signals.pending = [];
    // El hijo solo hereda el hilo que llamó a fork.
    c.threads = [{ ...structuredClone(caller), tid: child, main: true, holds: [] }];
    this.procs.push(c);
    const m = structuredClone(this.mem(parent));
    m.stacks = { [String(child)]: m.stacks[String(caller.tid)] ?? [] };
    this.mems.set(child, m);
    for (const pipe of this.pipes) {
      for (const [fd, entry] of Object.entries(c.fds)) {
        if (entry.kind !== 'pipe' || entry.pipe !== pipe.id) continue;
        (entry.end === 'r' ? pipe.readers : pipe.writers).push({ pid: child, fd: Number(fd) });
      }
    }
    this.emit({ type: 'fork', parent, child });
  }

  exit(pid: number, code: number): void {
    const p = this.proc(pid);
    for (const b of this.mems.get(pid)?.heap ?? []) {
      if (b.freedAt !== undefined) continue;
      this.leaks.push({ pid, addr: b.addr, size: b.size, ...(b.type ? { type: b.type } : {}), allocAt: b.allocAt, allocLine: b.allocLine });
    }
    for (const fd of Object.keys(p.fds)) this.dropFd(pid, Number(fd));
    p.fds = {};
    p.state = 'zombie';
    p.exit = { code };
    // Huérfanos: init (1) adopta a los hijos vivos y recoge de inmediato a los zombies.
    for (const c of this.procs) {
      if (c.ppid !== pid || c.state === 'reaped') continue;
      this.emit({ type: 'reparent', pid: c.pid, from: pid, to: 1 });
      c.ppid = 1;
      if (c.state === 'zombie') {
        c.state = 'reaped';
        this.mems.delete(c.pid);
      }
    }
    for (const th of p.threads) {
      th.state = 'exited';
      delete th.blockedOn;
      delete th.inCall;
    }
    this.emit({ type: 'exit', pid, scope: 'process', code });
  }

  reap(parent: number, child: number, target = -1): void {
    const c = this.proc(child);
    const status = c.exit ?? { code: 0 };
    c.state = 'reaped';
    this.mems.delete(child);
    this.emit({ type: 'wait', pid: parent, target, reaped: child, status });
  }

  // ----- bloqueo -----

  block(pid: number, tid: number, reason: BlockReason, inCall?: string): void {
    const th = this.thread(pid, tid);
    th.state = 'blocked';
    th.blockedOn = reason;
    if (inCall) th.inCall = inCall;
    this.emit({ type: 'block', pid, tid, reason });
  }

  unblock(pid: number, tid: number): void {
    const th = this.thread(pid, tid);
    th.state = 'ready';
    delete th.blockedOn;
    this.emit({ type: 'unblock', pid, tid });
  }

  // Termina la llamada a libc en curso (el hilo ya no está "dentro" de read, wait, etc.).
  endCall(pid: number, tid: number): void {
    delete this.thread(pid, tid).inCall;
  }

  // ----- pipes y fds -----

  pipe(pid: number, rfd: number, wfd: number): string {
    const id = `p${this.pipeCount++}`;
    const p = this.proc(pid);
    p.fds[String(rfd)] = { kind: 'pipe', pipe: id, end: 'r' };
    p.fds[String(wfd)] = { kind: 'pipe', pipe: id, end: 'w' };
    this.pipes.push({
      id,
      createdBy: pid,
      size: 0,
      buffer: '',
      capacity: 65536,
      readers: [{ pid, fd: rfd }],
      writers: [{ pid, fd: wfd }],
      warnings: [],
    });
    this.emit({ type: 'pipe', pid, pipe: id, fds: [rfd, wfd] });
    return id;
  }

  pipeById(id: string): Pipe {
    const p = this.pipes.find((x) => x.id === id);
    if (!p) throw new Error(`no existe el pipe ${id}`);
    return p;
  }

  close(pid: number, fd: number): void {
    const was = this.proc(pid).fds[String(fd)];
    if (!was) throw new Error(`fd ${fd} no está abierto en ${pid}`);
    this.dropFd(pid, fd);
    delete this.proc(pid).fds[String(fd)];
    this.emit({ type: 'close', pid, fd, was });
  }

  private dropFd(pid: number, fd: number): void {
    const entry: Fd | undefined = this.proc(pid).fds[String(fd)];
    if (!entry || entry.kind !== 'pipe') return;
    const pipe = this.pipeById(entry.pipe);
    const keep = (e: { pid: number; fd: number }) => !(e.pid === pid && e.fd === fd);
    pipe.readers = pipe.readers.filter(keep);
    pipe.writers = pipe.writers.filter(keep);
    pipe.warnings = pipe.warnings.filter((w) => !(w.pid === pid && w.fd === fd));
    if (pipe.readers.length === 0 && pipe.writers.length === 0) {
      this.pipes = this.pipes.filter((x) => x !== pipe);
    }
  }

  writePipe(pid: number, tid: number, fd: number, text: string): void {
    const bytes = utf8(text);
    const entry = this.proc(pid).fds[String(fd)];
    if (entry?.kind !== 'pipe') throw new Error(`fd ${fd} no es un pipe`);
    const pipe = this.pipeById(entry.pipe);
    pipe.size += bytes.length;
    pipe.buffer = (pipe.buffer + bytes).slice(0, 256);
    this.emit({ type: 'write', pid, tid, fd, pipe: pipe.id, bytes, n: bytes.length });
  }

  readPipe(pid: number, tid: number, fd: number, max: number, into?: number): string {
    const entry = this.proc(pid).fds[String(fd)];
    if (entry?.kind !== 'pipe') throw new Error(`fd ${fd} no es un pipe`);
    const pipe = this.pipeById(entry.pipe);
    const bytes = pipe.buffer.slice(0, max);
    pipe.buffer = pipe.buffer.slice(bytes.length);
    pipe.size -= bytes.length;
    const ev: EventRead = { type: 'read', pid, tid, fd, pipe: pipe.id, bytes, n: bytes.length, eof: bytes.length === 0 };
    if (into !== undefined) ev.into = hex(into);
    this.emit(ev);
    return bytes;
  }

  // ----- hilos y sincronización -----

  createThread(pid: number, creator: number, tid: number, fn: string, arg: Value, line: number, params: Var[], locals: Var[]): void {
    const p = this.proc(pid);
    p.threads.push({ tid, main: false, state: 'ready', line, fn, start: { fn, arg }, holds: [] });
    this.mem(pid).stacks[String(tid)] = [{ fn, line, params, locals }];
    this.emit({ type: 'threadCreate', pid, creator, tid, fn, arg });
  }

  threadExit(pid: number, tid: number, retval: Value): void {
    const th = this.thread(pid, tid);
    th.state = 'exited';
    th.retval = retval;
    th.line = null;
    th.fn = null;
    this.mem(pid).stacks[String(tid)] = [];
    this.emit({ type: 'exit', pid, tid, scope: 'thread', retval });
  }

  join(pid: number, tid: number, target: number): void {
    const p = this.proc(pid);
    const th = this.thread(pid, target);
    const ev: EventJoin = { type: 'join', tid, target };
    if (th.retval) ev.retval = th.retval;
    this.emit(ev);
    p.threads = p.threads.filter((x) => x.tid !== target);
    delete this.mem(pid).stacks[String(target)];
  }

  addSync(obj: SyncObject): void {
    this.sync.push(obj);
  }

  mutex(id: string): Extract<SyncObject, { kind: 'mutex' }> {
    const m = this.sync.find((x) => x.id === id);
    if (!m || m.kind !== 'mutex') throw new Error(`no existe el mutex ${id}`);
    return m;
  }

  lock(pid: number, tid: number, id: string): 'acquired' | 'blocked' {
    const m = this.mutex(id);
    if (m.owner === null) {
      m.owner = tid;
      m.waiters = m.waiters.filter((w) => w !== tid);
      this.thread(pid, tid).holds.push(id);
      this.endCall(pid, tid);
      this.emit({ type: 'mutex', op: 'lock', id, tid, result: 'acquired' });
      return 'acquired';
    }
    m.waiters.push(tid);
    this.emit({ type: 'mutex', op: 'lock', id, tid, result: 'blocked' });
    this.block(pid, tid, { kind: 'mutex', id, owner: m.owner }, 'pthread_mutex_lock');
    return 'blocked';
  }

  unlock(pid: number, tid: number, id: string): void {
    const m = this.mutex(id);
    m.owner = null;
    const th = this.thread(pid, tid);
    th.holds = th.holds.filter((x) => x !== id);
    this.emit({ type: 'mutex', op: 'unlock', id, tid, result: 'released' });
    const next = m.waiters[0];
    if (next !== undefined) this.unblock(pid, next);
  }

  // ----- señales -----

  setAction(pid: number, sig: SignalName, fn: string): void {
    this.proc(pid).signals.actions[sig] = { action: 'handler', fn };
  }

  sendSignal(from: InFlightSignal['from'], to: number, sig: SignalName): void {
    this.inflight.push({ signal: sig, from, to, status: 'pending' });
    const p = this.proc(to);
    if (!p.signals.pending.includes(sig)) p.signals.pending.push(sig);
    this.emit({ type: 'signalSend', from, to, signal: sig });
  }

  deliverSignal(
    pid: number,
    tid: number,
    sig: SignalName,
    action: 'handler' | 'ignore' | 'terminate',
    handler?: { fn: string; line: number; params: Var[]; locals: Var[] },
  ): void {
    this.inflight = this.inflight.filter((s) => !(s.to === pid && s.signal === sig));
    const p = this.proc(pid);
    p.signals.pending = p.signals.pending.filter((s) => s !== sig);
    const ev: EventSignalDeliver = { type: 'signalDeliver', pid, tid, signal: sig, action };
    if (handler) {
      ev.handler = handler.fn;
      this.pushFrame(pid, tid, handler.fn, handler.line, handler.params, handler.locals, sig);
      this.thread(pid, tid).inHandler = sig;
    }
    this.emit(ev);
  }

  signalReturn(pid: number, tid: number, sig: SignalName): void {
    const stacks = this.mem(pid).stacks;
    stacks[String(tid)] = stacks[String(tid)].slice(1);
    const th = this.thread(pid, tid);
    delete th.inHandler;
    th.fn = stacks[String(tid)][0]?.fn ?? null;
    this.emit({ type: 'signalReturn', pid, tid, signal: sig });
  }

  // ----- cierre -----

  private runnable(): TaskRef[] {
    const out: TaskRef[] = [];
    for (const p of this.procs) {
      if (p.state === 'zombie' || p.state === 'reaped') continue;
      for (const th of p.threads) {
        if (th.state === 'running' || th.state === 'ready') out.push({ pid: p.pid, tid: th.tid });
      }
    }
    return out;
  }

  private snapshotId(m: MemorySnapshot): string {
    const key = JSON.stringify(m);
    let id = this.snapshotIds.get(key);
    if (!id) {
      id = `m${this.snapshotIds.size}`;
      this.snapshotIds.set(key, id);
      this.snapshots[id] = structuredClone(m);
    }
    return id;
  }

  private commit(actor: TaskRef | null, executed: Step['executed'], choices: TaskRef[]): void {
    for (const [pid, m] of this.mems) {
      m.heap = m.heap.filter((b) => b.freedAt === undefined || this.t - b.freedAt <= FREED_VISIBLE_STEPS);
      if (this.proc(pid).state === 'reaped') this.mems.delete(pid);
    }
    for (const p of this.procs) {
      if (p.state === 'zombie' || p.state === 'reaped' || p.state === 'stopped') {
        p.mem = p.state === 'reaped' ? null : p.mem;
        continue;
      }
      for (const th of p.threads) {
        if (th.state !== 'running' && th.state !== 'ready') continue;
        th.state = actor && actor.pid === p.pid && actor.tid === th.tid ? 'running' : 'ready';
      }
      const states = p.threads.map((th) => th.state);
      p.state = states.includes('running') ? 'running' : states.includes('ready') ? 'ready' : 'blocked';
    }
    for (const p of this.procs) {
      const m = this.mems.get(p.pid);
      if (m && p.state !== 'reaped') p.mem = this.snapshotId(m);
      else if (p.state === 'reaped') p.mem = null;
    }
    const step: Step = {
      t: this.t,
      actor,
      choices,
      clock: this.clock,
      events: this.events,
      processes: structuredClone(this.procs),
      pipes: structuredClone(this.pipes),
      stdin: { ...this.stdinState },
      signals: structuredClone(this.inflight),
      timers: [],
      sync: structuredClone(this.sync),
    };
    if (executed) step.executed = executed;
    this.steps.push(step);
    this.events = [];
  }

  build(outcome: Trace['outcome']): Trace {
    return {
      version: 1,
      arch: 'x86_64',
      source: this.source,
      stdin: this.opts.stdin ?? '',
      run: {
        policy: this.opts.policy ?? 'round_robin',
        seed: this.opts.seed ?? 0,
        stdinEof: this.opts.stdinEof ?? true,
        schedule: this.opts.policy === 'manual' ? this.steps.flatMap((s) => (s.actor ? [s.actor] : [])) : [],
        injections: [],
        limits: DEFAULT_LIMITS,
      },
      compile: { ok: true, command: 'gcc -g -O0 -fno-omit-frame-pointer -pthread -no-pie -o prog prog.c', diagnostics: [] },
      outcome,
      truncated: false,
      steps: this.steps,
      snapshots: this.snapshots,
      output: this.output,
      summary: { leaks: this.leaks, memErrors: [] },
    };
  }
}
