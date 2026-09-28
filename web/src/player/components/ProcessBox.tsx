// Cuadrado de proceso (sección 5): encabezado, carriles de hilos, memoria, consola y puertos.
import { motion } from 'motion/react';
import { ChevronDown, ChevronRight, Hourglass, Pause, Play, Skull } from 'lucide-react';
import { useMemo } from 'react';
import { decodeBytes } from '../../trace/bytes.ts';
import { blockText, fdText } from '../../trace/describe.ts';
import { outputUpTo, processAt, snapshotOf, threadLabel } from '../../trace/query.ts';
import type { Fd, Process } from '../../trace/types.ts';
import { BOX_PAD, CONSOLE_H, CONSOLE_LINES, HEADER_H, MEM_TITLE_H, PORT_H, PORT_W } from '../layout/constants.ts';
import { layoutMemory } from '../layout/memoryLayout.ts';
import type { BoxLayout, PortLayout } from '../layout/sceneLayout.ts';
import { fillOf, inkOf, threadInk, tipProps, useScene } from '../SceneContext.tsx';
import { MemoryPanel } from './MemoryPanel.tsx';
import { ThreadLanes } from './ThreadLanes.tsx';

const EASE = [0.4, 0, 0.2, 1] as const;

function stateChip(p: Process): { text: string; cls: string } {
  switch (p.state) {
    case 'running':
      return { text: 'Ejecutando', cls: 'running' };
    case 'ready':
      return { text: 'Listo', cls: 'ready' };
    case 'blocked':
      return { text: 'Bloqueado', cls: 'blocked' };
    case 'stopped':
      return { text: 'Detenido', cls: 'stopped' };
    case 'zombie':
      if (p.exit && 'signal' in p.exit) return { text: `Zombie · ${p.exit.signal}`, cls: 'zombie' };
      return { text: `Zombie · salió con ${p.exit && 'code' in p.exit ? p.exit.code : '?'}`, cls: 'zombie' };
    case 'reaped':
      return { text: 'Recogido', cls: 'reaped' };
  }
}

const STD_NAMES: Record<number, string> = { 0: 'stdin', 1: 'stdout', 2: 'stderr' };

function portLabel(fd: number, entry: Fd): string | null {
  const std = STD_NAMES[fd];
  if (std && entry.kind === 'pipe') return `${std} → ${entry.pipe}`;
  if (entry.kind === 'pipe') return `${entry.pipe} ${entry.end === 'r' ? 'lectura' : 'escritura'}`;
  return null;
}

