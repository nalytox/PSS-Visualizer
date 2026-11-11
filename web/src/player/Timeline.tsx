// Línea de tiempo con marcadores de eventos. Clic en un marcador salta a ese paso.
import { Box, CircleX, GitBranch, GitFork, GitMerge, Hourglass, Lock, LockOpen, Plug, Trash2, Unplug, Zap, Droplet, Droplets, Replace, Skull, Terminal } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import { describeStep } from '../trace/describe.ts';
import { fmt } from '../trace/format.ts';
import type { TraceIndex } from '../trace/query.ts';
import { flag, type Flag, type LoadedTrace } from '../trace/loaded.ts';
import type { Player } from './usePlayer.ts';

interface Marker {
  icon: LucideIcon;
  label: string;
  tone: string;
}

const MARKER_SPACING = 22;

// Un marcador por paso: el evento más importante. Sale de las banderas del índice, sin abrir el paso.
function markerOf(flags: number): (Marker & { rank: number }) | null {
  const m = pickMarker(flags);
  return m ? { ...m, rank: RANK.indexOf(m.label) } : null;
}

const RANK = ['deadlock', 'fork', 'exec', 'pthread_create', 'señal', 'exit', 'wait', 'join', 'pipe', 'write', 'read', 'salida', 'close', 'mutex', 'bloqueo', 'malloc', 'free'];

function pickMarker(flags: number): Marker | null {
  const has = (...names: Flag[]) => names.some((n) => (flags & flag(n)) !== 0);
  if (has('deadlock')) return { icon: Skull, label: 'deadlock', tone: 'danger' };
  if (has('fork')) return { icon: GitFork, label: 'fork', tone: 'proc' };
  if (has('exec')) return { icon: Replace, label: 'exec', tone: 'proc' };
  if (has('threadCreate')) return { icon: GitBranch, label: 'pthread_create', tone: 'proc' };
  if (has('signalSend', 'signalDeliver')) return { icon: Zap, label: 'señal', tone: 'signal' };
  if (has('exitProcess')) return { icon: CircleX, label: 'exit', tone: 'proc' };
  if (has('waitReaped')) return { icon: Hourglass, label: 'wait', tone: 'proc' };
  if (has('join')) return { icon: GitMerge, label: 'join', tone: 'proc' };
  if (has('pipe')) return { icon: Plug, label: 'pipe', tone: 'data' };
  if (has('writePipe')) return { icon: Droplet, label: 'write', tone: 'data' };
  if (has('read')) return { icon: Droplets, label: 'read', tone: 'data' };
  if (has('write')) return { icon: Terminal, label: 'salida', tone: 'data' };
  if (has('close')) return { icon: Unplug, label: 'close', tone: 'data' };
  if (has('mutexAcquired')) return { icon: Lock, label: 'mutex', tone: 'sync' };
  if (has('mutexReleased')) return { icon: LockOpen, label: 'mutex', tone: 'sync' };
  if (has('block')) return { icon: Hourglass, label: 'bloqueo', tone: 'blocked' };
  if (has('malloc')) return { icon: Box, label: 'malloc', tone: 'mem' };
  if (has('free')) return { icon: Trash2, label: 'free', tone: 'mem' };
  return null;
}

export function Timeline({ trace, index, player }: { trace: LoadedTrace; index: TraceIndex; player: Player }) {
  const bar = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(1000);
  const last = Math.max(1, player.last);
  const pct = (s: number) => `${(s / last) * 100}%`;

  useLayoutEffect(() => {
    const el = bar.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Si hay más eventos que espacio, se agrupan: un marcador por celda, el de mayor importancia.
  const markers = useMemo(() => {
    const cells = Math.max(1, Math.floor(width / MARKER_SPACING));
    const best = new Map<number, { s: number; m: NonNullable<ReturnType<typeof markerOf>> }>();
    const flags = trace.columns.flags;
    for (let s = 0; s < trace.length; s++) {
      if (flags[s] === 0) continue;
      const m = markerOf(flags[s]);
      if (!m) continue;
      const cell = Math.round((s / last) * cells);
      const cur = best.get(cell);
      if (!cur || m.rank < cur.m.rank) best.set(cell, { s, m });
    }
    return [...best.values()];
  }, [trace, width, last]);

  const scrub = (clientX: number) => {
    const r = bar.current?.getBoundingClientRect();
    if (!r) return;
    const s = Math.round(((clientX - r.left) / r.width) * last);
    player.goto(s, false);
  };

  return (
    <div className="timeline">
      <div
        ref={bar}
        className="timeline-track"
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest('button')) return;
          scrub(e.clientX);
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1 && !(e.target as HTMLElement).closest('button')) scrub(e.clientX);
        }}
      >
        {/* El deslizador y los marcadores son hermanos: un control interactivo no puede contener otros. */}
        <div
          className="timeline-slider"
          role="slider"
          tabIndex={0}
          aria-label="Línea de tiempo"
          aria-valuemin={0}
          aria-valuemax={player.last}
          aria-valuenow={player.t}
          aria-valuetext={`Paso ${fmt(player.t)} de ${fmt(player.last)}`}
        >
          <div className="timeline-rail" />
          <div className="timeline-fill" style={{ width: pct(player.t) }} />
          <div className="timeline-thumb" style={{ left: pct(player.t) }} />
        </div>
        {markers.map(({ s, m }) => {
          const Icon = m.icon;
          // La narración del paso se arma al mirarlo: con muchos pasos, cada una abre un bloque.
          const explain = (e: { currentTarget: HTMLElement }) => {
            e.currentTarget.title = `Paso ${fmt(s)}: ${describeStep(trace, index, s)[0] ?? m.label}`;
          };
          return (
            <button
              key={s}
              type="button"
              className={`marker ${m.tone}${s === player.t ? ' current' : ''}${s <= player.t ? ' past' : ''}`}
              style={{ left: pct(s) }}
              onClick={() => player.goto(s, false)}
              onPointerEnter={explain}
              onFocus={explain}
              aria-label={`Ir al paso ${fmt(s)}: ${m.label}`}
            >
              <Icon size={12} strokeWidth={2.4} />
            </button>
          );
        })}
      </div>
    </div>
  );
}
