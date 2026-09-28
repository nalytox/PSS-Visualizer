// Los cuatro capítulos de la introducción. Cada uno es un guion (tweens + subtítulos) y una función
// que dibuja el fotograma con las mismas piezas visuales del visualizador: cuadrado de proceso,
// carril de hilo, tubo con cápsulas y bloque de señal.
import type { ReactNode } from 'react';
import { Zap } from 'lucide-react';
import { Capsule } from '../player/components/PipeTube.tsx';
import { type Caption, type Timeline, type Tween, type Values, hide, show, timeline, tw, typed } from './engine.ts';

export interface Chapter {
  id: string;
  title: string;
  example: string; // programa de la galería que muestra lo mismo
  captions: Caption[];
  timeline: Timeline;
  render: (v: Values, t: number) => ReactNode;
}

export const VIEW = { w: 900, h: 470 };

function Box(props: { x: number; y: number; w: number; h: number; pid: number; sub: string; hue: number; o?: number; chip?: string; children?: ReactNode; running?: boolean }) {
  const { x, y, w, h, pid, sub, hue, o = 1, chip, children, running } = props;
  return (
    <g transform={`translate(${x},${y})`} opacity={o} className={`proc ${running ? 'running' : 'ready'}`}>
      <rect width={w} height={h} rx={14} className="proc-body" style={{ stroke: `var(--proc-${hue}-ink)` }} filter={running ? 'url(#glow)' : undefined} />
      <path d={`M0,14 a14,14 0 0 1 14,-14 h${w - 28} a14,14 0 0 1 14,14 v32 h${-w} z`} style={{ fill: `var(--proc-${hue})` }} />
      <text x={16} y={21} className="proc-pid">
        PID {pid}
      </text>
      <text x={16} y={37} className="proc-ppid">
        {sub}
      </text>
      {chip && (
        <g>
          <rect x={w - 16 - chip.length * 7.2 - 26} y={11} width={chip.length * 7.2 + 26} height={24} rx={12} className={`chip ${running ? 'running' : 'ready'}`} />
          <text x={w - 16 - (chip.length * 7.2 + 26) / 2} y={27} textAnchor="middle" className="chip-text">
            {chip}
          </text>
        </g>
      )}
      {children}
    </g>
  );
}

function Console({ x, y, w, text }: { x: number; y: number; w: number; text: string }) {
  return (
    <g>
      <rect x={x} y={y} width={w} height={34} rx={10} className="console-bg" />
      <text x={x + 10} y={y + 14} className="console-label">
        consola
      </text>
      <text x={x + 10} y={y + 28} className="console-line">
        {text || ' '}
      </text>
    </g>
  );
}

function CodeLine({ x, y, text, o = 1 }: { x: number; y: number; text: string; o?: number }) {
  return (
    <g opacity={o}>
      <rect x={x} y={y - 15} width={text.length * 8.4 + 20} height={22} rx={11} className="now-running" />
      <text x={x + 10} y={y} className="now-text code strong">
        {text}
      </text>
    </g>
  );
}

function Pill({ x, y, text, o }: { x: number; y: number; text: string; o: number }) {
  return (
    <g className="fork-ret" opacity={o}>
      <rect x={x} y={y} width={text.length * 7.4 + 18} height={20} rx={10} />
      <text x={x + 9} y={y + 14}>
        {text}
      </text>
    </g>
  );
}

// ---------- Forks ----------

const forkCaptions: Caption[] = [
  { start: 0, end: 7, text: 'Este cuadrado es un proceso: un programa en ejecución, con su PID, su memoria y su consola.' },
  { start: 7, end: 15, text: 'fork crea una copia casi exacta del proceso; la única diferencia es lo que devuelve.' },
  { start: 15, end: 23, text: 'En el padre, fork devuelve el PID del hijo. En el hijo, devuelve 0: así cada uno sabe quién es.' },
  { start: 23, end: 31, text: 'Desde aquí cada uno sigue por su cuenta y escribe en su propia consola.' },
];

const forkTweens: Tween[] = [
  show('parent', 0.3, 0.8),
  show('code', 2, 0.6),
  show('child', 7.5, 0.4),
  tw('child', 'x', 70, 500, 7.5, 10.5),
  tw('child', 'y', 40, 268, 7.5, 10.5),
  tw('flash', 'o', 0, 0.8, 10.5, 11.2),
  tw('flash', 'o', 0.8, 0, 11.2, 13),
  show('ret-p', 15.5),
  show('ret-c', 17),
  tw('pcon', 'k', 0, 1, 23.5, 25.5, 'linear'),
  tw('ccon', 'k', 0, 1, 26, 28, 'linear'),
];

