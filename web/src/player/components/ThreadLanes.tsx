// Carriles de hilos (sección 6): una línea horizontal por hilo sobre el reloj global t.
import { motion } from 'motion/react';
import { Hourglass, Lock, Zap } from 'lucide-react';
import { blockText } from '../../trace/describe.ts';
import { excerpt, processAt, threadAt, threadLabel } from '../../trace/query.ts';
import type { BlockReason, Step, Thread } from '../../trace/types.ts';
import { BOX_PAD, COL_W, LABEL_W, LANE_H, LANES_PAD_TOP, NOW_W, WINDOW } from '../layout/constants.ts';
import type { BoxLayout } from '../layout/sceneLayout.ts';
import { mutexColor, threadInk, tipProps, useScene } from '../SceneContext.tsx';

const HANDLER_LIFT = 11;

function reasonShort(r: BlockReason, label: (tid: number) => string): string {
  switch (r.kind) {
    case 'read':
      return r.stdin ? 'read stdin' : `read ${r.pipe ?? 'fd ' + r.fd}`;
    case 'write':
      return `write ${r.pipe}`;
    case 'wait':
      return r.target === -1 ? 'wait' : `wait ${r.target}`;
    case 'join':
      return `join ${label(r.tid)}`;
    case 'mutex':
      return 'mutex';
    case 'cond':
      return 'cond wait';
    case 'sem':
      return 'sem_wait';
    case 'sleep':
      return 'sleep';
    case 'pause':
      return 'pause';
    case 'sigsuspend':
      return 'sigsuspend';
  }
}

