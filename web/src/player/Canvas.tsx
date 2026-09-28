// Lienzo SVG con zoom y desplazamiento. Compone procesos, cables, tubos, señales y animaciones.
import { AnimatePresence } from 'motion/react';
import { Maximize, ZoomIn, ZoomOut } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { TraceIndex } from '../trace/query.ts';
import type { Trace } from '../trace/types.ts';
import { layoutScene } from './layout/sceneLayout.ts';
import { Cables } from './components/Cables.tsx';
import { Defs } from './components/Defs.tsx';
import { InitNode, WaitLines } from './components/Family.tsx';
import { Minimap } from './components/Minimap.tsx';
import { Overlays, hiddenCapsules } from './components/Overlays.tsx';
import { PipeTube } from './components/PipeTube.tsx';
import { ProcessBox } from './components/ProcessBox.tsx';
import { SignalBlock } from './components/SignalBlock.tsx';
import { Tooltip, type TipState } from './components/Tooltip.tsx';
import { SceneContext, threadInk, type SceneCtx, type Tip } from './SceneContext.tsx';
import type { Player } from './usePlayer.ts';

interface View {
  x: number;
  y: number;
  k: number;
  smooth?: boolean; // transición suave (encuadre, seguimiento); rueda y arrastre son inmediatos
}

const MIN_K = 0.2;
const MAX_K = 2.5;
// Zoom mínimo del encuadre automático: por debajo, el texto de la memoria deja de leerse.
const FIT_MIN_K = 0.8;