function forkRender(v: Values) {
  const cx = v('child', 'x', 70);
  const cy = v('child', 'y', 40);
  const mem = (x: number, y: number, pid: string) => (
    <g transform={`translate(${x},${y})`}>
      <rect width={150} height={52} rx={8} className="mem-panel stack" />
      <text x={10} y={18} className="mem-title">
        main()
      </text>
      <text x={10} y={40} className="mem-name">
        pid
      </text>
      <rect x={50} y={27} width={70} height={18} rx={4} className="mem-cell" />
      <text x={58} y={40} className="mem-value">
        {pid}
      </text>
    </g>
  );
  return (
    <g>
      <path d={`M240,236 C240,${(236 + cy) / 2} ${cx + 170},${(236 + cy) / 2} ${cx + 170},${cy}`} className="tree-edge" opacity={cy > 240 ? v('child', 'o') : 0} />
      <g opacity={v('child', 'o')}>
        <Box x={cx} y={cy} w={340} h={196} pid={1001} sub="hijo de 1000" hue={1} chip="Listo">
          <CodeLine x={16} y={76} text="pid = fork();" />
          {mem(16, 96, v('ret-c', 'o') > 0.5 ? '0' : '?')}
          <rect x={14} y={94} width={154} height={56} rx={9} className="mem-cell changed" opacity={v('flash', 'o')} />
          <Pill x={200} y={62} text="fork() = 0" o={v('ret-c', 'o')} />
          <Console x={14} y={152} w={312} text={typed('soy el hijo', v('ccon', 'k'))} />
        </Box>
      </g>
      <Box x={70} y={40} w={340} h={196} pid={1000} sub="proceso inicial" hue={0} o={v('parent', 'o')} chip="Ejecutando" running>
        <CodeLine x={16} y={76} text="pid = fork();" o={v('code', 'o')} />
        {mem(16, 96, v('ret-p', 'o') > 0.5 ? '1001' : '?')}
        <Pill x={200} y={62} text="fork() = 1001" o={v('ret-p', 'o')} />
        <Console x={14} y={152} w={312} text={typed('soy el padre', v('pcon', 'k'))} />
      </Box>
    </g>
  );
}

// ---------- Threads ----------

const LANE = { main: 110, t1: 230, t2: 350 };
const X0 = 150;
const X1 = 840;

const threadCaptions: Caption[] = [
  { start: 0, end: 7, text: 'Un hilo es una línea de ejecución dentro de un proceso. Todos los hilos comparten la misma memoria.' },
  { start: 7, end: 14, text: 'pthread_create abre dos ramas nuevas: dos hilos más dentro del mismo proceso.' },
  { start: 14, end: 22, text: 'El planificador los turna: en cada paso avanza uno solo, el que tiene el tramo de color.' },
  { start: 22, end: 30, text: 'T1 toma el mutex y su tramo se subraya. T2 también lo quiere y queda esperando, en ámbar.' },
  { start: 30, end: 37, text: 'Al final, pthread_join espera a cada hilo y las ramas vuelven a unirse.' },
];

// Turnos: (carril, inicio del tramo en segundos). Cada tramo dura 1,4 s y avanza 50 px.
const TURNS: [keyof typeof LANE, number][] = [
  ['main', 1], ['main', 2.5], ['main', 8], ['main', 10], ['t1', 14.5], ['t2', 16], ['t1', 17.5], ['t2', 19], ['t1', 20.5], ['t1', 23], ['t1', 24.5], ['t1', 26], ['t2', 27.5], ['t2', 29], ['main', 32.5],
];
const turnX = (k: number) => X0 + 40 + k * 44;

const threadTweens: Tween[] = [
  show('lanes', 0.2, 0.8),
  tw('cursor', 'x', X0 + 40, turnX(TURNS.length), 1, 34, 'linear'),
  show('t1', 8.5),
  show('t2', 10.5),
  show('mutex', 22.5),
  show('wait', 23.5),
  hide('wait', 29.2),
  show('join', 31),
];

