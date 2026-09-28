// Disposición del panel de memoria (convención de Python Tutor: pilas a la izquierda, heap a la
// derecha, flechas para punteros). Es una función pura: recibe una instantánea y devuelve
// primitivas con coordenadas, más el índice de direcciones que usan las flechas.
import { visible } from '../../trace/bytes.ts';
import type { Field, HeapBlock, MemorySnapshot, Value, Var } from '../../trace/types.ts';
import { CELL_H, CW, INDEX_H, ROW_H } from './constants.ts';

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Prim =
  | { k: 'panel'; r: Rect; title: string; tone: 'frame' | 'globals' | 'handler'; ink?: string; sub?: string }
  | { k: 'heap'; r: Rect; title: string; sub: string; freed: boolean; leak: boolean; addr: string }
  | { k: 'name'; x: number; y: number; text: string; type: string; addr: string }
  | { k: 'cell'; r: Rect; text: string; key: string; uninit: boolean; tip: string; italic?: boolean }
  | { k: 'index'; x: number; y: number; text: string }
  | { k: 'box'; r: Rect; key: string }
  | { k: 'ptr'; r: Rect; key: string; target: string | null; tip: string; uninit: boolean }
  | { k: 'label'; x: number; y: number; text: string; ink?: string };

export interface Arrow {
  from: { x: number; y: number };
  target: string;
  to: Rect | null; // null = colgante
  side: 'left' | 'right';
  freed: boolean;
}

export interface MemoryLayout {
  w: number;
  h: number;
  prims: Prim[];
  arrows: Arrow[];
  // clave estable de cada celda → texto mostrado, para resaltar cambios
  values: Map<string, string>;
}

interface Located {
  addr: number;
  size: number;
  rect: Rect;
  depth: number; // 0 = variable o bloque completo; mayor = más anidado
  freed: boolean;
  array: boolean;
}

interface Ctx {
  prims: Prim[];
  located: Located[];
  pointers: { rect: Rect; target: string }[];
  values: Map<string, string>;
  freed: boolean;
}

interface Node {
  w: number;
  h: number;
  draw: (x: number, y: number, ctx: Ctx) => void;
}

const PAD = 8;
const GAP = 14;

export const parseAddr = (a: string): number => parseInt(a, 16);

function textW(s: string): number {
  return Array.from(s).length * CW;
}

function scalarText(v: Extract<Value, { kind: 'scalar' }>): string {
  return v.repr ?? String(v.value);
}

function locate(ctx: Ctx, addr: number | null, size: number, rect: Rect, depth: number, array = false) {
  if (addr === null || size <= 0) return;
  ctx.located.push({ addr, size, rect, depth, freed: ctx.freed, array });
}

