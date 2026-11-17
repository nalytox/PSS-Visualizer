// Tamaño del panel de código: se cambia arrastrando cualquiera de sus esquinas. Las de la derecha
// ensanchan hacia la derecha y las de la izquierda al arrastrar hacia la izquierda (el panel está
// pegado al borde); las de abajo y arriba lo alargan de la misma forma. Doble clic restablece.
import { useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';

export interface CodeSize {
  width: number; // ancho de la columna izquierda
  height: number | null; // alto del panel; null = el que le toque
}

export const DEFAULT_CODE_SIZE: CodeSize = { width: 430, height: null };
const KEY = 'pss-tamano-codigo';
export const MIN_WIDTH = 300;
export const MIN_HEIGHT = 200;
// Lo que queda para el lienzo como mínimo.
const STAGE_MIN = 360;

function load(): CodeSize {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? 'null') as CodeSize | null;
    if (saved && Number.isFinite(saved.width)) return { width: saved.width, height: saved.height ?? null };
  } catch {
    // Sin almacenamiento se usa el tamaño de siempre.
  }
  return DEFAULT_CODE_SIZE;
}

export function useCodeSize() {
  const [size, setSize] = useState<CodeSize>(load);
  const save = (s: CodeSize) => {
    try {
      localStorage.setItem(KEY, JSON.stringify(s));
    } catch {
      // El tamaño vale para esta visita.
    }
  };
  return {
    size,
    setSize,
    save,
    reset: () => {
      setSize(DEFAULT_CODE_SIZE);
      try {
        localStorage.removeItem(KEY);
      } catch {
        // Nada que borrar.
      }
    },
  };
}

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), Math.max(lo, hi));

type Corner = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';
const CORNERS: Corner[] = ['top-left', 'top-right', 'bottom-left', 'bottom-right'];

export function ResizeCorners(props: {
  panel: React.RefObject<HTMLElement | null>;
  size: CodeSize;
  reserved: number; // ancho ocupado por otras columnas (la teoría)
  onResize: (s: CodeSize) => void;
  onCommit: (s: CodeSize) => void;
  onReset: () => void;
}) {
  const drag = useRef<{ x: number; y: number; w: number; h: number; sx: number; sy: number; last: CodeSize } | null>(null);
  // El ancho de la grilla, no el de la ventana: la aplicación tiene un ancho mínimo y puede ser más ancha.
  const maxWidth = () => (props.panel.current?.closest('.main')?.clientWidth ?? window.innerWidth) - STAGE_MIN - props.reserved;
  const maxHeight = () => Math.max(MIN_HEIGHT, window.innerHeight * 2);
  const currentHeight = () => props.panel.current?.getBoundingClientRect().height ?? 400;

  const start = (corner: Corner) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = {
      x: e.clientX,
      y: e.clientY,
      w: props.size.width,
      h: currentHeight(),
      sx: corner.endsWith('right') ? 1 : -1,
      sy: corner.startsWith('bottom') ? 1 : -1,
      last: props.size,
    };
  };
  const move = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    const next = {
      width: Math.round(clamp(d.w + d.sx * (e.clientX - d.x), MIN_WIDTH, maxWidth())),
      height: Math.round(clamp(d.h + d.sy * (e.clientY - d.y), MIN_HEIGHT, maxHeight())),
    };
    d.last = next;
    props.onResize(next);
  };
  const end = () => {
    if (drag.current) props.onCommit(drag.current.last);
    drag.current = null;
  };
  const keys = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 60 : 20;
    const h = props.size.height ?? currentHeight();
    let next: CodeSize | null = null;
    if (e.key === 'ArrowRight') next = { ...props.size, width: props.size.width + step };
    else if (e.key === 'ArrowLeft') next = { ...props.size, width: props.size.width - step };
    else if (e.key === 'ArrowDown') next = { ...props.size, height: h + step };
    else if (e.key === 'ArrowUp') next = { ...props.size, height: h - step };
    else if (e.key === 'Home') {
      e.preventDefault();
      e.stopPropagation();
      props.onReset();
      return;
    }
    if (!next) return;
    e.preventDefault();
    // Las flechas mueven el paso en el reproductor: aquí cambian el tamaño.
    e.stopPropagation();
    next = {
      width: Math.round(clamp(next.width, MIN_WIDTH, maxWidth())),
      height: next.height === null ? null : Math.round(clamp(next.height, MIN_HEIGHT, maxHeight())),
    };
    props.onResize(next);
    props.onCommit(next);
  };

  return (
    <>
      {CORNERS.map((c) => {
        const keyboard = c === 'bottom-right';
        return (
          <div
            key={c}
            className={`resize-corner ${c}`}
            onPointerDown={start(c)}
            onPointerMove={move}
            onPointerUp={end}
            onPointerCancel={end}
            onDoubleClick={props.onReset}
            title="Arrastra para cambiar el tamaño del código (doble clic: tamaño original)"
            {...(keyboard
              ? {
                  role: 'button',
                  tabIndex: 0,
                  'aria-label': 'Cambiar el tamaño del código: flechas para ajustar, Inicio para el tamaño original',
                  onKeyDown: keys,
                }
              : { 'aria-hidden': true })}
          />
        );
      })}
    </>
  );
}