function threadRender(v: Values, t: number) {
  const cursor = v('cursor', 'x');
  const lane = (name: keyof typeof LANE, label: string, tid: number, hue: number, o = 1) => (
    <g opacity={o}>
      <circle cx={X0 - 70} cy={LANE[name]} r={6} style={{ fill: `var(--proc-${hue}-ink)` }} />
      <text x={X0 - 58} y={LANE[name] - 2} className="lane-name">
        {label}
      </text>
      <text x={X0 - 58} y={LANE[name] + 11} className="lane-tid">
        tid {tid}
      </text>
      <line x1={name === 'main' ? X0 : 250} x2={Math.min(X1, cursor)} y1={LANE[name]} y2={LANE[name]} className="lane-idle" />
    </g>
  );
  const hueOf = { main: 0, t1: 3, t2: 7 } as const;
  return (
    <g opacity={v('lanes', 'o')}>
      <rect x={40} y={40} width={820} height={400} rx={14} className="proc-body" />
      <text x={60} y={68} className="proc-pid">
        PID 1000 · un proceso, tres hilos
      </text>
      {lane('main', 'principal', 1000, 0)}
      {lane('t1', 'T1', 1001, 3, v('t1', 'o'))}
      {lane('t2', 'T2', 1002, 7, v('t2', 'o'))}
      <path d={`M${X0 + 110},${LANE.main} C${X0 + 130},${LANE.main} ${230},${LANE.t1} ${250},${LANE.t1}`} className="lane-branch" style={{ stroke: 'var(--proc-3-ink)' }} opacity={v('t1', 'o')} />
      <path d={`M${X0 + 150},${LANE.main} C${X0 + 180},${LANE.main} ${230},${LANE.t2} ${250},${LANE.t2}`} className="lane-branch" style={{ stroke: 'var(--proc-7-ink)' }} opacity={v('t2', 'o')} />
      {TURNS.map(([name, start], k) => {
        const u = Math.min(1, Math.max(0, (t - start) / 1.4));
        if (u <= 0) return null;
        const x = turnX(k);
        return <line key={k} x1={x} x2={x + 44 * u} y1={LANE[name]} y2={LANE[name]} className="lane-run" style={{ stroke: `var(--proc-${hueOf[name]}-ink)` }} />;
      })}
      <g opacity={v('mutex', 'o')}>
        <line x1={turnX(9)} x2={turnX(12)} y1={LANE.t1 + 10} y2={LANE.t1 + 10} className="lane-hold" style={{ stroke: 'var(--mutex-0)' }} />
        <text x={turnX(9)} y={LANE.t1 + 28} className="lane-tid">
          mutex candado
        </text>
      </g>
      <g opacity={v('wait', 'o')}>
        <rect x={turnX(9)} y={LANE.t2 - 6} width={turnX(12) - turnX(9)} height={12} fill="url(#hatch-blocked)" className="lane-blocked" />
        <text x={turnX(9)} y={LANE.t2 - 14} className="lane-tid">
          espera el mutex
        </text>
      </g>
      <g opacity={v('join', 'o')}>
        <path d={`M${turnX(13) + 44},${LANE.t1} C${turnX(14)},${LANE.t1} ${turnX(14)},${LANE.main} ${turnX(14) + 20},${LANE.main}`} className="lane-join" />
        <path d={`M${turnX(14)},${LANE.t2} C${turnX(14) + 20},${LANE.t2} ${turnX(14) + 10},${LANE.main} ${turnX(14) + 30},${LANE.main}`} className="lane-join" />
      </g>
      <line x1={cursor} x2={cursor} y1={80} y2={410} className="now-line" />
      <text x={cursor} y={78} className="now-label" textAnchor="middle">
        ahora
      </text>
    </g>
  );
}

// ---------- Pipes ----------

const TUBE = { cx: 460, cy: 245, len: 210 };
const SQ = Math.SQRT1_2;
const tubeTop = { x: TUBE.cx - (TUBE.len / 2) * SQ, y: TUBE.cy - (TUBE.len / 2) * SQ };
const tubeBottom = { x: TUBE.cx + (TUBE.len / 2) * SQ, y: TUBE.cy + (TUBE.len / 2) * SQ };
const writerPort = { x: 360, y: 115 };
const readerPort = { x: 580, y: 360 };
const rest = (k: number) => ({ x: tubeBottom.x - (26 + k * 28) * SQ, y: tubeBottom.y - (26 + k * 28) * SQ });
const lerp = (a: { x: number; y: number }, b: { x: number; y: number }, u: number) => ({ x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u });
const LETTERS = ['h', 'o', 'l', 'a'];

const pipeCaptions: Caption[] = [
  { start: 0, end: 7, text: 'Un pipe es un tubo con dos extremos: por uno se escribe y por el otro se lee.' },
  { start: 7, end: 16, text: 'El escritor manda bytes: entran por arriba y esperan en el buffer del pipe.' },
  { start: 16, end: 24, text: 'El lector los saca en el mismo orden en que entraron.' },
  { start: 24, end: 31, text: 'Cuando nadie puede escribir más, read devuelve 0: es el fin de archivo (EOF).' },
];

