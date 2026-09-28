// Estado de reproducción: paso actual, animación en curso y controles. Toda la interfaz se deriva de t.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { nextEventStep, nextThreadStep, prevEventStep, runToLine } from '../trace/query.ts';
import type { Trace } from '../trace/types.ts';

export interface Anim {
  step: number; // paso cuyos eventos se animan
  dir: 1 | -1; // 1 hacia adelante; -1 reproduce en reversa
  start: number;
  duration: number;
}

export const SPEEDS = [0.25, 0.5, 1, 2, 4];
const BASE_ANIM_MS = 650;
const BASE_PLAY_MS = 1100;
// Si el usuario pide pasos más rápido que esto, se aplica el estado directo, sin animar eventos.
const FAST_MS = 160;

function reducedMotion(): boolean {
  return typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

export interface Player {
  t: number;
  last: number;
  anim: Anim | null;
  progress: number;
  playing: boolean;
  speed: number;
  selectedTid: number | null;
  hoverT: number | null;
  hoverLine: number | null;
  goto: (t: number, animate?: boolean) => void;
  next: () => void;
  prev: () => void;
  first: () => void;
  lastStep: () => void;
  nextEvent: () => void;
  prevEvent: () => void;
  threadStep: () => void;
  toLine: (line: number) => void;
  togglePlay: () => void;
  setSpeed: (s: number) => void;
  selectThread: (tid: number | null) => void;
  setHover: (t: number | null, line: number | null) => void;
}

export function usePlayer(trace: Trace, initialT = 0): Player {
  const last = trace.steps.length - 1;
  const [t, setT] = useState(() => Math.min(Math.max(0, initialT), last));
  const [anim, setAnim] = useState<Anim | null>(null);
  const [progress, setProgress] = useState(1);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(1);
  const [selectedTid, selectThread] = useState<number | null>(null);
  const [hover, setHoverState] = useState<{ t: number | null; line: number | null }>({ t: null, line: null });
  const lastRequest = useRef(0);
  const tRef = useRef(t);
  tRef.current = t;

  useEffect(() => {
    setT(Math.min(Math.max(0, initialT), last));
    setAnim(null);
    setPlaying(false);
    selectThread(null);
  }, [trace, initialT, last]);

  const goto = useCallback(
    (target: number, animate = true) => {
      const from = tRef.current;
      const to = Math.min(Math.max(0, target), last);
      if (to === from) return;
      const now = performance.now();
      const fast = now - lastRequest.current < FAST_MS;
      lastRequest.current = now;
      if (animate && !fast && !reducedMotion() && Math.abs(to - from) === 1) {
        setAnim({ step: to > from ? to : from, dir: to > from ? 1 : -1, start: now, duration: BASE_ANIM_MS / Math.max(1, speed) });
        setProgress(0);
      } else {
        setAnim(null);
        setProgress(1);
      }
      // Dos teclas antes del siguiente render deben sumar dos pasos, no leer el mismo t.
      tRef.current = to;
      setT(to);
    },
    [last, speed],
  );

  // Progreso de la animación de eventos (0 → 1), con requestAnimationFrame.
  useEffect(() => {
    if (!anim) return;
    let raf = 0;
    const tick = () => {
      const p = Math.min(1, (performance.now() - anim.start) / anim.duration);
      setProgress(p);
      if (p < 1) raf = requestAnimationFrame(tick);
      else setAnim(null);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [anim]);

  useEffect(() => {
    if (!playing) return;
    if (t >= last) {
      setPlaying(false);
      return;
    }
    const id = setTimeout(() => goto(t + 1), BASE_PLAY_MS / speed);
    return () => clearTimeout(id);
  }, [playing, t, last, speed, goto]);

  const api = useMemo<Player>(
    () => ({
      t,
      last,
      anim,
      progress,
      playing,
      speed,
      selectedTid,
      hoverT: hover.t,
      hoverLine: hover.line,
      goto,
      next: () => goto(tRef.current + 1),
      prev: () => goto(tRef.current - 1),
      first: () => goto(0, false),
      lastStep: () => goto(last, false),
      nextEvent: () => goto(nextEventStep(trace, tRef.current)),
      prevEvent: () => goto(prevEventStep(trace, tRef.current)),
      threadStep: () => {
        const tid = selectedTid ?? trace.steps[tRef.current].actor?.tid;
        if (tid !== undefined) goto(nextThreadStep(trace, tRef.current, tid));
      },
      toLine: (line: number) => goto(runToLine(trace, tRef.current, line)),
      togglePlay: () => {
        if (tRef.current >= last) goto(0, false);
        setPlaying((p) => !p);
      },
      setSpeed,
      selectThread,
      setHover: (ht, line) => setHoverState({ t: ht, line }),
    }),
    [t, last, anim, progress, playing, speed, selectedTid, hover, goto, trace],
  );
  return api;
}

// Atajos de la sección 10. Se ignoran mientras se escribe en un campo de texto.
export function usePlayerKeys(player: Player) {
  const ref = useRef(player);
  ref.current = player;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      if (el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName))) return;
      const p = ref.current;
      let handled = true;
      if (e.key === 'ArrowRight' && e.shiftKey) p.nextEvent();
      else if (e.key === 'ArrowLeft' && e.shiftKey) p.prevEvent();
      else if (e.key === 'ArrowRight' && e.altKey) p.threadStep();
      else if (e.key === 'ArrowRight') p.next();
      else if (e.key === 'ArrowLeft') p.prev();
      else if (e.key === 'Home') p.first();
      else if (e.key === 'End') p.lastStep();
      else if (e.key === ' ' && (!el || el.tagName !== 'BUTTON')) p.togglePlay();
      else handled = false;
      if (handled) e.preventDefault();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
