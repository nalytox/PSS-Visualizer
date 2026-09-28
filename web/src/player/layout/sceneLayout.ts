// Disposición del lienzo en un paso: árbol de procesos, puertos, tubos, cables y bloques de señal.
// Función pura y determinista: el mismo paso produce siempre las mismas posiciones, así que al
// avanzar o retroceder los elementos se desplazan con suavidad y nunca saltan.
import { indexTrace, processAt, snapshotOf, threadLabel, type TraceIndex } from '../../trace/query.ts';
import type { Fd, Pipe, Process, SignalSource, Step, Trace } from '../../trace/types.ts';
import {
  BLACKBOX_H,
  BOX_PAD,
  CONSOLE_H,
  GAP_X,
  GAP_Y,
  HEADER_H,
  INIT_H,
  INIT_W,
  LANE_H,
  LANES_PAD_BOTTOM,
  LANES_PAD_TOP,
  MEM_TITLE_H,
  MIN_BOX_W,
  PORT_GAP,
  PORT_H,
  REAPED_H,
  REAPED_W,
  TUBE_LEN,
  WINDOW,
} from './constants.ts';
import { layoutMemory, type MemoryLayout, type Rect } from './memoryLayout.ts';

export interface Pt {
  x: number;
  y: number;
}

export interface PortLayout {
  fd: number;
  x: number;
  y: number;
  side: 'left' | 'right';
  entry: Fd;
}

export interface BoxLayout {
  pid: number;
  x: number;
  y: number;
  w: number;
  h: number;
  compact: boolean; // proceso recogido: silueta mínima
  lanes: number[]; // tids con carril, en orden
  lanesY: number;
  memY: number;
  mem: MemoryLayout | null;
  memOpen: boolean;
  blackboxY: number | null; // tras un exec sin símbolos, franja de caja negra en vez de memoria
  consoleY: number;
  ports: PortLayout[];
  sigPort: Pt;
}

export interface PipeLayout {
  id: string;
  cx: number;
  cy: number;
  dir: 1 | -1; // 1: baja de izquierda a derecha
  top: Pt; // extremo de escritura
  bottom: Pt; // extremo de lectura
  pipe: Pipe;
}

export interface CableLayout {
  key: string;
  pid: number;
  fd: number;
  pipe: string;
  end: 'r' | 'w';
  d: string;
  from: Pt;
  c1: Pt;
  c2: Pt;
  to: Pt;
}

export interface SignalLayout {
  key: string;
  signal: string;
  source: SignalSource;
  to: number;
  x: number;
  y: number;
  w: number;
  h: number;
  cable: { from: Pt; c1: Pt; c2: Pt; to: Pt; d: string };
  status: 'sending' | 'pending' | 'delivered';
}

export interface EdgeLayout {
  key: string;
  d: string;
  reaped: boolean;
}

export interface Curve {
  from: Pt;
  c1: Pt;
  c2: Pt;
  to: Pt;
  d: string;
}

// Un padre bloqueado en wait y uno de los hijos que puede recoger.
export interface WaitLayout {
  key: string;
  parent: number;
  child: number;
  curve: Curve; // del padre al hijo
  mid: Pt;
}

// El estado de salida viaja del hijo recogido al padre.
export interface ReapLayout {
  key: string;
  child: number;
  label: string;
  curve: Curve; // del hijo al padre
}

export interface InitLayout {
  x: number;
  y: number;
  w: number;
  h: number;
  edges: { key: string; pid: number; d: string }[];
}

export interface SceneLayout {
  boxes: Map<number, BoxLayout>;
  pipes: PipeLayout[];
  cables: CableLayout[];
  signals: SignalLayout[];
  edges: EdgeLayout[];
  waits: WaitLayout[];
  reaps: ReapLayout[];
  init: InitLayout | null;
  bounds: Rect;
}

export interface SceneOptions {
  memOpen: (pid: number) => boolean;
  inkOf: (pid: number, tid: number) => string;
}

