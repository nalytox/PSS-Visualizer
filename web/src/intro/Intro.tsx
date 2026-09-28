// Modo introducción (sección 11): cuatro capítulos animados con controles propios. Con
// prefers-reduced-motion no hay animación: se avanza por fotogramas clave con clic.
import { ChevronLeft, ChevronRight, Pause, Play, SkipBack, SkipForward, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Defs } from '../player/components/Defs.tsx';
import { CHAPTERS, VIEW } from './chapters.tsx';
import { captionAt, keyframes } from './engine.ts';

const SPEEDS = [0.5, 0.75, 1, 1.5, 2];

function prefersReducedMotion(): boolean {
  return typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
}

export function Intro({ onClose, onTry }: { onClose: () => void; onTry: (example: string) => void }) {
  const [chapter, setChapter] = useState(0);
  const [t, setT] = useState(0);
  const reduced = useMemo(prefersReducedMotion, []);
  const [playing, setPlaying] = useState(!reduced);
  const [speed, setSpeed] = useState(1);
  const ch = CHAPTERS[chapter];
  const duration = ch.timeline.duration;
  const frames = useMemo(() => keyframes(ch.captions), [ch]);
  const dialog = useRef<HTMLDivElement>(null);

  useEffect(() => {
    dialog.current?.focus();
  }, []);

  useEffect(() => {
    if (!playing || reduced) return;
    let last = performance.now();
    let raf = 0;
    const tick = (now: number) => {
      const dt = (now - last) / 1000;
      last = now;
      setT((x) => {
        const next = Math.min(duration, x + dt * speed);
        if (next >= duration) setPlaying(false);
        return next;
      });
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, reduced, speed, duration]);

  const goChapter = (k: number) => {
    const next = Math.max(0, Math.min(CHAPTERS.length - 1, k));
    setChapter(next);
    setT(reduced ? keyframes(CHAPTERS[next].captions)[0] : 0);
    setPlaying(!reduced);
  };

  useEffect(() => {
    if (reduced) setT(frames[0]);
  }, [reduced, frames]);

  const frameIndex = frames.findIndex((f) => f >= t - 0.02);
  const nextFrame = () => {
    const k = frames.findIndex((f) => f > t + 0.02);
    if (k >= 0) setT(frames[k]);
    else if (chapter < CHAPTERS.length - 1) goChapter(chapter + 1);
  };
  const prevFrame = () => {
    const k = [...frames].reverse().findIndex((f) => f < t - 0.02);
    if (k >= 0) setT(frames[frames.length - 1 - k]);
  };

  const values = ch.timeline.values(t);
  const caption = captionAt(ch.captions, t);
  const ended = t >= duration - 0.05 || (reduced && frameIndex === frames.length - 1);

  const onKey = (e: React.KeyboardEvent) => {
    // La introducción tapa al visualizador: sus atajos no deben llegar al reproductor de atrás.
    e.stopPropagation();
    if (e.key === 'Escape') onClose();
    else if (e.key === ' ' && (e.target as HTMLElement).tagName !== 'BUTTON') {
      e.preventDefault();
      if (reduced) nextFrame();
      else setPlaying((p) => !p);
    } else if (e.key === 'ArrowRight' && reduced) nextFrame();
    else if (e.key === 'ArrowLeft' && reduced) prevFrame();
  };

  return (
    <div className="intro" role="dialog" aria-modal="true" aria-label="Introducción" tabIndex={-1} ref={dialog} onKeyDown={onKey}>
      <div className="intro-card">
        <header className="intro-head">
          <nav className="intro-chapters" aria-label="Capítulos">
            {CHAPTERS.map((c, k) => (
              <button key={c.id} type="button" className={k === chapter ? 'active' : ''} aria-current={k === chapter ? 'step' : undefined} onClick={() => goChapter(k)}>
                {k + 1}. {c.title}
              </button>
            ))}
          </nav>
          <button type="button" className="ghost intro-skip" onClick={onClose}>
            <X size={16} /> Saltar introducción
          </button>
        </header>

        <svg
          className="intro-stage"
          viewBox={`0 0 ${VIEW.w} ${VIEW.h}`}
          role="img"
          aria-label={`${ch.title}: ${caption?.text ?? ''}`}
          onClick={reduced ? nextFrame : undefined}
        >
          <Defs />
          {ch.render(values, t)}
        </svg>

        <p className="intro-caption" aria-live="polite">
          {caption?.text}
        </p>

        <div className="intro-controls" role="toolbar" aria-label="Controles de la introducción">
          <button type="button" onClick={() => goChapter(chapter - 1)} disabled={chapter === 0} aria-label="Capítulo anterior" title="Capítulo anterior">
            <SkipBack size={18} />
          </button>
          {reduced ? (
            <>
              <button type="button" onClick={prevFrame} disabled={frameIndex <= 0} aria-label="Fotograma anterior" title="Fotograma anterior (←)">
                <ChevronLeft size={18} />
              </button>
              <span className="intro-frames">
                {Math.max(0, frameIndex) + 1} / {frames.length}
              </span>
              <button type="button" onClick={nextFrame} aria-label="Fotograma siguiente" title="Fotograma siguiente (→ o clic)">
                <ChevronRight size={18} />
              </button>
            </>
          ) : (
            <button
              type="button"
              className="primary"
              onClick={() => {
                if (ended) setT(0);
                setPlaying((p) => !p || ended);
              }}
              aria-label={playing ? 'Pausar' : 'Reproducir'}
              title="Reproducir o pausar (Espacio)"
            >
              {playing ? <Pause size={18} /> : <Play size={18} />}
            </button>
          )}
          <button type="button" onClick={() => goChapter(chapter + 1)} disabled={chapter === CHAPTERS.length - 1} aria-label="Capítulo siguiente" title="Capítulo siguiente">
            <SkipForward size={18} />
          </button>
          {!reduced && (
            <>
              <input
                type="range"
                className="intro-progress"
                min={0}
                max={duration}
                step={0.05}
                value={t}
                aria-label="Progreso del capítulo"
                aria-valuetext={`${Math.round(t)} de ${Math.round(duration)} segundos`}
                onChange={(e) => {
                  setPlaying(false);
                  setT(Number(e.target.value));
                }}
              />
              <label className="speed">
                <span>Velocidad</span>
                <select value={speed} onChange={(e) => setSpeed(Number(e.target.value))}>
                  {SPEEDS.map((s) => (
                    <option key={s} value={s}>
                      {String(s).replace('.', ',')}x
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          {ended && (
            <button type="button" className="run intro-try" onClick={() => onTry(ch.example)}>
              <Play size={15} /> Probar este ejemplo
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