export function ProcessBox(props: { box: BoxLayout; proc: Process; bornFrom?: { x: number; y: number }; animate: boolean }) {
  const { box, proc, bornFrom, animate } = props;
  const ctx = useScene();
  const { trace, index, player } = ctx;
  const t = player.t;
  const step = trace.steps[t];
  const ink = inkOf(index, proc.pid);
  const fill = fillOf(index, proc.pid);
  const transition = { duration: animate ? 0.35 : 0, ease: EASE };

  const events = step.events;
  const forward = player.anim?.dir === 1 && player.anim.step === t;
  const flashHandler = forward && events.some((e) => e.type === 'signalDeliver' && e.pid === proc.pid && e.action === 'handler');
  const flashKill = forward && events.some((e) => e.type === 'signalDeliver' && e.pid === proc.pid && (e.action === 'terminate' || e.action === 'core'));
  const justForked = forward && events.some((e) => e.type === 'fork' && e.child === proc.pid);

  // Vista previa: al pasar sobre un nodo de carril se muestra la memoria de ese instante.
  const memT = player.hoverT ?? t;
  const memProc = processAt(trace.steps[memT], proc.pid) ?? proc;
  const stacks = memProc.threads.map((th) => ({
    tid: th.tid,
    title: memProc.threads.length > 1 ? `hilo ${threadLabel(index, th.tid)}` : '',
    ink: threadInk(index, proc.pid, th.tid),
  }));
  const memLayout = useMemo(() => {
    if (!box.memOpen) return null;
    if (memT === t) return box.mem;
    const snap = snapshotOf(trace, memProc);
    return snap ? layoutMemory(snap, stacks) : null;
  }, [box.mem, box.memOpen, memT, t, memProc.mem, trace]);
  const prevValues = useMemo(() => {
    if (!box.memOpen || memT === 0) return null;
    const prevProc = processAt(trace.steps[memT - 1], proc.pid);
    if (!prevProc || prevProc.mem === memProc.mem) return memLayout?.values ?? null;
    const snap = snapshotOf(trace, prevProc);
    return snap ? layoutMemory(snap, stacks).values : null;
  }, [box.memOpen, memT, memProc.mem, trace, proc.pid, memLayout]);

  const chip = stateChip(proc);
  const blockedThread = proc.threads.find((th) => th.state === 'blocked' && th.blockedOn);
  const chipBody = blockedThread?.blockedOn
    ? blockText(index, step, proc.pid, blockedThread.tid, blockedThread.blockedOn)
    : chip.cls === 'zombie'
      ? 'Terminó, pero su padre todavía no hizo wait: el kernel guarda su código de salida.'
      : chip.cls === 'ready'
        ? 'Puede avanzar, pero el planificador eligió a otra tarea en este paso.'
        : chip.cls === 'running'
          ? 'Es la tarea que avanzó en este paso.'
          : undefined;

  const initial = bornFrom ? { x: bornFrom.x, y: bornFrom.y, opacity: 0.3, scale: 0.92 } : { x: box.x, y: box.y, opacity: 0, scale: 1 };

  if (box.compact) {
    return (
      <motion.g
        initial={initial}
        animate={{ x: box.x, y: box.y, opacity: 1, scale: 1 }}
        exit={{ opacity: 0 }}
        transition={transition}
        className="proc reaped"
        {...tipProps(ctx, { title: `Proceso ${proc.pid} recogido`, body: 'Su padre hizo wait: el kernel liberó sus últimos recursos. Conserva su lugar en el árbol.' })}
      >
        <rect width={box.w} height={box.h} rx={box.h / 2} className="proc-silhouette" style={{ stroke: ink }} />
        <text x={box.w / 2} y={box.h / 2 + 5} textAnchor="middle" className="proc-silhouette-text">
          PID {proc.pid} · recogido
        </text>
      </motion.g>
    );
  }

  const outLines = consoleLines(outputUpTo(trace, t, proc.pid));
  const lastChunkT = trace.output.filter((c) => c.pid === proc.pid && c.t <= t).at(-1)?.t;
  const Chevron = box.memOpen ? ChevronDown : ChevronRight;

  return (
    <motion.g
      initial={initial}
      animate={{ x: box.x, y: box.y, opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.92 }}
      transition={transition}
      className={`proc ${proc.state}`}
    >
      <rect
        width={box.w}
        height={box.h}
        rx={14}
        className={`proc-body${flashHandler ? ' flash-handler' : ''}${flashKill ? ' flash-kill' : ''}`}
        style={proc.state === 'running' || proc.state === 'ready' || proc.state === 'zombie' ? { stroke: ink } : undefined}
        filter={proc.state === 'running' ? 'url(#glow)' : 'url(#soft-shadow)'}
      />
      <path d={`M0,14 a14,14 0 0 1 14,-14 h${box.w - 28} a14,14 0 0 1 14,14 v${HEADER_H - 14} h${-box.w} z`} style={{ fill }} className="proc-header" />
      <g
        className="proc-title"
        {...tipProps(ctx, {
          title: `Proceso ${proc.pid}`,
          body: `${proc.ppid === null ? 'Proceso inicial del programa' : `Hijo de ${proc.ppid}`} · grupo ${proc.pgid} · ${proc.threads.length} ${proc.threads.length === 1 ? 'hilo' : 'hilos'}`,
        })}
      >
        <text x={16} y={21} className="proc-pid">
          PID {proc.pid}
        </text>
        <text x={16} y={37} className="proc-ppid">
          {proc.ppid === null ? 'proceso inicial' : proc.ppid === 1 ? 'huérfano · adoptado por init (1)' : `hijo de ${proc.ppid}`}
          {proc.image.kind === 'blackbox' ? ` · ${proc.image.path}` : ''}
        </text>
      </g>
      <g {...tipProps(ctx, { title: chip.text, body: chipBody })}>
        <rect x={box.w - 16 - chipWidth(chip.text)} y={11} width={chipWidth(chip.text)} height={24} rx={12} className={`chip ${chip.cls}`} />
        {chip.cls === 'blocked' && <Hourglass x={box.w - 10 - chipWidth(chip.text)} y={16} width={14} height={14} className="chip-icon" />}
        {chip.cls === 'running' && <Play x={box.w - 10 - chipWidth(chip.text)} y={16} width={14} height={14} className="chip-icon" />}
        {chip.cls === 'stopped' && <Pause x={box.w - 10 - chipWidth(chip.text)} y={16} width={14} height={14} className="chip-icon" />}
        {chip.cls === 'zombie' && <Skull x={box.w - 10 - chipWidth(chip.text)} y={16} width={14} height={14} className="chip-icon" />}
        <text x={box.w - 16 - chipWidth(chip.text) / 2 + (chip.cls === 'ready' || chip.cls === 'reaped' ? 0 : 8)} y={27} textAnchor="middle" className="chip-text">
          {chip.text}
        </text>
      </g>

      <g className={proc.state === 'zombie' ? 'desaturate' : undefined}>
        <ThreadLanes box={{ ...box, x: 0, y: 0 }} animate={animate} />

        {proc.mem !== null || box.memOpen ? (
          <g>
            <line x1={BOX_PAD} x2={box.w - BOX_PAD} y1={box.memY + 2} y2={box.memY + 2} className="proc-divider" />
            <g className="mem-toggle" {...tipProps(ctx, { title: box.memOpen ? 'Plegar memoria' : 'Mostrar memoria' }, { onClick: () => ctx.toggleMem(proc.pid) })}>
              <rect x={BOX_PAD - 4} y={box.memY + 6} width={130} height={22} rx={11} className="mem-toggle-bg" />
              <Chevron x={BOX_PAD} y={box.memY + 10} width={14} height={14} className="mem-toggle-icon" />
              <text x={BOX_PAD + 18} y={box.memY + 21} className="mem-toggle-text">
                Memoria{memT !== t ? ` · vista de t=${memT}` : ''}
              </text>
            </g>
            {memLayout && <MemoryPanel layout={memLayout} prevValues={prevValues} x={BOX_PAD} y={box.memY + MEM_TITLE_H + 4} flash={justForked} />}
          </g>
        ) : null}

        <g className="console" {...tipProps(ctx, { title: `Consola del proceso ${proc.pid}`, body: 'Lo que este proceso escribió en stdout y stderr (en rojo).' })}>
          <rect x={BOX_PAD - 4} y={box.consoleY + 4} width={box.w - 2 * BOX_PAD + 8} height={CONSOLE_H - 12} rx={10} className="console-bg" />
          <text x={BOX_PAD + 6} y={box.consoleY + 17} className="console-label">
            consola
          </text>
          {outLines.length === 0 && (
            <text x={BOX_PAD + 60} y={box.consoleY + 17} className="console-empty">
              (sin salida todavía)
            </text>
          )}
          {outLines.map((l, i) => (
            <text key={`${i}-${l.text}`} x={BOX_PAD + 6} y={box.consoleY + 32 + i * 15} className={`console-line${l.stderr ? ' stderr' : ''}${l.fresh && lastChunkT === t ? ' fresh' : ''}`}>
              {l.text || ' '}
            </text>
          ))}
        </g>
      </g>

      {box.ports.map((port) => (
        <Port key={port.fd} port={port} box={box} pid={proc.pid} ink={ink} />
      ))}

      <g
        className="sig-port"
        {...tipProps(ctx, {
          title: 'Puerto de señales',
          body: proc.signals.pending.length > 0 ? `Señales pendientes: ${proc.signals.pending.join(', ')}` : describeActions(proc),
        })}
      >
        <line x1={-10} y1={box.sigPort.y - box.y} x2={0} y2={box.sigPort.y - box.y} className="sig-port-stub" />
        <circle cx={-12} cy={box.sigPort.y - box.y} r={7} className={`sig-port-ring${proc.signals.pending.length > 0 ? ' charged' : ''}`} />
        <circle cx={-12} cy={box.sigPort.y - box.y} r={2.8} className="sig-port-core" />
      </g>
    </motion.g>
  );
}