export function lanesOf(trace: Trace, index: TraceIndex, t: number, pid: number): number[] {
  const tids = new Set<number>();
  for (let s = Math.max(0, t - WINDOW + 1); s <= t; s++) {
    processAt(trace.steps[s], pid)?.threads.forEach((th) => tids.add(th.tid));
  }
  return [...tids].sort((a, b) => (index.threadOrder.get(a) ?? 0) - (index.threadOrder.get(b) ?? 0));
}

function leaksOf(trace: Trace, pid: number, t: number): Set<string> {
  const last = t === trace.steps.length - 1;
  return new Set(last ? trace.summary.leaks.filter((l) => l.pid === pid).map((l) => l.addr) : []);
}

function boxSize(trace: Trace, index: TraceIndex, step: Step, p: Process, opts: SceneOptions): Omit<BoxLayout, 'x' | 'y' | 'ports' | 'sigPort'> {
  if (p.state === 'reaped') {
    return { pid: p.pid, w: REAPED_W, h: REAPED_H, compact: true, lanes: [], lanesY: 0, memY: 0, mem: null, memOpen: false, blackboxY: null, consoleY: 0 };
  }
  const lanes = lanesOf(trace, index, step.t, p.pid);
  const snap = snapshotOf(trace, p);
  const memOpen = opts.memOpen(p.pid) && snap !== null;
  const mem =
    snap && memOpen
      ? layoutMemory(
          snap,
          p.threads.map((th) => ({
            tid: th.tid,
            title: p.threads.length > 1 ? `hilo ${threadLabel(index, th.tid)}` : '',
            ink: opts.inkOf(p.pid, th.tid),
          })),
          { leaks: leaksOf(trace, p.pid, step.t) },
        )
      : null;
  const lanesY = HEADER_H;
  const lanesH = LANES_PAD_TOP + Math.max(1, lanes.length) * LANE_H + LANES_PAD_BOTTOM;
  const memY = lanesY + lanesH;
  const blackbox = p.image.kind === 'blackbox';
  const memH = blackbox ? BLACKBOX_H : snap ? MEM_TITLE_H + (mem ? mem.h + 14 : 0) : 0;
  const consoleY = memY + memH;
  const w = Math.max(MIN_BOX_W, mem ? mem.w + BOX_PAD * 2 : 0);
  return { pid: p.pid, w, h: consoleY + CONSOLE_H, compact: false, lanes, lanesY, memY, mem, memOpen, blackboxY: blackbox ? memY : null, consoleY };
}

type Sized = Omit<BoxLayout, 'x' | 'y' | 'ports' | 'sigPort'>;

// Árbol genealógico con anchos variables: cada subárbol ocupa el ancho de sus hijos o el propio.
function placeTree(procs: Process[], sizes: Map<number, Sized>, index: TraceIndex): Map<number, Pt> {
  const pids = new Set(procs.map((p) => p.pid));
  const children = new Map<number, number[]>();
  const roots: number[] = [];
  for (const p of procs) {
    const parent = index.parent.get(p.pid);
    if (parent !== undefined && pids.has(parent)) {
      if (!children.has(parent)) children.set(parent, []);
      children.get(parent)!.push(p.pid);
    } else roots.push(p.pid);
  }
  const depthH: number[] = [];
  const depthOf = new Map<number, number>();
  const walk = (pid: number, d: number) => {
    depthOf.set(pid, d);
    depthH[d] = Math.max(depthH[d] ?? 0, sizes.get(pid)!.h);
    (children.get(pid) ?? []).forEach((c) => walk(c, d + 1));
  };
  roots.forEach((r) => walk(r, 0));
  const levelY: number[] = [];
  depthH.reduce((y, h, d) => {
    levelY[d] = y;
    return y + h + GAP_Y;
  }, 0);

  const span = new Map<number, number>();
  const measure = (pid: number): number => {
    const kids = children.get(pid) ?? [];
    const kidsW = kids.reduce((s, c) => s + measure(c), 0) + GAP_X * Math.max(0, kids.length - 1);
    const w = Math.max(sizes.get(pid)!.w, kidsW);
    span.set(pid, w);
    return w;
  };
  const pos = new Map<number, Pt>();
  const place = (pid: number, left: number) => {
    const s = span.get(pid)!;
    const size = sizes.get(pid)!;
    pos.set(pid, { x: left + (s - size.w) / 2, y: levelY[depthOf.get(pid)!] });
    const kids = children.get(pid) ?? [];
    const kidsW = kids.reduce((acc, c) => acc + span.get(c)!, 0) + GAP_X * Math.max(0, kids.length - 1);
    let x = left + (s - kidsW) / 2;
    for (const c of kids) {
      place(c, x);
      x += span.get(c)! + GAP_X;
    }
  };
  let left = 0;
  for (const r of roots) {
    measure(r);
    place(r, left);
    left += span.get(r)! + GAP_X;
  }
  return pos;
}