// Nodo de un valor. `addr` y `size` permiten registrar la celda como destino de punteros.
function valueNode(v: Value, key: string, addr: number | null, size: number, uninit: boolean, depth: number, type: string): Node {
  switch (v.kind) {
    case 'scalar': {
      const text = uninit ? '?' : scalarText(v);
      const w = Math.max(3, Array.from(text).length) * CW + 12;
      return {
        w,
        h: CELL_H,
        draw: (x, y, ctx) => {
          const r = { x, y, w, h: CELL_H };
          ctx.prims.push({ k: 'cell', r, text, key, uninit, tip: `${type}${addr !== null ? ' @ 0x' + addr.toString(16) : ''}` });
          ctx.values.set(key, text);
          locate(ctx, addr, size, r, depth);
        },
      };
    }
    case 'opaque': {
      const text = v.note;
      const w = textW(text) + 14;
      return {
        w,
        h: CELL_H,
        draw: (x, y, ctx) => {
          const r = { x, y, w, h: CELL_H };
          ctx.prims.push({ k: 'cell', r, text, key, uninit, tip: type, italic: true });
          ctx.values.set(key, text);
          locate(ctx, addr, size, r, depth);
        },
      };
    }
    case 'pointer': {
      if (v.fn) {
        const text = uninit ? '?' : `→ ${v.fn}()`;
        const w = textW(text) + 14;
        return {
          w,
          h: CELL_H,
          draw: (x, y, ctx) => {
            const r = { x, y, w, h: CELL_H };
            ctx.prims.push({ k: 'cell', r, text, key, uninit, tip: `${type}: puntero a la función ${v.fn}` });
            ctx.values.set(key, text);
            locate(ctx, addr, size, r, depth);
          },
        };
      }
      const w = 30;
      return {
        w,
        h: CELL_H,
        draw: (x, y, ctx) => {
          const r = { x, y, w, h: CELL_H };
          const target = uninit ? null : v.target;
          const tip = uninit ? `${type} sin inicializar` : target === null ? `${type} = NULL` : `${type} → ${target}`;
          ctx.prims.push({ k: 'ptr', r, key, target, tip, uninit });
          ctx.values.set(key, uninit ? '?' : String(target));
          locate(ctx, addr, size, r, depth);
          if (target !== null) ctx.pointers.push({ rect: r, target });
        },
      };
    }
    case 'array': {
      const elemSize = v.length > 0 ? size / v.length : 0;
      if (v.text !== undefined) {
        const text = uninit ? '?' : `"${visible(v.text)}"`;
        const w = Math.max(4, Array.from(text).length) * CW + 14;
        return {
          w,
          h: CELL_H,
          draw: (x, y, ctx) => {
            const r = { x, y, w, h: CELL_H };
            ctx.prims.push({ k: 'cell', r, text, key, uninit, tip: `${type}: ${v.length} bytes, se muestra hasta el \\0` });
            ctx.values.set(key, text);
            locate(ctx, addr, size, r, depth);
          },
        };
      }
      const itemType = type.replace(/\[\d+\]$/, '');
      const items = v.items.map((it, i) =>
        valueNode(it, `${key}[${i}]`, addr === null ? null : addr + i * elemSize, elemSize, uninit, depth + 1, itemType),
      );
      const more = v.items.length < v.length;
      const w = items.reduce((s, n) => s + n.w, 0) + (more ? 3 * CW + 10 : 0) + 4;
      const h = INDEX_H + Math.max(CELL_H, ...items.map((n) => n.h));
      return {
        w,
        h,
        draw: (x, y, ctx) => {
          const r = { x, y, w, h };
          ctx.prims.push({ k: 'box', r: { x, y: y + INDEX_H - 2, w, h: h - INDEX_H + 2 }, key });
          locate(ctx, addr, size, { x, y: y + INDEX_H, w, h: h - INDEX_H }, depth, true);
          let cx = x + 2;
          items.forEach((n, i) => {
            ctx.prims.push({ k: 'index', x: cx + n.w / 2, y: y + INDEX_H - 4, text: String(i) });
            n.draw(cx, y + INDEX_H, ctx);
            cx += n.w;
          });
          if (more) ctx.prims.push({ k: 'cell', r: { x: cx, y: y + INDEX_H, w: 3 * CW + 8, h: CELL_H }, text: '…', key: key + '…', uninit: false, tip: `${v.length - v.items.length} elementos más` });
          void r;
        },
      };
    }
    case 'struct':
    case 'union': {
      return tableNode(v.fields, key, addr, size, uninit, depth, type);
    }
  }
}

// Tabla de campos (struct, union) o de variables (frames).
function tableNode(fields: (Field | Var)[], key: string, addr: number | null, size: number, uninit: boolean, depth: number, type: string): Node {
  const nameW = Math.max(3, ...fields.map((f) => Array.from(f.name).length)) * CW + 10;
  const rows = fields.map((f) =>
    valueNode(f.value, `${key}.${f.name}`, parseAddr(f.addr), f.size, uninit || !!f.uninit, depth + 1, f.type),
  );
  const valW = Math.max(30, ...rows.map((n) => n.w));
  const heights = rows.map((n) => Math.max(ROW_H, n.h + 4));
  const w = nameW + valW + 10;
  const h = heights.reduce((s, x) => s + x, 0) + 4;
  return {
    w,
    h,
    draw: (x, y, ctx) => {
      ctx.prims.push({ k: 'box', r: { x, y, w, h }, key });
      locate(ctx, addr, size, { x, y, w, h }, depth);
      let cy = y + 2;
      fields.forEach((f, i) => {
        const rh = heights[i];
        ctx.prims.push({ k: 'name', x: x + 6, y: cy + rh / 2, text: f.name, type: f.type, addr: f.addr });
        rows[i].draw(x + nameW, cy + (rh - rows[i].h) / 2, ctx);
        cy += rh;
      });
      void type;
    },
  };
}

interface Block {
  w: number;
  h: number;
  draw: (x: number, y: number, ctx: Ctx) => void;
}