// "a" + "el" se contrae en "al".
function towards(text: string): string {
  return text.startsWith('el ') ? 'al ' + text.slice(3) : 'a ' + text;
}

function describeActions(p: Process): string {
  const entries = Object.entries(p.signals.actions);
  const actions = entries.length === 0 ? 'Todas las señales tienen su acción por defecto.' : entries.map(([s, a]) => (a.action === 'handler' ? `${s} → ${a.fn}()` : `${s} ignorada`)).join(' · ');
  const mask = p.signals.mask.length > 0 ? ` · Máscara: ${p.signals.mask.join(', ')}` : '';
  return actions + mask;
}

function chipWidth(text: string): number {
  return Math.round(text.length * 7.2 + 34);
}

function consoleLines(chunks: { bytes: string; stream: string }[]) {
  const lines: { text: string; stderr: boolean; fresh: boolean }[] = [];
  let current = { text: '', stderr: false, fresh: false };
  chunks.forEach((c, ci) => {
    const text = decodeBytes(c.bytes);
    for (const ch of text) {
      if (ch === '\n') {
        lines.push(current);
        current = { text: '', stderr: false, fresh: false };
      } else {
        current.text += ch;
        current.stderr = c.stream === 'stderr';
        current.fresh = ci === chunks.length - 1;
      }
    }
    if (ci === chunks.length - 1 && lines.length > 0 && current.text === '') lines[lines.length - 1].fresh = true;
  });
  if (current.text) lines.push(current);
  return lines.slice(-CONSOLE_LINES);
}