const SQ = Math.SQRT1_2;

function curve(from: Pt, c1: Pt, c2: Pt, to: Pt): Curve {
  return { from, c1, c2, to, d: cablePath(from, c1, c2, to) };
}

// Curva vertical entre el borde inferior del padre y el superior del hijo, corrida `dx` para no
// taparse con la línea del árbol.
function familyCurve(pb: BoxLayout, cb: BoxLayout, dx: number): Curve {
  const a = { x: pb.x + pb.w / 2 + dx, y: pb.y + pb.h };
  const c = { x: cb.x + cb.w / 2 + dx, y: cb.y };
  const my = (a.y + c.y) / 2;
  return curve(a, { x: a.x, y: my }, { x: c.x, y: my }, c);
}

function waitsOn(step: Step, parent: Process, target: number): Process[] {
  return step.processes.filter(
    (c) =>
      c.ppid === parent.pid &&
      c.state !== 'reaped' &&
      (target === -1 || (target > 0 ? c.pid === target : c.pgid === (target === 0 ? parent.pgid : -target))),
  );
}

function placeWaits(step: Step, boxes: Map<number, BoxLayout>): WaitLayout[] {
  const out: WaitLayout[] = [];
  for (const p of step.processes) {
    const r = p.threads.find((th) => th.blockedOn?.kind === 'wait')?.blockedOn;
    if (r?.kind !== 'wait') continue;
    const pb = boxes.get(p.pid);
    for (const c of waitsOn(step, p, r.target)) {
      const cb = boxes.get(c.pid);
      if (!pb || !cb) continue;
      const cv = familyCurve(pb, cb, 34);
      out.push({ key: `${p.pid}-${c.pid}`, parent: p.pid, child: c.pid, curve: cv, mid: bezierPoint(cv.from, cv.c1, cv.c2, cv.to, 0.5) });
    }
  }
  return out;
}

function placeReaps(step: Step, boxes: Map<number, BoxLayout>): ReapLayout[] {
  const out: ReapLayout[] = [];
  for (const ev of step.events) {
    if (ev.type !== 'wait' || ev.reaped === undefined) continue;
    const pb = boxes.get(ev.pid);
    const cb = boxes.get(ev.reaped);
    if (!pb || !cb) continue;
    const down = familyCurve(pb, cb, 34);
    const label = !ev.status ? 'recogido' : 'code' in ev.status ? `código ${ev.status.code}` : ev.status.signal;
    out.push({ key: `reap-${ev.pid}-${ev.reaped}`, child: ev.reaped, label, curve: curve(down.to, down.c2, down.c1, down.from) });
  }
  return out;
}

// Nodo virtual init (1): arriba a la izquierda del árbol. Sus líneas punteadas corren por un riel
// sobre el árbol y bajan a cada huérfano vivo; van detrás de los cuadrados, así que solo se ven
// entre ellos.
function placeInit(step: Step, boxes: Map<number, BoxLayout>, left: number, top: number): InitLayout | null {
  const orphans = step.processes.filter((p) => p.ppid === 1);
  if (orphans.length === 0) return null;
  const railY = top - 50;
  const x = left - INIT_W - 40;
  const y = railY - INIT_H / 2;
  const edges = orphans
    .filter((p) => p.state !== 'reaped')
    .flatMap((p) => {
      const b = boxes.get(p.pid);
      if (!b) return [];
      const cx = b.x + 48;
      return [{ key: `init-${p.pid}`, pid: p.pid, d: `M${x + INIT_W},${railY} H${cx - 12} Q${cx},${railY} ${cx},${railY + 12} V${b.y}` }];
    });
  return { x, y, w: INIT_W, h: INIT_H, edges };
}

