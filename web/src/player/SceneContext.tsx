import { createContext, useContext } from 'react';
import type { TraceIndex } from '../trace/query.ts';
import type { Trace } from '../trace/types.ts';
import type { Player } from './usePlayer.ts';

export interface Tip {
  title: string;
  body?: string;
}

export interface SceneCtx {
  trace: Trace;
  index: TraceIndex;
  player: Player;
  showTip: (tip: Tip, el: Element) => void;
  hideTip: () => void;
  toggleMem: (pid: number) => void;
  // Modo manual: elegir qué tarea da el siguiente paso.
  choose?: (task: { pid: number; tid: number }) => void;
}

export const SceneContext = createContext<SceneCtx | null>(null);

export function useScene(): SceneCtx {
  const ctx = useContext(SceneContext);
  if (!ctx) throw new Error('useScene fuera de SceneContext');
  return ctx;
}

export const fillOf = (index: TraceIndex, pid: number) => `var(--proc-${index.hue.get(pid) ?? 0})`;
export const inkOf = (index: TraceIndex, pid: number) => `var(--proc-${index.hue.get(pid) ?? 0}-ink)`;
// El hilo principal lleva el color del proceso; los demás, tonos alejados de la misma rotación.
export const threadInk = (index: TraceIndex, pid: number, tid: number) => {
  const hue = ((index.hue.get(pid) ?? 0) + (index.threadOrder.get(tid) ?? 0) * 3) % 8;
  return `var(--proc-${hue}-ink)`;
};
export const mutexColor = (index: TraceIndex, id: string) => `var(--mutex-${index.mutexHue.get(id) ?? 0})`;

// Props para que un elemento SVG sea inspeccionable con mouse y teclado.
export function tipProps(ctx: SceneCtx, tip: Tip, extra: { onClick?: () => void } = {}) {
  return {
    tabIndex: 0,
    role: extra.onClick ? 'button' : 'img',
    'aria-label': tip.body ? `${tip.title}. ${tip.body}` : tip.title,
    onMouseEnter: (e: React.MouseEvent) => ctx.showTip(tip, e.currentTarget),
    onMouseLeave: () => ctx.hideTip(),
    onFocus: (e: React.FocusEvent) => ctx.showTip(tip, e.currentTarget),
    onBlur: () => ctx.hideTip(),
    onClick: extra.onClick,
    onKeyDown: extra.onClick
      ? (e: React.KeyboardEvent) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            e.stopPropagation();
            extra.onClick!();
          }
        }
      : undefined,
  };
}