function Port({ port, box, pid, ink }: { port: PortLayout; box: BoxLayout; pid: number; ink: string }) {
  const ctx = useScene();
  const lx = port.x - box.x;
  const ly = port.y - box.y;
  const x = port.side === 'right' ? lx - PORT_W / 2 : lx - PORT_W / 2;
  const label = portLabel(port.fd, port.entry);
  const std = STD_NAMES[port.fd];
  const warning = ctx.trace.steps[ctx.player.t].pipes.some((p) => p.warnings.some((w) => w.pid === pid && w.fd === port.fd));
  return (
    <g
      className={`port${port.entry.kind === 'pipe' ? ' pipe-port' : ''}${warning ? ' warning' : ''}`}
      {...tipProps(ctx, {
        title: `fd ${port.fd}${std ? ` (${std})` : ''}`,
        body: `Apunta ${towards(fdText(port.entry))}.${warning ? ' Este extremo sigue abierto y nadie lo usa: el lector nunca verá EOF.' : ''}`,
      })}
    >
      <rect x={x} y={ly - PORT_H / 2} width={PORT_W} height={PORT_H} rx={5} className="port-body" style={port.entry.kind === 'pipe' ? { stroke: ink } : undefined} />
      <text x={lx} y={ly + 4} textAnchor="middle" className="port-text">
        {port.fd}
      </text>
      {label && (
        <text x={port.side === 'right' ? lx + PORT_W / 2 + 6 : lx - PORT_W / 2 - 6} y={ly - 10} textAnchor={port.side === 'right' ? 'start' : 'end'} className="port-label">
          {label}
        </text>
      )}
    </g>
  );
}