function placePipes(step: Step, boxes: Map<number, BoxLayout>): PipeLayout[] {
  const out: PipeLayout[] = [];
  const taken: Pt[] = [];
  const writerNow = new Map<string, number>();
  const readerNow = new Map<string, number>();
  for (const ev of step.events) {
    if (ev.type === 'write' && ev.pipe) writerNow.set(ev.pipe, ev.pid);
    if (ev.type === 'read' && ev.pipe) readerNow.set(ev.pipe, ev.pid);
  }
  for (const pipe of step.pipes) {
    const writers = [...new Set(pipe.writers.map((e) => e.pid))];
    const readers = [...new Set(pipe.readers.map((e) => e.pid))];
    const pick = (now: number | undefined, mine: number[], other: number[]) =>
      now ?? mine.find((p) => !other.includes(p)) ?? mine[0] ?? pipe.createdBy;
    const w = boxes.get(pick(writerNow.get(pipe.id), writers, readers)) ?? boxes.get(pipe.createdBy);
    const r = boxes.get(pick(readerNow.get(pipe.id), readers, writers)) ?? w;
    if (!w || !r) continue;
    let cx: number;
    let cy: number;
    let dir: 1 | -1 = 1;
    const wc = { x: w.x + w.w / 2 };
    const rc = { x: r.x + r.w / 2 };
    if (w === r) {
      cx = w.x + w.w + 120;
      cy = w.y + HEADER_H + 90;
    } else if (Math.abs(w.y - r.y) > 1) {
      // Niveles distintos del árbol: el tubo va al costado, a la altura media de los puertos, para
      // que los cables no crucen la línea que une padre e hijo.
      cx = Math.max(w.x + w.w, r.x + r.w) + 130;
      cy = (w.y + r.y) / 2 + HEADER_H + 70;
      dir = 1;
    } else {
      const [left, right] = w.x <= r.x ? [w, r] : [r, w];
      cx = (left.x + left.w + right.x) / 2;
      cy = Math.max(left.y, right.y) + HEADER_H + 110;
      dir = rc.x >= wc.x ? 1 : -1;
    }
    while (taken.some((p) => Math.abs(p.x - cx) < 70 && Math.abs(p.y - cy) < 70)) cx += 80;
    taken.push({ x: cx, y: cy });
    const hx = (TUBE_LEN / 2) * SQ;
    out.push({
      id: pipe.id,
      cx,
      cy,
      dir,
      top: { x: cx - dir * hx, y: cy - hx },
      bottom: { x: cx + dir * hx, y: cy + hx },
      pipe,
    });
  }
  return out;
}

function placePorts(p: Process, box: BoxLayout, pipes: PipeLayout[]): void {
  const fds = Object.keys(p.fds)
    .map(Number)
    .sort((a, b) => a - b);
  const next = { left: box.y + HEADER_H + 14, right: box.y + HEADER_H + 14 };
  for (const fd of fds) {
    const entry = p.fds[String(fd)];
    let side: 'left' | 'right' = 'right';
    if (entry.kind === 'pipe') {
      const tube = pipes.find((t) => t.id === entry.pipe);
      if (tube && tube.cx < box.x + box.w / 2) side = 'left';
    }
    const y = next[side];
    next[side] += PORT_GAP;
    box.ports.push({ fd, side, x: side === 'left' ? box.x : box.x + box.w, y: y + PORT_H / 2, entry });
  }
}

function unit(v: Pt): Pt {
  const n = Math.hypot(v.x, v.y) || 1;
  return { x: v.x / n, y: v.y / n };
}

