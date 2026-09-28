// Relaciones entre procesos que no son el árbol: la espera de wait y la adopción por init (1).
import { AnimatePresence, motion } from 'motion/react';
import { Hourglass } from 'lucide-react';
import type { InitLayout, WaitLayout } from '../layout/sceneLayout.ts';
import { tipProps, useScene } from '../SceneContext.tsx';

export function WaitLines({ waits, animate }: { waits: WaitLayout[]; animate: boolean }) {
  const ctx = useScene();
  return (
    <g className="wait-lines">
      <AnimatePresence initial={false}>
        {waits.map((w) => (
          <motion.g key={w.key} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration: animate ? 0.3 : 0 }}>
            <path d={w.curve.d} className="wait-line" />
            <g
              transform={`translate(${w.mid.x},${w.mid.y})`}
              {...tipProps(ctx, {
                title: `${w.parent} espera a ${w.child}`,
                body: `El proceso ${w.parent} está en wait: seguirá cuando ${w.child} termine y le entregue su código de salida.`,
              })}
            >
              <circle r={13} className="wait-badge" />
              <Hourglass x={-7} y={-7} width={14} height={14} className="wait-icon" />
            </g>
          </motion.g>
        ))}
      </AnimatePresence>
    </g>
  );
}

export function InitNode({ init }: { init: InitLayout }) {
  const ctx = useScene();
  return (
    <g className="init">
      {init.edges.map((e) => (
        <path key={e.key} d={e.d} className="init-edge" />
      ))}
      <g
        transform={`translate(${init.x},${init.y})`}
        {...tipProps(ctx, {
          title: 'init (1)',
          body: 'Cuando un proceso termina antes que sus hijos, init los adopta y recoge a cada uno apenas termina: así no quedan zombies para siempre.',
        })}
      >
        <rect width={init.w} height={init.h} rx={init.h / 2} className="init-body" />
        <text x={init.w / 2} y={init.h / 2 + 5} textAnchor="middle" className="init-text">
          init (1)
        </text>
      </g>
    </g>
  );
}