const pipeTweens: Tween[] = [
  show('boxes', 0.2, 0.8),
  show('tube', 2, 0.8),
  ...LETTERS.map((_, k) => tw(`c${k}`, 'in', 0, 1, 8 + k * 1.6, 9.6 + k * 1.6)),
  ...LETTERS.map((_, k) => tw(`c${k}`, 'out', 0, 1, 16.5 + k * 1.5, 18 + k * 1.5)),
  tw('wcable', 'o', 1, 0, 24.5, 25.5),
  tw('eof', 'u', 0, 1, 26, 28.5),
];

function pipePath(k: number, v: Values) {
  const a = v(`c${k}`, 'in');
  const b = v(`c${k}`, 'out');
  if (a <= 0) return null;
  // Entra: del puerto al extremo de arriba y cae hasta su lugar en el buffer (los primeros al fondo).
  const slot = rest(k);
  let p = a < 0.5 ? lerp(writerPort, tubeTop, a * 2) : lerp(tubeTop, slot, (a - 0.5) * 2);
  if (b > 0) p = b < 0.4 ? lerp(slot, tubeBottom, b / 0.4) : lerp(tubeBottom, readerPort, (b - 0.4) / 0.6);
  if (b >= 1) return null;
  return p;
}

function pipeRender(v: Values) {
  const read = LETTERS.filter((_, k) => v(`c${k}`, 'out') >= 1).join('');
  const eof = v('eof', 'u');
  const angle = 45;
  return (
    <g>
      <g opacity={v('boxes', 'o')}>
        <Box x={40} y={40} w={320} h={150} pid={1001} sub="escribe en el pipe" hue={1} chip="Ejecutando" running>
          <CodeLine x={16} y={80} text='write(fd[1], "hola", 4);' />
          <Console x={14} y={104} w={272} text="" />
        </Box>
        <Box x={580} y={300} w={300} h={150} pid={1000} sub="lee del pipe" hue={0} chip="Listo">
          <CodeLine x={16} y={80} text="read(fd[0], buf, 16);" />
          <Console x={14} y={104} w={272} text={read ? `leí "${read}"${eof >= 1 ? ' · EOF' : ''}` : ''} />
        </Box>
      </g>
      <g opacity={v('tube', 'o')} className="pipe-tube">
        <path d={`M${writerPort.x},${writerPort.y} C${writerPort.x + 30},${writerPort.y} ${tubeTop.x - 20},${tubeTop.y - 20} ${tubeTop.x},${tubeTop.y}`} className="cable write" style={{ stroke: 'var(--proc-1-ink)' }} opacity={v('wcable', 'o', 1)} />
        <path d={`M${readerPort.x},${readerPort.y} C${readerPort.x - 30},${readerPort.y} ${tubeBottom.x + 15},${tubeBottom.y + 15} ${tubeBottom.x},${tubeBottom.y}`} className="cable read" style={{ stroke: 'var(--proc-0-ink)' }} />
        <g transform={`translate(${TUBE.cx},${TUBE.cy}) rotate(${angle})`}>
          <rect x={-TUBE.len / 2} y={-17} width={TUBE.len} height={34} rx={17} className="tube-body" />
          <rect x={-TUBE.len / 2} y={-17} width={TUBE.len} height={34} rx={17} fill="url(#glass)" className="tube-glass" />
        </g>
        <text x={TUBE.cx + 40} y={TUBE.cy - 30} className="tube-id">
          p0
        </text>
      </g>
      {LETTERS.map((ch, k) => {
        const p = pipePath(k, v);
        return p ? <Capsule key={ch + k} p={p} ch={ch} /> : null;
      })}
      {eof > 0 && eof < 1 && <Capsule p={lerp(tubeBottom, readerPort, eof)} ch="EOF" special />}
    </g>
  );
}

// ---------- Señales ----------

const sigCaptions: Caption[] = [
  { start: 0, end: 6, text: 'Una señal es un aviso asíncrono: le puede llegar a un proceso en cualquier momento.' },
  { start: 6, end: 12, text: 'kill envía la señal: el pulso viaja por el cable hasta el puerto de señales del proceso.' },
  { start: 12, end: 20, text: 'El proceso interrumpe lo que hacía y ejecuta su handler: su carril se desvía.' },
  { start: 20, end: 27, text: 'Al terminar el handler, el proceso vuelve exactamente a donde estaba.' },
];