export function cablePath(from: Pt, c1: Pt, c2: Pt, to: Pt): string {
  return `M${from.x},${from.y} C${c1.x},${c1.y} ${c2.x},${c2.y} ${to.x},${to.y}`;
}

function placeCables(boxes: Map<number, BoxLayout>, pipes: PipeLayout[]): CableLayout[] {
  const out: CableLayout[] = [];
  for (const box of boxes.values()) {
    for (const port of box.ports) {
      if (port.entry.kind !== 'pipe') continue;
      const entry = port.entry;
      const tube = pipes.find((t) => t.id === entry.pipe);
      if (!tube) continue;
      const out1 = port.side === 'right' ? 1 : -1;
      const from = { x: port.x + out1 * 12, y: port.y };
      const end = entry.end === 'w' ? tube.top : tube.bottom;
      // El cable entra al tubo en la dirección de su eje, inclinado hacia el lado del puerto.
      const axis = entry.end === 'w' ? { x: -tube.dir * SQ, y: -SQ } : { x: tube.dir * SQ, y: SQ };
      const toPort = unit({ x: from.x - end.x, y: from.y - end.y });
      const bend = unit({ x: axis.x + toPort.x, y: axis.y + toPort.y });
      const c1 = { x: from.x + out1 * 70, y: from.y };
      const c2 = { x: end.x + bend.x * 60, y: end.y + bend.y * 60 };
      out.push({
        key: `${box.pid}:${port.fd}:${entry.pipe}`,
        pid: box.pid,
        fd: port.fd,
        pipe: entry.pipe,
        end: entry.end,
        d: cablePath(from, c1, c2, end),
        from,
        c1,
        c2,
        to: end,
      });
    }
  }
  return out;
}

// Origen de una señal ya entregada: el envío más reciente con esa señal y ese destino.
function sourceOf(trace: Trace, t: number, signal: string, to: number): SignalSource {
  for (let s = t; s >= 0; s--) {
    for (const ev of trace.steps[s].events) {
      if (ev.type === 'signalSend' && ev.signal === signal && ev.to === to) return ev.from;
    }
  }
  return { kind: 'kernel', cause: 'fault' };
}

function placeSignals(trace: Trace, step: Step, boxes: Map<number, BoxLayout>, top: number): SignalLayout[] {
  const items = new Map<string, { signal: string; source: SignalSource; to: number; status: SignalLayout['status'] }>();
  const keyOf = (signal: string, source: SignalSource, to: number) =>
    `${signal}:${source.kind}:${'pid' in source ? source.pid : ''}:${to}`;
  for (const s of step.signals) items.set(keyOf(s.signal, s.from, s.to), { signal: s.signal, source: s.from, to: s.to, status: 'pending' });
  for (const ev of step.events) {
    if (ev.type === 'signalSend' && ev.to > 0) {
      items.set(keyOf(ev.signal, ev.from, ev.to), { signal: ev.signal, source: ev.from, to: ev.to, status: 'sending' });
    }
  }
  // Entregas de este paso: el bloque sigue visible para mostrar de dónde vino.
  for (const ev of step.events) {
    if (ev.type !== 'signalDeliver') continue;
    const found = [...items.values()].some((i) => i.signal === ev.signal && i.to === ev.pid);
    if (!found) {
      const source = sourceOf(trace, step.t, ev.signal, ev.pid);
      items.set(keyOf(ev.signal, source, ev.pid), { signal: ev.signal, source, to: ev.pid, status: 'delivered' });
    }
  }
  const out: SignalLayout[] = [];
  const stackAt = new Map<string, number>();
  for (const [key, it] of items) {
    const target = boxes.get(it.to);
    if (!target) continue;
    const w = 150;
    const h = 46;
    let x: number;
    let y: number;
    let anchor: string;
    if (it.source.kind === 'process') {
      const sender = boxes.get(it.source.pid) ?? target;
      x = sender.x - w - 70;
      y = sender.y + HEADER_H + 30;
      anchor = `p${sender.pid}`;
    } else if (it.source.kind === 'kernel') {
      x = target.x + target.w / 2 - w / 2 - 140;
      y = top - h - 70;
      anchor = 'kernel';
    } else {
      x = target.x - w - 70;
      y = target.y + HEADER_H + 30;
      anchor = `t${target.pid}`;
    }
    const n = stackAt.get(anchor) ?? 0;
    stackAt.set(anchor, n + 1);
    y += n * (h + 12);
    const from = it.source.kind === 'kernel' ? { x: x + w / 2, y: y + h } : { x: x + w, y: y + h / 2 };
    const to = target.sigPort;
    const c1 = it.source.kind === 'kernel' ? { x: from.x, y: from.y + 80 } : { x: from.x + 60, y: from.y };
    const c2 = { x: to.x - 80, y: to.y };
    out.push({ key, ...it, x, y, w, h, cable: { from, c1, c2, to, d: cablePath(from, c1, c2, to) } });
  }
  return out;
}