export function ThreadLanes({ box, animate }: { box: BoxLayout; animate: boolean }) {
  const ctx = useScene();
  const { trace, index, player } = ctx;
  const t = player.t;
  const t0 = Math.max(0, t - WINDOW + 1);
  const pid = box.pid;
  const ink = (tid: number) => threadInk(index, pid, tid);
  const x0 = BOX_PAD + LABEL_W;
  const X = (s: number) => x0 + s * COL_W + COL_W / 2;
  const laneY = (i: number) => box.lanesY + LANES_PAD_TOP + i * LANE_H + LANE_H / 2;
  const first = Math.max(0, t0 - 2);
  const range: number[] = [];
  for (let s = first; s <= t; s++) range.push(s);
  const label = (tid: number) => threadLabel(index, tid);
  const laneOf = new Map(box.lanes.map((tid, i) => [tid, i]));

  const stateAt = (s: number, tid: number): Thread | undefined => threadAt(trace.steps[s], pid, tid);
  const yAt = (s: number, tid: number, i: number) => laneY(i) - (stateAt(s, tid)?.inHandler ? HANDLER_LIFT : 0);

  const clipId = `lanes-clip-${pid}`;
  const lanesTop = box.lanesY + 6;
  const lanesBottom = laneY(box.lanes.length - 1) + LANE_H / 2;
  const nowX = x0 + (t - t0) * COL_W + COL_W / 2;
  const current = trace.steps[t];

  return (
    <g className="lanes">
      <clipPath id={clipId}>
        <rect x={x0 - 4} y={lanesTop - 4} width={WINDOW * COL_W + 8} height={lanesBottom - lanesTop + 12} />
      </clipPath>

      {/* Marcas del reloj global */}
      <g clipPath={`url(#${clipId})`}>
        <motion.g initial={false} animate={{ x: -t0 * COL_W }} transition={{ duration: animate ? 0.35 : 0, ease: [0.4, 0, 0.2, 1] }}>
          {range.map((s) => (
            <g key={`tick-${s}`}>
              <line
                x1={X(s)}
                x2={X(s)}
                y1={lanesTop + 10}
                y2={lanesBottom}
                className="lane-grid"
              />
              {s % 5 === 0 && Math.abs(s - t) > 1 && (
                <text x={X(s)} y={lanesTop + 6} className="lane-tick" textAnchor="middle">
                  {s}
                </text>
              )}
            </g>
          ))}

          {box.lanes.map((tid, i) => (
            <LaneHistory key={tid} tid={tid} i={i} range={range} X={X} yAt={yAt} laneY={laneY} laneOf={laneOf} stateAt={stateAt} ink={ink(tid)} pid={pid} />
          ))}
        </motion.g>
      </g>

      {/* Instante actual: la misma línea vertical en todos los cuadrados */}
      <line x1={nowX} x2={nowX} y1={lanesTop + 8} y2={lanesBottom + 2} className="now-line" />
      <text x={nowX} y={lanesTop + 6} className="now-label" textAnchor="middle">
        t={t}
      </text>

      {/* Etiquetas de hilos */}
      {box.lanes.map((tid, i) => {
        const th = stateAt(t, tid);
        const selected = player.selectedTid === tid;
        return (
          <g
            key={`label-${tid}`}
            className={`lane-label${selected ? ' selected' : ''}`}
            {...tipProps(
              ctx,
              { title: `Hilo ${label(tid)} (tid ${tid})`, body: selected ? 'Seleccionado: Alt + → avanza solo este hilo.' : 'Clic para seleccionarlo y avanzar solo este hilo con Alt + →.' },
              { onClick: () => player.selectThread(selected ? null : tid) },
            )}
          >
            <rect x={BOX_PAD - 4} y={laneY(i) - 13} width={LABEL_W - 6} height={26} rx={13} className="lane-label-bg" />
            <circle cx={BOX_PAD + 7} cy={laneY(i)} r={5} style={{ fill: th && th.state !== 'exited' ? ink(tid) : 'var(--border)' }} />
            <text x={BOX_PAD + 17} y={laneY(i) - 1} className="lane-name">
              {label(tid)}
            </text>
            <text x={BOX_PAD + 17} y={laneY(i) + 10} className="lane-tid">
              tid {tid}
            </text>
          </g>
        );
      })}

      {/* Columna "ahora": qué hará cada hilo o por qué espera */}
      {box.lanes.map((tid, i) => (
        <NowCell key={`now-${tid}`} tid={tid} y={laneY(i)} x={x0 + WINDOW * COL_W + 10} th={stateAt(t, tid)} step={current} reasonShort={(r) => reasonShort(r, label)} ink={ink(tid)} pid={pid} laneOf={laneOf} laneY={laneY} nowX={nowX} />
      ))}
    </g>
  );
}

