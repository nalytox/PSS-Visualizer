// Tubo de un pipe (sección 7): diagonal a 45°, entrada arriba (escritura) y salida abajo (lectura).
import { capsules, formatSize } from '../../trace/bytes.ts';
import { TUBE_LEN, TUBE_R } from '../layout/constants.ts';
import type { PipeLayout, Pt } from '../layout/sceneLayout.ts';
import { tipProps, useScene } from '../SceneContext.tsx';

const SQ = Math.SQRT1_2;
export const CAPSULE = 16;
const CAPSULE_STEP = 19;
const MAX_CAPSULES = 5;

// Posición de reposo de la cápsula i (0 = la próxima que se leerá, junto a la salida).
export function capsuleRest(pl: PipeLayout, i: number): Pt {
  const u = 18 + i * CAPSULE_STEP;
  return { x: pl.bottom.x - pl.dir * SQ * u, y: pl.bottom.y - SQ * u };
}

export function Capsule({ p, ch, special }: { p: Pt; ch: string; special?: boolean }) {
  const w = special ? 34 : CAPSULE;
  return (
    <g className={`capsule${special ? ' special' : ''}`} transform={`translate(${p.x},${p.y})`}>
      <rect x={-w / 2} y={-CAPSULE / 2} width={w} height={CAPSULE} rx={CAPSULE / 2} />
      <text y={4} textAnchor="middle">
        {ch}
      </text>
    </g>
  );
}

export function PipeTube({ pl, hideHead = 0, hideTail = 0 }: { pl: PipeLayout; hideHead?: number; hideTail?: number }) {
  const ctx = useScene();
  const { pipe } = pl;
  const angle = pl.dir === 1 ? 45 : 135;
  const caps = capsules(pipe.buffer);
  const visibleCaps = caps.slice(hideHead, Math.max(hideHead, caps.length - hideTail));
  const shown = visibleCaps.slice(0, MAX_CAPSULES);
  const extra = pipe.size - hideHead - hideTail - shown.length;
  const level = pipe.size === 0 ? 0 : Math.max(0.06, pipe.size / pipe.capacity);
  const warnW = pipe.warnings.some((w) => w.end === 'w');
  const warnR = pipe.warnings.some((w) => w.end === 'r');
  const noWriters = pipe.writers.length === 0;
  const label = { x: pl.cx + pl.dir * 34, y: pl.cy - 30 };
  const readers = [...new Set(pipe.readers.map((e) => e.pid))].join(', ') || 'nadie';
  const writers = [...new Set(pipe.writers.map((e) => e.pid))].join(', ') || 'nadie';

  return (
    <g
      className={`pipe-tube${pipe.broken ? ' broken' : ''}`}
      {...tipProps(ctx, {
        title: `Pipe ${pipe.id} · ${formatSize(pipe.size)} de ${formatSize(pipe.capacity)}`,
        body: `Escriben: ${writers}. Leen: ${readers}.${noWriters ? ' Ya no quedan escritores: cuando se vacíe, read devolverá 0 (EOF).' : ''}`,
      })}
    >
      <g transform={`translate(${pl.cx},${pl.cy}) rotate(${angle})`}>
        <rect x={-TUBE_LEN / 2 - 3} y={-TUBE_R - 3} width={TUBE_LEN + 6} height={2 * TUBE_R + 6} rx={TUBE_R + 3} className="tube-halo" />
        <rect x={-TUBE_LEN / 2} y={-TUBE_R} width={TUBE_LEN} height={2 * TUBE_R} rx={TUBE_R} className="tube-body" />
        {level > 0 && (
          <rect x={TUBE_LEN / 2 - TUBE_LEN * level} y={-TUBE_R + 2} width={TUBE_LEN * level - 2} height={2 * TUBE_R - 4} rx={TUBE_R - 2} className="tube-liquid" />
        )}
        <rect x={-TUBE_LEN / 2} y={-TUBE_R} width={TUBE_LEN} height={2 * TUBE_R} rx={TUBE_R} fill="url(#glass)" className="tube-glass" />
        <line x1={-TUBE_LEN / 2 + 14} y1={-TUBE_R + 5} x2={TUBE_LEN / 2 - 14} y2={-TUBE_R + 5} className="tube-sheen" />
        {pipe.broken && <path d={`M-6,${-TUBE_R} l6,10 l-5,6 l7,${TUBE_R}`} className="tube-crack" />}
      </g>

      {/* Extremos: tapón gris si quedó abierto sin uso */}
      <EndCap p={pl.top} plugged={warnW} />
      <EndCap p={pl.bottom} plugged={warnR} />
      <text x={pl.top.x - pl.dir * 8} y={pl.top.y - 12} textAnchor={pl.dir === 1 ? 'end' : 'start'} className="tube-end-label">
        escribe · fd[1]
      </text>
      <text x={pl.bottom.x + pl.dir * 8} y={pl.bottom.y + 22} textAnchor={pl.dir === 1 ? 'start' : 'end'} className="tube-end-label">
        lee · fd[0]
      </text>

      {shown.map((ch, i) => (
        <Capsule key={`${i}-${ch}`} p={capsuleRest(pl, i)} ch={ch} />
      ))}
      {extra > 0 && (
        <text x={capsuleRest(pl, MAX_CAPSULES).x} y={capsuleRest(pl, MAX_CAPSULES).y + 4} textAnchor="middle" className="tube-more">
          +{extra}
        </text>
      )}

      <g transform={`translate(${label.x},${label.y})`}>
        <text className="tube-id" textAnchor={pl.dir === 1 ? 'start' : 'end'}>
          {pipe.id}
        </text>
        <text y={15} className="tube-level" textAnchor={pl.dir === 1 ? 'start' : 'end'}>
          {formatSize(pipe.size)} / {formatSize(pipe.capacity)}
        </text>
      </g>
    </g>
  );
}

function EndCap({ p, plugged }: { p: Pt; plugged: boolean }) {
  return (
    <g>
      <circle cx={p.x} cy={p.y} r={6} className={plugged ? 'tube-plug' : 'tube-end'} />
      {plugged && (
        <text x={p.x} y={p.y + 20} textAnchor="middle" className="tube-plug-label">
          fd sin cerrar
        </text>
      )}
    </g>
  );
}