export function layoutScene(trace: Trace, t: number, opts: SceneOptions, index: TraceIndex = indexTrace(trace)): SceneLayout {
  const step = trace.steps[t];
  const sizes = new Map<number, Sized>();
  for (const p of step.processes) sizes.set(p.pid, boxSize(trace, index, step, p, opts));
  const pos = placeTree(step.processes, sizes, index);

  const boxes = new Map<number, BoxLayout>();
  for (const p of step.processes) {
    const s = sizes.get(p.pid)!;
    const { x, y } = pos.get(p.pid)!;
    boxes.set(p.pid, { ...s, x, y, ports: [], sigPort: { x, y: y + (s.compact ? s.h / 2 : HEADER_H / 2) } });
  }
  const pipes = placePipes(step, boxes);
  for (const p of step.processes) placePorts(p, boxes.get(p.pid)!, pipes);
  const cables = placeCables(boxes, pipes);

  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  const grow = (x: number, y: number, w = 0, h = 0) => {
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x + w);
    maxY = Math.max(maxY, y + h);
  };
  for (const b of boxes.values()) grow(b.x, b.y, b.w, b.h);
  const signals = placeSignals(trace, step, boxes, minY);
  for (const pl of pipes) grow(pl.cx - 80, pl.cy - 80, 160, 160);
  for (const s of signals) grow(s.x, s.y, s.w, s.h);
  const init = placeInit(step, boxes, minX, minY);
  if (init) grow(init.x, init.y, init.w, init.h);

  const edges: EdgeLayout[] = [];
  for (const b of boxes.values()) {
    const parent = index.parent.get(b.pid);
    const pb = parent !== undefined ? boxes.get(parent) : undefined;
    if (!pb) continue;
    const a = { x: pb.x + pb.w / 2, y: pb.y + pb.h };
    const c = { x: b.x + b.w / 2, y: b.y };
    const my = (a.y + c.y) / 2;
    edges.push({ key: `${pb.pid}-${b.pid}`, d: `M${a.x},${a.y} C${a.x},${my} ${c.x},${my} ${c.x},${c.y}`, reaped: b.compact });
  }

  const margin = 40;
  return {
    boxes,
    pipes,
    cables,
    signals,
    edges,
    waits: placeWaits(step, boxes),
    reaps: placeReaps(step, boxes),
    init,
    bounds: { x: minX - margin, y: minY - margin, w: maxX - minX + 2 * margin, h: maxY - minY + 2 * margin },
  };
}

// Punto de una curva cúbica, para mover cápsulas y pulsos a lo largo de los cables.
export function bezierPoint(p0: Pt, c1: Pt, c2: Pt, p3: Pt, u: number): Pt {
  const v = 1 - u;
  return {
    x: v * v * v * p0.x + 3 * v * v * u * c1.x + 3 * v * u * u * c2.x + u * u * u * p3.x,
    y: v * v * v * p0.y + 3 * v * v * u * c1.y + 3 * v * u * u * c2.y + u * u * u * p3.y,
  };
}