function LaneHistory(props: {
  tid: number;
  i: number;
  range: number[];
  X: (s: number) => number;
  yAt: (s: number, tid: number, i: number) => number;
  laneY: (i: number) => number;
  laneOf: Map<number, number>;
  stateAt: (s: number, tid: number) => Thread | undefined;
  ink: string;
  pid: number;
}) {
  const { tid, i, range, X, yAt, laneY, laneOf, stateAt, ink, pid } = props;
  const ctx = useScene();
  const { trace, index, player } = ctx;
  const parts: React.ReactNode[] = [];

  for (const s of range) {
    const th = stateAt(s, tid);
    const prev = s > 0 ? stateAt(s - 1, tid) : undefined;
    const step = trace.steps[s];
    const y = yAt(s, tid, i);
    if (!th) continue;

    // Creación: una rama nace del carril del hilo creador.
    const created = step.events.find((e) => e.type === 'threadCreate' && e.tid === tid);
    if (created && created.type === 'threadCreate') {
      const ci = laneOf.get(created.creator);
      if (ci !== undefined) {
        const yc = yAt(s, created.creator, ci);
        parts.push(
          <path key={`born-${s}`} d={`M${X(s)},${yc} C${X(s) + 4},${(yc + y) / 2} ${X(s) - 6},${y} ${X(s) + 8},${y}`} className="lane-branch" style={{ stroke: ink }} />,
        );
      }
    }

    if (prev && prev.state !== 'exited' && th.state !== 'exited') {
      const yp = yAt(s - 1, tid, i);
      // El tramo s−1 → s cuenta qué hizo el hilo durante el paso s: si no fue elegido, su estado
      // durante ese paso es el que tenía al terminar el anterior.
      const ran = step.actor?.tid === tid;
      if (!ran && prev.state === 'blocked') {
        parts.push(
          <rect key={`blk-${s}`} x={X(s - 1)} y={laneY(i) - 6} width={X(s) - X(s - 1)} height={12} fill="url(#hatch-blocked)" className="lane-blocked" />,
        );
      } else {
        parts.push(
          <line
            key={`seg-${s}`}
            x1={X(s - 1)}
            y1={yp}
            x2={X(s)}
            y2={y}
            className={ran ? 'lane-run' : prev.state === 'stopped' ? 'lane-stopped' : 'lane-idle'}
            style={ran ? { stroke: ink } : undefined}
          />,
        );
      }
      // Mutex tomado: el tramo se subraya con el color del mutex.
      th.holds.forEach((m, k) => {
        if (!prev.holds.includes(m)) return;
        parts.push(
          <line key={`hold-${s}-${m}`} x1={X(s - 1)} y1={laneY(i) + 9 + k * 4} x2={X(s)} y2={laneY(i) + 9 + k * 4} className="lane-hold" style={{ stroke: mutexColor(index, m) }} />,
        );
      });
    }

    // Fin del hilo: círculo sólido.
    if (th.state === 'exited' && (!prev || prev.state !== 'exited')) {
      parts.push(<circle key={`end-${s}`} cx={X(s)} cy={y} r={5.5} className="lane-end" style={{ fill: ink }} />);
    }

    // Join: la línea del hilo que terminó desemboca en la del que hace join.
    for (const ev of step.events) {
      if (ev.type !== 'join' || ev.tid !== tid) continue;
      const ti = laneOf.get(ev.target);
      if (ti === undefined) continue;
      let e = s - 1;
      while (e > 0 && stateAt(e - 1, ev.target)?.state === 'exited') e--;
      parts.push(
        <path
          key={`join-${s}-${ev.target}`}
          d={`M${X(e)},${laneY(ti)} C${X(e) + 20},${laneY(ti)} ${X(s) - 20},${y} ${X(s) - 6},${y}`}
          className="lane-join"
          markerEnd="url(#arrow-purple)"
        />,
      );
    }

    // Nodo: el hilo dio un paso aquí.
    if (step.actor?.tid === tid && step.actor.pid === pid) {
      const line = step.executed?.line;
      const image = processAt(step, pid)?.image;
      const blackbox = !line && image?.kind === 'blackbox';
      const tip = {
        title: `t=${s} · ${line ? `línea ${line}` : blackbox ? image.path : 'sin línea propia'}`,
        body: line
          ? excerpt(index, line, 60)
          : blackbox
            ? 'Avanzó el programa cargado con exec: no tiene líneas propias que mostrar.'
            : 'El kernel desvió el hilo (por ejemplo, para entregar una señal).',
      };
      parts.push(
        <g
          key={`node-${s}`}
          className="lane-node"
          {...tipProps(ctx, tip, { onClick: () => player.goto(s, false) })}
          onMouseEnter={(e) => {
            ctx.showTip(tip, e.currentTarget);
            player.setHover(s, line ?? null);
          }}
          onMouseLeave={() => {
            ctx.hideTip();
            player.setHover(null, null);
          }}
        >
          <circle cx={X(s)} cy={y} r={9} className="lane-node-hit" />
          {line ? (
            <circle cx={X(s)} cy={y} r={4.5} style={{ fill: ink }} className={s === player.t ? 'lane-node-now' : undefined} />
          ) : blackbox ? (
            <rect x={X(s) - 4.5} y={y - 4.5} width={9} height={9} rx={2} className="lane-node-blackbox" />
          ) : (
            <path d={`M${X(s) - 4},${y} L${X(s)},${y - 5} L${X(s) + 4},${y} L${X(s)},${y + 5} z`} style={{ fill: 'var(--sig-user)' }} />
          )}
          {line && (
            <text x={X(s)} y={y - 8} className="lane-line-no" textAnchor="middle">
              {line}
            </text>
          )}
        </g>,
      );
    }
  }
  return <g>{parts}</g>;
}

