// Línea de tiempo con marcadores de eventos. Clic en un marcador salta a ese paso.
import { Box, CircleX, GitBranch, GitFork, GitMerge, Hourglass, Lock, LockOpen, Plug, Trash2, Unplug, Zap, Droplet, Droplets, Replace, Skull, Terminal } from 'lucide-react';
import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { LucideIcon } from 'lucide-react';
import { describeStep } from '../trace/describe.ts';
import type { TraceIndex } from '../trace/query.ts';
import type { Event, Trace } from '../trace/types.ts';
import type { Player } from './usePlayer.ts';

interface Marker {
  icon: LucideIcon;
  label: string;
  tone: string;
}

const MARKER_SPACING = 22;

// Un marcador por paso: el evento más importante.
function markerOf(events: Event[]): (Marker & { rank: number }) | null {
  const m = pickMarker(events);
  return m ? { ...m, rank: RANK.indexOf(m.label) } : null;
}

const RANK = ['deadlock', 'fork', 'exec', 'pthread_create', 'señal', 'exit', 'wait', 'join', 'pipe', 'write', 'read', 'salida', 'close', 'mutex', 'bloqueo', 'malloc', 'free'];

function pickMarker(events: Event[]): Marker | null {
  const pick = (type: Event['type'], pred: (e: Event) => boolean = () => true) => events.find((e) => e.type === type && pred(e));
  if (pick('deadlock')) return { icon: Skull, label: 'deadlock', tone: 'danger' };
  if (pick('fork')) return { icon: GitFork, label: 'fork', tone: 'proc' };
  if (pick('exec')) return { icon: Replace, label: 'exec', tone: 'proc' };
  if (pick('threadCreate')) return { icon: GitBranch, label: 'pthread_create', tone: 'proc' };
  if (pick('signalSend') || pick('signalDeliver')) return { icon: Zap, label: 'señal', tone: 'signal' };
  if (pick('exit', (e) => e.type === 'exit' && e.scope === 'process')) return { icon: CircleX, label: 'exit', tone: 'proc' };
  if (pick('wait', (e) => e.type === 'wait' && e.reaped !== undefined)) return { icon: Hourglass, label: 'wait', tone: 'proc' };
  if (pick('join')) return { icon: GitMerge, label: 'join', tone: 'proc' };
  if (pick('pipe')) return { icon: Plug, label: 'pipe', tone: 'data' };
  if (pick('write', (e) => e.type === 'write' && !!e.pipe)) return { icon: Droplet, label: 'write', tone: 'data' };
  if (pick('read')) return { icon: Droplets, label: 'read', tone: 'data' };
  if (pick('write')) return { icon: Terminal, label: 'salida', tone: 'data' };
  if (pick('close')) return { icon: Unplug, label: 'close', tone: 'data' };
  if (pick('mutex', (e) => e.type === 'mutex' && e.result === 'acquired')) return { icon: Lock, label: 'mutex', tone: 'sync' };
  if (pick('mutex', (e) => e.type === 'mutex' && e.result === 'released')) return { icon: LockOpen, label: 'mutex', tone: 'sync' };
  if (pick('block')) return { icon: Hourglass, label: 'bloqueo', tone: 'blocked' };
  if (pick('malloc')) return { icon: Box, label: 'malloc', tone: 'mem' };
  if (pick('free')) return { icon: Trash2, label: 'free', tone: 'mem' };
  return null;
}

export function Timeline({ trace, index, player }: { trace: Trace; index: TraceIndex; player: Player }) {
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
    trace.steps.forEach((step, s) => {
      const m = markerOf(step.events);
      if (!m) return;
      const cell = Math.round((s / last) * cells);
      const cur = best.get(cell);
      if (!cur || m.rank < cur.m.rank) best.set(cell, { s, m });
    });
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
        role="slider"
        tabIndex={0}
        aria-label="Línea de tiempo"
        aria-valuemin={0}
        aria-valuemax={player.last}
        aria-valuenow={player.t}
        aria-valuetext={`Paso ${player.t} de ${player.last}`}
        onPointerDown={(e) => {
          if ((e.target as HTMLElement).closest('button')) return;
          scrub(e.clientX);
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1 && !(e.target as HTMLElement).closest('button')) scrub(e.clientX);
        }}
      >
        <div className="timeline-rail" />
        <div className="timeline-fill" style={{ width: pct(player.t) }} />
        {markers.map(({ s, m }) => {
          const Icon = m.icon;
          const text = describeStep(trace, index, s)[0] ?? m.label;
          return (
            <button
              key={s}
              type="button"
              className={`marker ${m.tone}${s === player.t ? ' current' : ''}${s <= player.t ? ' past' : ''}`}
              style={{ left: pct(s) }}
              onClick={() => player.goto(s, false)}
              title={`Paso ${s}: ${text}`}
              aria-label={`Ir al paso ${s}: ${m.label}`}
            >
              <Icon size={12} strokeWidth={2.4} />
            </button>
          );
        })}
        <div className="timeline-thumb" style={{ left: pct(player.t) }} />
      </div>
    </div>
  );
}
