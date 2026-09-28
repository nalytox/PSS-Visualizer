// Generador de señal (sección 8): bloque externo con un rayo, cableado al puerto del proceso destino.
import { motion } from 'motion/react';
import { Clock, Cpu, Keyboard, Zap } from 'lucide-react';
import type { SignalLayout } from '../layout/sceneLayout.ts';
import { tipProps, useScene } from '../SceneContext.tsx';

const ERRORS = ['SIGSEGV', 'SIGFPE', 'SIGPIPE', 'SIGBUS', 'SIGILL'];
const UNCATCHABLE = ['SIGKILL', 'SIGSTOP'];

export function signalTone(sig: string): 'kill' | 'error' | 'user' {
  if (UNCATCHABLE.includes(sig)) return 'kill';
  if (ERRORS.includes(sig)) return 'error';
  return 'user';
}

function sourceText(s: SignalLayout): string {
  switch (s.source.kind) {
    case 'process':
      return s.source.via === 'raise' ? `raise() en PID ${s.source.pid}` : `kill() desde PID ${s.source.pid}`;
    case 'kernel':
      return 'Kernel';
    case 'timer':
      return 'alarm()';
    case 'terminal':
      return 'Terminal';
  }
}

export function SignalBlock({ s, animate }: { s: SignalLayout; animate: boolean }) {
  const ctx = useScene();
  const tone = signalTone(s.signal);
  const Icon = s.source.kind === 'kernel' ? Cpu : s.source.kind === 'timer' ? Clock : s.source.kind === 'terminal' ? Keyboard : Zap;
  const status =
    s.status === 'pending'
      ? 'Pendiente: todavía no se entrega.'
      : s.status === 'sending'
        ? 'Enviándose en este paso.'
        : 'Entregada en este paso.';
  return (
    <motion.g
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: animate ? 0.12 : 0 }}
      className={`signal ${tone} ${s.status}`}
    >
      <path d={s.cable.d} className="signal-cable" />
      <g {...tipProps(ctx, { title: `${s.signal} → proceso ${s.to}`, body: `${sourceText(s)}. ${status}${tone === 'kill' ? ' No se puede capturar ni ignorar.' : ''}` })}>
        {tone === 'kill' && <rect x={s.x - 4} y={s.y - 4} width={s.w + 8} height={s.h + 8} rx={15} className="signal-double" />}
        <rect x={s.x} y={s.y} width={s.w} height={s.h} rx={12} className="signal-body" />
        <circle cx={s.x + 22} cy={s.y + s.h / 2} r={14} className="signal-icon-bg" />
        <Icon x={s.x + 14} y={s.y + s.h / 2 - 8} width={16} height={16} className="signal-icon" />
        <text x={s.x + 44} y={s.y + 20} className="signal-name">
          {s.signal}
        </text>
        <text x={s.x + 44} y={s.y + 35} className="signal-source">
          {sourceText(s)}
        </text>
      </g>
    </motion.g>
  );
}