function frameBlock(title: string, sub: string | undefined, vars: Var[], tone: 'frame' | 'globals' | 'handler', keyPrefix: string, ink?: string): Block {
  const nameW = Math.max(4, ...vars.map((v) => Array.from(v.name).length)) * CW + 12;
  const rows = vars.map((v) => valueNode(v.value, `${keyPrefix}.${v.name}@${v.addr}`, parseAddr(v.addr), v.size, !!v.uninit, 0, v.type));
  const heights = rows.map((n) => Math.max(ROW_H + 2, n.h + 6));
  const titleW = textW(title) + (sub ? textW(sub) + 16 : 0) + 24;
  const w = Math.max(titleW, nameW + Math.max(40, ...rows.map((n) => n.w)) + PAD * 2);
  const h = 24 + (vars.length === 0 ? ROW_H : heights.reduce((s, x) => s + x, 0)) + 6;
  return {
    w,
    h,
    draw: (x, y, ctx) => {
      ctx.prims.push({ k: 'panel', r: { x, y, w, h }, title, sub, tone, ink });
      let cy = y + 26;
      if (vars.length === 0) ctx.prims.push({ k: 'label', x: x + PAD, y: cy + ROW_H / 2, text: 'sin variables' });
      vars.forEach((v, i) => {
        const rh = heights[i];
        ctx.prims.push({ k: 'name', x: x + PAD, y: cy + rh / 2, text: v.name, type: v.type, addr: v.addr });
        rows[i].draw(x + PAD + nameW, cy + (rh - rows[i].h) / 2, ctx);
        cy += rh;
      });
    },
  };
}

function heapBlock(b: HeapBlock, leak: boolean): Block {
  const addr = parseAddr(b.addr);
  const inner = valueNode(b.value, `heap@${b.addr}`, addr, b.size, false, 1, b.type ?? '');
  const title = b.addr;
  const sub = `${b.type ?? 'bloque'} · ${b.size} B`;
  const w = Math.max(textW(title) + textW(sub) + 30, inner.w + PAD * 2);
  const h = 24 + inner.h + PAD;
  return {
    w,
    h,
    draw: (x, y, ctx) => {
      const freed = b.freedAt !== undefined;
      ctx.prims.push({ k: 'heap', r: { x, y, w, h }, title, sub, freed, leak, addr: b.addr });
      const saved = ctx.freed;
      ctx.freed = freed;
      locate(ctx, addr, b.size, { x, y, w, h }, 0);
      inner.draw(x + PAD, y + 24, ctx);
      ctx.freed = saved;
    },
  };
}

// Orden del heap siguiendo las flechas, de izquierda a derecha: los bloques que nadie más del heap
// apunta van primero (en el orden en que los alcanzan las variables); cada bloque queda a la derecha
// del que lo apunta. Así una lista enlazada se dibuja en cadena.
function heapColumns(snapshot: MemorySnapshot): HeapBlock[][] {
  const blocks = snapshot.heap;
  const blockAt = (a: number) => blocks.find((b) => a >= parseAddr(b.addr) && a < parseAddr(b.addr) + Math.max(1, b.size));
  const targetsOf = (v: Value, out: number[]) => {
    if (v.kind === 'pointer' && v.target) out.push(parseAddr(v.target));
    else if (v.kind === 'struct' || v.kind === 'union') v.fields.forEach((f) => !f.uninit && targetsOf(f.value, out));
    else if (v.kind === 'array') v.items.forEach((it) => targetsOf(it, out));
  };
  const edges = new Map<HeapBlock, HeapBlock[]>();
  const inDegree = new Map<HeapBlock, number>(blocks.map((b) => [b, 0]));
  for (const b of blocks) {
    const out: number[] = [];
    targetsOf(b.value, out);
    const next = [...new Set(out.map(blockAt).filter((x): x is HeapBlock => !!x && x !== b))];
    edges.set(b, next);
    next.forEach((n) => inDegree.set(n, (inDegree.get(n) ?? 0) + 1));
  }
  const roots: number[] = [];
  const vars = [...snapshot.globals, ...Object.values(snapshot.stacks).flat().flatMap((f) => [...f.params, ...f.locals])];
  vars.forEach((v) => !v.uninit && targetsOf(v.value, roots));
  const rank = (b: HeapBlock) => {
    const i = roots.findIndex((a) => blockAt(a) === b);
    return i === -1 ? Infinity : i;
  };
  const sources = blocks.filter((b) => inDegree.get(b) === 0).sort((a, b) => rank(a) - rank(b));
  // En un ciclo (lista circular) no hay fuentes: se parte por el bloque que alcanza primero una variable.
  if (sources.length === 0 && blocks.length > 0) sources.push([...blocks].sort((a, b) => rank(a) - rank(b))[0]);

  const column = new Map<HeapBlock, number>();
  const order: HeapBlock[] = [];
  const visit = (b: HeapBlock, c: number, path: Set<HeapBlock>) => {
    if (path.has(b)) return;
    if (!column.has(b)) order.push(b);
    if ((column.get(b) ?? -1) >= c) return;
    column.set(b, c);
    path.add(b);
    for (const n of edges.get(b) ?? []) visit(n, c + 1, path);
    path.delete(b);
  };
  sources.forEach((b) => visit(b, 0, new Set()));
  blocks.forEach((b) => visit(b, 0, new Set()));

  const cols: HeapBlock[][] = [];
  for (const b of order) (cols[column.get(b)!] ??= []).push(b);
  return cols.filter((c) => c && c.length > 0);
}

