// Cables entre puertos de fd y extremos de tubos. Al cerrar un fd el cable se desenchufa.
import { AnimatePresence, motion } from 'motion/react';
import type { CableLayout } from '../layout/sceneLayout.ts';
import { inkOf, useScene } from '../SceneContext.tsx';

const EASE = [0.4, 0, 0.2, 1] as const;

export function Cables({ cables, animate }: { cables: CableLayout[]; animate: boolean }) {
  const { index } = useScene();
  const duration = animate ? 0.35 : 0;
  return (
    <g className="cables">
      <AnimatePresence initial={false}>
        {cables.map((c) => (
          <motion.g key={c.key} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} transition={{ duration }}>
            <motion.path
              className="cable-halo"
              initial={{ d: c.d, pathLength: 0 }}
              animate={{ d: c.d, pathLength: 1 }}
              exit={{ pathLength: 0 }}
              transition={{ duration, ease: EASE }}
            />
            <motion.path
              className={`cable ${c.end === 'w' ? 'write' : 'read'}`}
              style={{ stroke: inkOf(index, c.pid) }}
              initial={{ d: c.d, pathLength: 0 }}
              animate={{ d: c.d, pathLength: 1 }}
              exit={{ pathLength: 0 }}
              transition={{ duration, ease: EASE }}
            />
          </motion.g>
        ))}
      </AnimatePresence>
    </g>
  );
}
