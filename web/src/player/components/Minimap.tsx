// Minimapa (sección 5.4): con más de ocho procesos, un plano de todo el árbol con el recuadro de lo
// que se ve. Un clic o un arrastre centra la vista en ese punto.
import { useRef } from 'react';
import type { SceneLayout } from '../layout/sceneLayout.ts';
import { inkOf, useScene } from '../SceneContext.tsx';

const MAX_W = 200;
const MAX_H = 130;

export function Minimap(props: {
  scene: SceneLayout;
  view: { x: number; y: number; k: number };
  size: { w: number; h: number };
  onCenter: (x: number, y: number) => void;
}) {
  const { scene, view, size, onCenter } = props;
  const { index, trace, player } = useScene();
  const dragging = useRef(false);
  const b = scene.bounds;
  const m = Math.min(MAX_W / b.w, MAX_H / b.h);
  const w = b.w * m;
  const h = b.h * m;
  const vx = (-view.x / view.k - b.x) * m;
  const vy = (-view.y / view.k - b.y) * m;
  const vw = (size.w / view.k) * m;
  const vh = (size.h / view.k) * m;
  const actor = trace.steps[player.t].actor?.pid;

  const center = (e: React.PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    onCenter((e.clientX - r.left) / m + b.x, (e.clientY - r.top) / m + b.y);
  };

  return (
    <div className="minimap" aria-hidden="true">
      <svg
        width={w}
        height={h}
        onPointerDown={(e) => {
          e.stopPropagation();
          dragging.current = true;
          e.currentTarget.setPointerCapture(e.pointerId);
          center(e);
        }}
        onPointerMove={(e) => dragging.current && center(e)}
        onPointerUp={() => (dragging.current = false)}
      >
        {[...scene.boxes.values()].map((box) => (
          <rect
            key={box.pid}
            x={(box.x - b.x) * m}
            y={(box.y - b.y) * m}
            width={Math.max(2, box.w * m)}
            height={Math.max(2, box.h * m)}
            rx={box.compact ? (box.h * m) / 2 : 2}
            className={`minimap-box${box.pid === actor ? ' actor' : ''}${box.compact ? ' reaped' : ''}`}
            style={{ fill: inkOf(index, box.pid) }}
          />
        ))}
        <rect x={vx} y={vy} width={vw} height={vh} className="minimap-view" />
      </svg>
    </div>
  );
}