export interface StackSpec {
  tid: number;
  title: string;
  ink: string;
}

export function layoutMemory(
  snapshot: MemorySnapshot,
  stacks: StackSpec[],
  opts: { leaks?: Set<string> } = {},
): MemoryLayout {
  const ctx: Ctx = { prims: [], located: [], pointers: [], values: new Map(), freed: false };
  let y = 0;
  let w = 0;

  if (snapshot.globals.length > 0) {
    const g = frameBlock('Globales', undefined, snapshot.globals, 'globals', 'g');
    g.draw(0, y, ctx);
    y += g.h + GAP;
    w = Math.max(w, g.w);
  }

  let x = 0;
  let colsH = 0;
  for (const s of stacks) {
    const frames = snapshot.stacks[String(s.tid)] ?? [];
    let cy = y;
    let colW = 0;
    const blocks = frames.map((f, i) => {
      const tone = f.signal ? 'handler' : 'frame';
      const title = f.signal ? `${f.fn}(${f.signal})` : `${f.fn}()`;
      return frameBlock(title, i === 0 ? s.title : undefined, [...f.params, ...f.locals], tone, `s${s.tid}.${frames.length - 1 - i}`, s.ink);
    });
    for (const b of blocks) {
      b.draw(x, cy, ctx);
      cy += b.h + 8;
      colW = Math.max(colW, b.w);
    }
    if (frames.length > 0) {
      x += colW + GAP;
      colsH = Math.max(colsH, cy - y);
    }
  }

  const cols = heapColumns(snapshot);
  if (cols.length > 0) {
    x += x > 0 ? 36 : 0;
    ctx.prims.push({ k: 'label', x, y: y + 8, text: 'Heap' });
    for (const col of cols) {
      let cy = y + 18;
      let colW = 0;
      for (const b of col) {
        const blk = heapBlock(b, opts.leaks?.has(b.addr) ?? false);
        blk.draw(x, cy, ctx);
        cy += blk.h + 12;
        colW = Math.max(colW, blk.w);
      }
      x += colW + 40;
      colsH = Math.max(colsH, cy - y);
    }
    x -= 40;
  } else if (x > 0) {
    x -= GAP;
  }
  w = Math.max(w, x);
  const h = y + colsH;

  const arrows: Arrow[] = ctx.pointers.map(({ rect, target }) => {
    const hit = resolvePointer(ctx.located, parseAddr(target));
    const from = { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
    if (!hit) return { from, target, to: null, side: 'left', freed: false };
    const side = hit.rect.x > rect.x + rect.w + 10 ? 'left' : 'right';
    return { from, target, to: hit.rect, side, freed: hit.freed };
  });

  return { w: Math.max(w, 120), h: Math.max(h, 30), prims: ctx.prims, arrows, values: ctx.values };
}

// Destino de un puntero: la celda más externa que empieza exactamente en esa dirección; si no hay,
// la más interna que la contiene. Sin coincidencia, el puntero es colgante.
// Un puntero al inicio de un arreglo apunta a su primer elemento, como en C.
export function resolvePointer(located: Located[], addr: number): Located | null {
  const exact = located.filter((l) => l.addr === addr).sort((a, b) => a.depth - b.depth);
  const firstNonArray = exact.find((l) => !l.array);
  if (exact.length > 0) return exact[0].array && firstNonArray ? firstNonArray : exact[0];
  const inside = located.filter((l) => addr > l.addr && addr < l.addr + l.size).sort((a, b) => b.depth - a.depth);
  return inside[0] ?? null;
}
