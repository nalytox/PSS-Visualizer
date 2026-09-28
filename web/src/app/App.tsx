import { useEffect, useMemo, useState } from 'react';
import { Contrast, GraduationCap, Moon, Sun } from 'lucide-react';
import { CodePanel, type CodeMarks } from '../editor/CodePanel.tsx';
import { Canvas } from '../player/Canvas.tsx';
import { Controls } from '../player/Controls.tsx';
import { Timeline } from '../player/Timeline.tsx';
import { usePlayer, usePlayerKeys } from '../player/usePlayer.ts';
import { threadInk } from '../player/SceneContext.tsx';
import { catalog } from '../trace/catalog.ts';
import { describeStep } from '../trace/describe.ts';
import { indexTrace, threadAt, threadLabel } from '../trace/query.ts';
import type { Trace } from '../trace/types.ts';
import { StdinPanel } from './StdinPanel.tsx';
import { TerminalPanel } from './TerminalPanel.tsx';
import { readHash, writeHash } from './urlState.ts';
import { useTheme } from './useTheme.ts';

export function App() {
  const initial = useMemo(readHash, []);
  const [traceId, setTraceId] = useState(() => (catalog.some((c) => c.id === initial.trace) ? initial.trace! : catalog[0]?.id));
  const [trace, setTrace] = useState<Trace | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const entry = catalog.find((c) => c.id === traceId);
    if (!entry) return;
    let alive = true;
    entry
      .load()
      .then((tr) => alive && setTrace(tr))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [traceId]);

  if (error) return <div className="fatal">No se pudo cargar la traza: {error}</div>;
  if (!trace || !traceId) return <div className="loading">Cargando…</div>;
  return <Visualizer key={traceId} trace={trace} traceId={traceId} onPick={setTraceId} initialT={traceId === initial.trace ? initial.t : 0} />;
}

function Visualizer({ trace, traceId, onPick, initialT }: { trace: Trace; traceId: string; onPick: (id: string) => void; initialT: number }) {
  const index = useMemo(() => indexTrace(trace), [trace]);
  const player = usePlayer(trace, initialT);
  usePlayerKeys(player);
  const [theme, cycleTheme] = useTheme();
  const t = player.t;
  const step = trace.steps[t];

  useEffect(() => writeHash({ trace: traceId, t }), [traceId, t]);

  const marks: CodeMarks = useMemo(() => {
    const actor = step.actor;
    const actorThread = actor ? threadAt(step, actor.pid, actor.tid) : undefined;
    const cursors = step.processes.flatMap((p) =>
      p.state === 'reaped' || p.state === 'zombie'
        ? []
        : p.threads
            .filter((th) => th.state !== 'exited' && th.line !== null)
            .map((th) => ({
              line: th.line!,
              ink: threadInk(index, p.pid, th.tid),
              label: `PID ${p.pid} · hilo ${threadLabel(index, th.tid)}`,
              running: actor?.tid === th.tid,
            })),
    );
    return {
      executed: step.executed?.line ?? null,
      next: actorThread?.line ?? null,
      hover: player.hoverLine,
      cursors,
    };
  }, [step, index, player.hoverLine]);

  const narration = describeStep(trace, index, t);
  const ThemeIcon = theme === 'dark' ? Moon : theme === 'light' ? Sun : Contrast;
  const entry = catalog.find((c) => c.id === traceId);

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <Logo />
          <div>
            <h1>Visualizador de procesos en C</h1>
            <p>fork, pipes, señales e hilos, paso a paso</p>
          </div>
        </div>
        <label className="trace-pick">
          <span>Ejemplo</span>
          <select value={traceId} onChange={(e) => onPick(e.target.value)}>
            {catalog.map((c) => (
              <option key={c.id} value={c.id}>
                {c.title}
              </option>
            ))}
          </select>
        </label>
        <p className="trace-summary">{entry?.summary}</p>
        <div className="topbar-actions">
          <button type="button" className="ghost" disabled title="La introducción animada llega en la fase 6">
            <GraduationCap size={16} /> Introducción
          </button>
          <button
            type="button"
            className="ghost icon"
            onClick={cycleTheme}
            aria-label={`Tema: ${theme === 'system' ? 'según el sistema' : theme === 'dark' ? 'oscuro' : 'claro'}. Cambiar tema`}
            title="Cambiar tema (sistema, claro, oscuro)"
          >
            <ThemeIcon size={18} />
          </button>
        </div>
      </header>

      <main className="main">
        <aside className="side">
          <section className="panel code-panel" aria-label="Código">
            <div className="panel-head">
              <h2>Código</h2>
              <span className="badge" title="En la fase 1 podrás escribir tu propio programa">
                solo lectura
              </span>
            </div>
            <CodePanel source={trace.source} marks={marks} onLineClick={player.toLine} />
            <div className="legend">
              <span className="legend-item">
                <i className="sw executed" /> recién ejecutada
              </span>
              <span className="legend-item">
                <i className="sw next" /> próxima
              </span>
              <span className="legend-item">clic en el margen: avanzar hasta esa línea</span>
            </div>
          </section>
          <section className="panel narration" aria-live="polite" aria-label="Qué pasó en este paso">
            {narration.map((n, i) => (
              <p key={i}>{n}</p>
            ))}
          </section>
          <TerminalPanel trace={trace} index={index} t={t} />
          <StdinPanel trace={trace} t={t} />
        </aside>
        <section className="stage" aria-label="Procesos, hilos, pipes y señales">
          <Canvas trace={trace} index={index} player={player} />
        </section>
      </main>

      <footer className="bottombar">
        <Controls player={player} index={index} />
        <Timeline trace={trace} index={index} player={player} />
      </footer>
    </div>
  );
}

function Logo() {
  return (
    <svg width="38" height="38" viewBox="0 0 38 38" aria-hidden="true" className="logo">
      <rect x="2" y="3" width="15" height="13" rx="4" className="logo-a" />
      <rect x="21" y="22" width="15" height="13" rx="4" className="logo-b" />
      <path d="M13 16 L25 22" className="logo-pipe" />
      <circle cx="19" cy="19" r="2.4" className="logo-dot" />
    </svg>
  );
}
