// Animaciones de eventos del paso actual: cápsulas que viajan por cables y tubos, pulsos de señal.
// Son función del progreso p ∈ [0, 1]; hacia atrás se reproducen con 1 − p.
import { Zap } from 'lucide-react';
import { capsules } from '../../trace/bytes.ts';
import type { PipeLayout, Pt, SceneLayout } from '../layout/sceneLayout.ts';
import { bezierPoint } from '../layout/sceneLayout.ts';
import { useScene } from '../SceneContext.tsx';
import { Capsule, capsuleRest } from './PipeTube.tsx';

const MAX_FLYING = 5;
const STAGGER = 0.07;

const clamp = (x: number) => Math.min(1, Math.max(0, x));
const lerp = (a: Pt, b: Pt, u: number): Pt => ({ x: a.x + (b.x - a.x) * u, y: a.y + (b.y - a.y) * u });
const ease = (u: number) => (u < 0.5 ? 2 * u * u : 1 - (-2 * u + 2) ** 2 / 2);

export interface HiddenCapsules {
  head: number;
  tail: number;
}

// Cuántas cápsulas estáticas ocultar mientras otras vuelan, para no dibujarlas dos veces.
export function hiddenCapsules(step: { events: { type: string; pipe?: string; n?: number }[] }, dir: 1 | -1, active: boolean): Map<string, HiddenCapsules> {
  const out = new Map<string, HiddenCapsules>();
  if (!active) return out;
  for (const ev of step.events) {
    if (!ev.pipe || ev.n === undefined) continue;
    const h = out.get(ev.pipe) ?? { head: 0, tail: 0 };
    if (ev.type === 'write' && dir === 1) h.tail += ev.n;
    if (ev.type === 'read' && dir === -1) h.head += ev.n;
    out.set(ev.pipe, h);
  }
  return out;
}

export function Overlays({ scene }: { scene: SceneLayout }) {
  const { trace, player } = useScene();
  const anim = player.anim;
  if (!anim || player.progress >= 1) return null;
  const step = trace.steps[anim.step];
  const p = anim.dir === 1 ? player.progress : 1 - player.progress;
  const items: React.ReactNode[] = [];
  const tubeOf = (id: string): PipeLayout | undefined => scene.pipes.find((x) => x.id === id);

  // El estado de salida del hijo recogido viaja hacia el padre por la línea de wait.
  for (const r of anim.step === player.t ? scene.reaps : []) {
    const pos = bezierPoint(r.curve.from, r.curve.c1, r.curve.c2, r.curve.to, ease(p));
    const w = r.label.length * 7 + 22;
    items.push(
      <g key={r.key} className="reap-token" transform={`translate(${pos.x},${pos.y})`}>
        <rect x={-w / 2} y={-11} width={w} height={22} rx={11} />
        <text y={4} textAnchor="middle">
          {r.label}
        </text>
      </g>,
    );
  }

  step.events.forEach((ev, ei) => {
    if (ev.type === 'write' && ev.pipe) {
      const cable = scene.cables.find((c) => c.pid === ev.pid && c.fd === ev.fd);
      const tube = tubeOf(ev.pipe);
      if (!cable || !tube) return;
      const after = step.pipes.find((x) => x.id === ev.pipe);
      const total = after ? capsules(after.buffer).length : 0;
      const chars = capsules(ev.bytes).slice(0, MAX_FLYING);
      chars.forEach((ch, k) => {
        const q = ease(clamp((p - k * STAGGER) / (1 - MAX_FLYING * STAGGER)));
        const rest = capsuleRest(tube, Math.max(0, total - capsules(ev.bytes).length + k));
        const pos = q < 0.6 ? bezierPoint(cable.from, cable.c1, cable.c2, cable.to, q / 0.6) : lerp(cable.to, rest, (q - 0.6) / 0.4);
        items.push(<Capsule key={`w-${ei}-${k}`} p={pos} ch={ch} />);
      });
    }
    if (ev.type === 'read' && ev.pipe) {
      const cable = scene.cables.find((c) => c.pid === ev.pid && c.fd === ev.fd);
      const tube = tubeOf(ev.pipe);
      if (!cable || !tube) return;
      if (ev.eof) {
        const q = ease(p);
        items.push(<Capsule key={`eof-${ei}`} p={bezierPoint(cable.to, cable.c2, cable.c1, cable.from, q)} ch="EOF" special />);
        return;
      }
      const chars = capsules(ev.bytes).slice(0, MAX_FLYING);
      chars.forEach((ch, k) => {
        const q = ease(clamp((p - k * STAGGER) / (1 - MAX_FLYING * STAGGER)));
        const rest = capsuleRest(tube, k);
        const pos = q < 0.35 ? lerp(rest, cable.to, q / 0.35) : bezierPoint(cable.to, cable.c2, cable.c1, cable.from, (q - 0.35) / 0.65);
        items.push(<Capsule key={`r-${ei}-${k}`} p={pos} ch={ch} />);
      });
    }
    if (ev.type === 'signalSend') {
      const s = scene.signals.find((x) => x.signal === ev.signal && x.to === ev.to);
      if (!s) return;
      const q = ease(p);
      const pos = bezierPoint(s.cable.from, s.cable.c1, s.cable.c2, s.cable.to, q);
      items.push(
        <g key={`sig-${ei}`} className="pulse" transform={`translate(${pos.x},${pos.y})`}>
          <circle r={12} className="pulse-glow" />
          <Zap x={-8} y={-8} width={16} height={16} className="pulse-bolt" />
        </g>,
      );
    }
  });
  return <g className="overlays">{items}</g>;
}
