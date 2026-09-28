import type { Tip } from '../SceneContext.tsx';

export interface TipState {
  tip: Tip;
  x: number;
  y: number;
}

export function Tooltip({ state }: { state: TipState | null }) {
  if (!state) return null;
  return (
    <div className="tooltip" role="tooltip" style={{ left: state.x, top: state.y }}>
      <div className="tooltip-title">{state.tip.title}</div>
      {state.tip.body && <div className="tooltip-body">{state.tip.body}</div>}
    </div>
  );
}