const BLOCK = { x: 60, y: 70, w: 190, h: 56 };
const PORT = { x: 420, y: 240 };
const cable = { from: { x: BLOCK.x + BLOCK.w, y: BLOCK.y + BLOCK.h / 2 }, c1: { x: 330, y: 98 }, c2: { x: 340, y: 240 }, to: PORT };
const bez = (u: number) => {
  const { from: a, c1: b, c2: c, to: d } = cable;
  const w = 1 - u;
  return { x: w * w * w * a.x + 3 * w * w * u * b.x + 3 * w * u * u * c.x + u * u * u * d.x, y: w * w * w * a.y + 3 * w * w * u * b.y + 3 * w * u * u * c.y + u * u * u * d.y };
};

const sigTweens: Tween[] = [
  show('block', 0.3),
  show('proc', 0.6),
  tw('lane', 'x', 530, 610, 1, 6, 'linear'),
  show('cable', 5.5),
  tw('pulse', 'u', 0, 1, 7, 11.5, 'inOut'),
  tw('lane', 'x', 610, 730, 12, 19, 'linear'),
  show('handler', 12.5),
  hide('handler', 20),
  tw('lane', 'x', 730, 840, 20, 26, 'linear'),
];

// El handler es un desvío del carril entre x = 620 y x = 720.
const bump = (x: number) => {
  const up = Math.min(1, Math.max(0, (x - 610) / 16));
  const down = Math.min(1, Math.max(0, (736 - x) / 16));
  return 34 * Math.min(up, down);
};

function sigRender(v: Values) {
  const laneY = 330;
  const x = v('lane', 'x', 530);
  const pts: string[] = [];
  for (let px = 530; px <= x; px += 4) pts.push(`${px},${laneY - bump(px)}`);
  const pu = v('pulse', 'u');
  const pp = bez(pu);
  const zig = Math.sin(pu * 40) * 6;
  return (
    <g>
      <g opacity={v('block', 'o')} className="signal user">
        <rect x={BLOCK.x} y={BLOCK.y} width={BLOCK.w} height={BLOCK.h} rx={12} className="signal-body" />
        <circle cx={BLOCK.x + 24} cy={BLOCK.y + BLOCK.h / 2} r={14} className="signal-icon-bg" />
        <Zap x={BLOCK.x + 16} y={BLOCK.y + BLOCK.h / 2 - 8} width={16} height={16} className="signal-icon" />
        <text x={BLOCK.x + 48} y={BLOCK.y + 24} className="signal-name">
          SIGUSR1
        </text>
        <text x={BLOCK.x + 48} y={BLOCK.y + 40} className="signal-source">
          kill() desde 1000
        </text>
      </g>
      <path d={`M${cable.from.x},${cable.from.y} C${cable.c1.x},${cable.c1.y} ${cable.c2.x},${cable.c2.y} ${cable.to.x},${cable.to.y}`} className="signal-cable" opacity={v('cable', 'o')} />
      <Box x={420} y={200} w={440} h={220} pid={1001} sub="manejador(SIGUSR1) instalado" hue={1} o={v('proc', 'o')} chip="Ejecutando" running>
        <circle cx={-12} cy={40} r={7} className={`sig-port-ring${pu > 0.95 && pu < 1 ? ' charged' : ''}`} />
        <CodeLine x={16} y={200} text="while (!llego) pause();" />
      </Box>
      <g opacity={v('proc', 'o')}>
        <text x={440} y={laneY + 4} className="lane-name">
          principal
        </text>
        <polyline points={pts.join(' ')} className="lane-run" style={{ stroke: 'var(--proc-1-ink)', fill: 'none' }} />
        <text x={625} y={laneY - 46} className="lane-tid" opacity={v('handler', 'o')}>
          manejador()
        </text>
      </g>
      {pu > 0 && pu < 1 && (
        <g className="pulse" transform={`translate(${pp.x + zig},${pp.y - zig})`}>
          <circle r={12} className="pulse-glow" />
          <path d="M-3,-8 L4,-1 L-1,0 L3,8 L-4,1 L1,0 Z" className="pulse-bolt" />
        </g>
      )}
    </g>
  );
}

export const CHAPTERS: Chapter[] = [
  { id: 'forks', title: 'Forks', example: '03_fork_simple', captions: forkCaptions, timeline: timeline(forkTweens, 31), render: (v) => forkRender(v) },
  { id: 'threads', title: 'Threads', example: '12_hilos_carrera', captions: threadCaptions, timeline: timeline(threadTweens, 37), render: threadRender },
  { id: 'pipes', title: 'Pipes', example: '06_pipe_padre_hijo', captions: pipeCaptions, timeline: timeline(pipeTweens, 31), render: (v) => pipeRender(v) },
  { id: 'signals', title: 'Señales', example: '09_sigusr1', captions: sigCaptions, timeline: timeline(sigTweens, 27), render: (v) => sigRender(v) },
];