function NowCell(props: {
  tid: number;
  y: number;
  x: number;
  th: Thread | undefined;
  step: Step;
  reasonShort: (r: BlockReason) => string;
  ink: string;
  pid: number;
  laneOf: Map<number, number>;
  laneY: (i: number) => number;
  nowX: number;
}) {
  const { tid, y, x, th, step, reasonShort, ink, pid, laneOf, laneY, nowX } = props;
  const ctx = useScene();
  const { index } = ctx;
  const w = NOW_W - 22;
  if (!th) {
    return (
      <text x={x + 4} y={y + 4} className="now-text muted">
        ya no existe
      </text>
    );
  }
  if (th.state === 'exited') {
    return (
      <text x={x + 4} y={y + 4} className="now-text muted">
        terminó
      </text>
    );
  }
  if (th.state === 'blocked' && th.blockedOn) {
    const r = th.blockedOn;
    const Icon = r.kind === 'mutex' ? Lock : Hourglass;
    const text = reasonShort(r);
    const ownerLane = r.kind === 'mutex' && r.owner !== null ? laneOf.get(r.owner) : undefined;
    return (
      <g>
        {ownerLane !== undefined && (
          <path
            d={`M${nowX + 8},${y} C${nowX + 30},${y} ${nowX + 30},${laneY(ownerLane)} ${nowX + 8},${laneY(ownerLane)}`}
            className="mutex-wait-arrow"
            markerEnd="url(#arrow)"
          />
        )}
        <g {...tipProps(ctx, { title: 'Bloqueado', body: blockText(index, step, pid, tid, r) })}>
          <rect x={x} y={y - 11} width={w} height={22} rx={11} className="now-blocked" />
          <Icon x={x + 6} y={y - 7} width={14} height={14} className="now-icon" />
          <text x={x + 24} y={y + 4} className="now-text">
            {text}
          </text>
        </g>
      </g>
    );
  }
  const running = step.actor?.tid === tid;
  const image = processAt(step, pid)?.image;
  if (image?.kind === 'blackbox' && th.line === null) {
    const name = image.argv[0] ?? image.path;
    return (
      <g {...tipProps(ctx, { title: `Ejecuta ${image.path}`, body: 'Un programa sin símbolos: no hay líneas que seguir, solo su salida y sus syscalls.' })}>
        <rect x={x} y={y - 11} width={w} height={22} rx={11} className={running ? 'now-running' : 'now-ready'} style={running ? { stroke: ink } : undefined} />
        <rect x={x + 9} y={y - 5} width={10} height={10} rx={2} className="now-blackbox" />
        <text x={x + 25} y={y + 4} className={`now-text${running ? ' strong' : ''}`}>
          ejecutando {name.length > 16 ? name.slice(0, 15) + '…' : name}
        </text>
      </g>
    );
  }
  const line = th.line;
  const label = `${line ?? '?'} · ${excerpt(index, line, 18)}`;
  return (
    <g {...tipProps(ctx, { title: `Próxima línea: ${line ?? '?'}`, body: excerpt(index, line, 80) })}>
      <rect x={x} y={y - 11} width={w} height={22} rx={11} className={running ? 'now-running' : 'now-ready'} style={running ? { stroke: ink } : undefined} />
      {th.inHandler && <Zap x={x + 5} y={y - 7} width={14} height={14} className="now-icon" />}
      <text x={x + (th.inHandler ? 22 : 9)} y={y + 4} className={`now-text code${running ? ' strong' : ''}`}>
        {label}
      </text>
    </g>
  );
}