export function Canvas({
  trace,
  index,
  player,
  onChoose,
}: {
  trace: Trace;
  index: TraceIndex;
  player: Player;
  onChoose?: (task: { pid: number; tid: number }) => void;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [view, setView] = useState<View>({ x: 0, y: 0, k: 1 });
  const [tip, setTip] = useState<TipState | null>(null);
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const drag = useRef<{ x: number; y: number; vx: number; vy: number } | null>(null);
  const fitted = useRef<Trace | null>(null);

  const t = player.t;
  const scene = useMemo(
    () => layoutScene(trace, t, { memOpen: (pid) => !collapsed.has(pid), inkOf: (pid, tid) => threadInk(index, pid, tid) }, index),
    [trace, t, collapsed, index],
  );
  const step = trace.steps[t];
  const animating = !!player.anim && player.progress < 1;
  const smooth = !!player.anim || player.playing;

  useLayoutEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // Encuadre de toda la ejecución: se muestrea la traza para que el zoom no cambie en cada paso.
  const fullBounds = useMemo(() => {
    const opts = { memOpen: (pid: number) => !collapsed.has(pid), inkOf: (pid: number, tid: number) => threadInk(index, pid, tid) };
    const last = trace.steps.length - 1;
    const samples = [...new Set([0, Math.floor(last / 3), Math.floor((2 * last) / 3), last])];
    return samples
      .map((s) => layoutScene(trace, s, opts, index).bounds)
      .reduce((a, o) => {
        const x = Math.min(a.x, o.x);
        const y = Math.min(a.y, o.y);
        return { x, y, w: Math.max(a.x + a.w, o.x + o.w) - x, h: Math.max(a.y + a.h, o.y + o.h) - y };
      });
  }, [trace, collapsed, index]);

  // Encuadre: si todo cabe a un zoom legible, se centra; si no, se muestra desde arriba a la
  // izquierda con ese zoom mínimo y la vista sigue al proceso que avanza.
  const fit = useCallback(() => {
    const b = fullBounds;
    const ideal = Math.min(size.w / b.w, size.h / b.h) * 0.96;
    const k = Math.min(1.1, Math.max(FIT_MIN_K, ideal));
    const x = b.w * k <= size.w ? (size.w - b.w * k) / 2 - b.x * k : 16 - b.x * k;
    const y = b.h * k <= size.h ? (size.h - b.h * k) / 2 - b.y * k : 16 - b.y * k;
    setView({ k, x, y, smooth: true });
  }, [fullBounds, size]);

  // Si lo que ocurrió en el paso quedó fuera de la vista, se desplaza (sin cambiar el zoom) para
  // mostrarlo: el proceso que avanzó, las señales que lo tocan y los tubos que usó.
  const actorPid = step.actor?.pid;
  useEffect(() => {
    const rects: { x: number; y: number; w: number; h: number }[] = [];
    const box = actorPid !== undefined ? scene.boxes.get(actorPid) : undefined;
    if (box) rects.push({ x: box.x, y: box.y, w: box.w, h: Math.min(box.h, 260) });
    for (const sg of scene.signals) {
      if (sg.status === 'pending') continue;
      rects.push({ x: sg.x, y: sg.y, w: sg.w, h: sg.h });
      const target = scene.boxes.get(sg.to);
      if (target) rects.push({ x: target.x, y: target.y, w: 60, h: 60 });
    }
    const used = new Set(step.events.flatMap((e) => ('pipe' in e && typeof e.pipe === 'string' ? [e.pipe] : [])));
    for (const pl of scene.pipes) if (used.has(pl.id)) rects.push({ x: pl.cx - 80, y: pl.cy - 80, w: 160, h: 160 });
    if (rects.length === 0) return;
    const fx = Math.min(...rects.map((r) => r.x));
    const fy = Math.min(...rects.map((r) => r.y));
    const fw = Math.max(...rects.map((r) => r.x + r.w)) - fx;
    const fh = Math.max(...rects.map((r) => r.y + r.h)) - fy;
    setView((v) => {
      const left = fx * v.k + v.x;
      const top = fy * v.k + v.y;
      const right = left + fw * v.k;
      const bottom = top + fh * v.k;
      const pad = 16;
      const maxRight = size.w - 56;
      let dx = 0;
      let dy = 0;
      if (right > maxRight) dx = maxRight - right;
      if (left + dx < pad) dx = pad - left;
      if (bottom > size.h - pad) dy = size.h - pad - bottom;
      if (top + dy < pad) dy = pad - top;
      return dx === 0 && dy === 0 ? v : { ...v, x: v.x + dx, y: v.y + dy, smooth: true };
    });
  }, [actorPid, t, scene, size, step]);

  useEffect(() => {
    if (fitted.current !== trace && size.w > 0) {
      fitted.current = trace;
      fit();
    }
  }, [trace, fit, size]);

  useEffect(() => {
    const el = wrap.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      setView((v) => {
        const k = Math.min(MAX_K, Math.max(MIN_K, v.k * Math.exp(-e.deltaY * 0.0015)));
        return { k, x: mx - ((mx - v.x) * k) / v.k, y: my - ((my - v.y) * k) / v.k, smooth: false };
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, []);

  const zoomBy = (f: number) =>
    setView((v) => {
      const k = Math.min(MAX_K, Math.max(MIN_K, v.k * f));
      const cx = size.w / 2;
      const cy = size.h / 2;
      return { k, x: cx - ((cx - v.x) * k) / v.k, y: cy - ((cy - v.y) * k) / v.k, smooth: true };
    });

  const showTip = useCallback((tp: Tip, el: Element) => {
    const host = wrap.current?.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    if (!host) return;
    const below = r.bottom - host.top + 8;
    setTip({ tip: tp, x: Math.min(Math.max(8, r.left - host.left + r.width / 2), host.width - 8), y: below > host.height - 90 ? r.top - host.top - 8 : below });
  }, []);
  const hideTip = useCallback(() => setTip(null), []);
  const toggleMem = useCallback(
    (pid: number) =>
      setCollapsed((s) => {
        const n = new Set(s);
        if (n.has(pid)) n.delete(pid);
        else n.add(pid);
        return n;
      }),
    [],
  );

  const ctx: SceneCtx = useMemo(
    () => ({ trace, index, player, showTip, hideTip, toggleMem, choose: onChoose }),
    [trace, index, player, showTip, hideTip, toggleMem, onChoose],
  );

  const hidden = hiddenCapsules(player.anim ? trace.steps[player.anim.step] : step, player.anim?.dir ?? 1, animating);
  const forkedFrom = new Map<number, number>();
  for (const ev of step.events) if (ev.type === 'fork') forkedFrom.set(ev.child, ev.parent);

  return (
    <SceneContext.Provider value={ctx}>
      <div
        ref={wrap}
        className="canvas"
        onPointerDown={(e) => {
          if ((e.target as Element).closest('[tabindex], button')) return;
          drag.current = { x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          const d = drag.current;
          if (d) setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y, smooth: false }));
        }}
        onPointerUp={() => (drag.current = null)}
      >
        <svg width={size.w} height={size.h} className="canvas-svg" role="group" aria-label={`Estado del sistema en el paso ${t}`}>
          <Defs />
          <g className={`viewport${view.smooth ? ' smooth' : ''}`} style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.k})` }}>
            <g className="edges">
              {scene.edges.map((e) => (
                <path key={e.key} d={e.d} className={`tree-edge${e.reaped ? ' reaped' : ''}`} />
              ))}
            </g>
            {scene.init && <InitNode init={scene.init} />}
            <WaitLines waits={scene.waits} animate={smooth} />
            <AnimatePresence>
              {scene.signals.map((s) => (
                <SignalBlock key={s.key} s={s} animate={smooth} />
              ))}
            </AnimatePresence>
            <AnimatePresence>
              {step.processes.map((p) => {
                const box = scene.boxes.get(p.pid)!;
                const parent = forkedFrom.get(p.pid);
                const pb = parent !== undefined && player.anim?.dir === 1 ? scene.boxes.get(parent) : undefined;
                return <ProcessBox key={p.pid} box={box} proc={p} bornFrom={pb ? { x: pb.x, y: pb.y } : undefined} animate={smooth} />;
              })}
            </AnimatePresence>
            <Cables cables={scene.cables} animate={smooth} />
            {scene.pipes.map((pl) => (
              <PipeTube key={pl.id} pl={pl} hideHead={hidden.get(pl.id)?.head} hideTail={hidden.get(pl.id)?.tail} />
            ))}
            <Overlays scene={scene} />
          </g>
        </svg>
        <div className="canvas-tools" role="toolbar" aria-label="Zoom del lienzo">
          <button type="button" onClick={() => zoomBy(1.2)} aria-label="Acercar" title="Acercar">
            <ZoomIn size={16} />
          </button>
          <button type="button" onClick={() => zoomBy(1 / 1.2)} aria-label="Alejar" title="Alejar">
            <ZoomOut size={16} />
          </button>
          <button type="button" onClick={fit} aria-label="Ajustar a la pantalla" title="Ajustar a la pantalla">
            <Maximize size={16} />
          </button>
        </div>
        {step.processes.length > 8 && (
          <Minimap scene={scene} view={view} size={size} onCenter={(x, y) => setView((v) => ({ ...v, x: size.w / 2 - x * v.k, y: size.h / 2 - y * v.k, smooth: false }))} />
        )}
        <Tooltip state={tip} />
      </div>
    </SceneContext.Provider>
  );
}
