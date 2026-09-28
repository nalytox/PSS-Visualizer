import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Contrast, GraduationCap, Moon, Pencil, Play, Sun, LoaderCircle, TriangleAlert, CircleX } from 'lucide-react';
import { CodePanel, type CodeMarks } from '../editor/CodePanel.tsx';
import { Canvas } from '../player/Canvas.tsx';
import { Controls } from '../player/Controls.tsx';
import { Timeline } from '../player/Timeline.tsx';
import { usePlayer, usePlayerKeys } from '../player/usePlayer.ts';
import { threadInk } from '../player/SceneContext.tsx';
import { catalog } from '../trace/catalog.ts';
import { describeStep } from '../trace/describe.ts';
import { indexTrace, threadAt, threadLabel } from '../trace/query.ts';
import type { Diagnostic, Trace } from '../trace/types.ts';
import { runProgram, serverAvailable } from './api.ts';
import { blankProgram, examples } from './examples.ts';
import { StdinPanel } from './StdinPanel.tsx';
import { TerminalPanel } from './TerminalPanel.tsx';
import { readHash, writeHash, type UrlState } from './urlState.ts';
import { useTheme } from './useTheme.ts';

interface Doc {
  source: string;
  stdin: string;
  stdinEof: boolean;
  ctrlc?: number[]; // Ctrl+C presionado después de estos pasos
  example?: string; // id del ejemplo si no se ha modificado
}

interface Loaded {
  trace: Trace;
  key: number;
  initialT: number;
  recorded?: string; // id de una traza grabada de la galería
}

type Server = 'checking' | 'ready' | 'offline';

export function App() {
  const [boot, setBoot] = useState<UrlState | null>(null);
  useEffect(() => {
    readHash().then(setBoot);
  }, []);
  if (!boot) return <div className="loading">Cargando…</div>;
  return <Workspace initial={boot} />;
}

function docFromUrl(u: UrlState): Doc {
  if (u.code !== undefined) return { source: u.code, stdin: u.stdin ?? '', stdinEof: !!u.eof, ctrlc: u.ctrlc };
  const ex = examples.find((e) => e.id === u.example) ?? examples[0];
  if (!ex) return { source: blankProgram, stdin: '', stdinEof: true };
  return { source: ex.source, stdin: ex.stdin, stdinEof: false, example: ex.id, ctrlc: u.ctrlc };
}

function Workspace({ initial }: { initial: UrlState }) {
  const [server, setServer] = useState<Server>('checking');
  const [doc, setDoc] = useState<Doc>(() => docFromUrl(initial));
  const [mode, setMode] = useState<'edit' | 'view'>('edit');
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [running, setRunning] = useState(false);
  const [runError, setRunError] = useState<string | null>(null);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const keyRef = useRef(0);
  const started = useRef(false);

  const show = useCallback((trace: Trace, initialT: number, recorded?: string) => {
    keyRef.current += 1;
    setLoaded({ trace, key: keyRef.current, initialT: Math.min(initialT, Math.max(0, trace.steps.length - 1)), recorded });
    setDiagnostics(trace.compile.diagnostics);
    setMode('view');
  }, []);

  const loadRecorded = useCallback(
    async (id: string, initialT = 0) => {
      const entry = catalog.find((c) => c.id === id);
      if (!entry) return false;
      const trace = await entry.load();
      setDoc({ source: trace.source, stdin: trace.stdin, stdinEof: trace.run.stdinEof, example: examples.some((e) => e.id === id) ? id : undefined });
      show(trace, initialT, id);
      return true;
    },
    [show],
  );

  const run = useCallback(
    async (d: Doc, initialT = 0) => {
      setRunError(null);
      if (server !== 'ready') {
        // Sin servidor, un ejemplo sin cambios se puede ver con su traza grabada.
        if (d.example && (await loadRecorded(d.example, initialT))) return;
        setRunError('Para ejecutar programas inicia el servidor local con ./pss o docker compose up.');
        return;
      }
      setRunning(true);
      try {
        const injections = (d.ctrlc ?? []).map((t) => ({ t, signal: 'SIGINT' }));
        const trace = await runProgram({ source: d.source, stdin: d.stdin, stdinEof: d.stdinEof, injections });
        if (trace.outcome.kind === 'compileError' || trace.steps.length === 0) {
          setDiagnostics(trace.compile.diagnostics);
          setMode('edit');
        } else {
          show(trace, initialT);
        }
      } catch (e) {
        setRunError(`No se pudo ejecutar: ${(e as Error).message}`);
      } finally {
        setRunning(false);
      }
    },
    [server, loadRecorded, show],
  );

  useEffect(() => {
    serverAvailable().then((ok) => setServer(ok ? 'ready' : 'offline'));
  }, []);

  // Al abrir: una traza grabada se muestra directo; un programa se ejecuta en cuanto se sabe si hay servidor.
  useEffect(() => {
    if (started.current || server === 'checking') return;
    started.current = true;
    if (initial.trace) loadRecorded(initial.trace, initial.t);
    else run(doc, initial.t);
  }, [server, initial, doc, run, loadRecorded]);

  const pick = (value: string) => {
    const [kind, id] = value.split(':');
    if (kind === 'tr') {
      loadRecorded(id);
      return;
    }
    const ex = examples.find((e) => e.id === id);
    const d: Doc = ex ? { source: ex.source, stdin: ex.stdin, stdinEof: false, example: ex.id } : { source: blankProgram, stdin: '', stdinEof: false };
    setDoc(d);
    setDiagnostics([]);
    run(d);
  };

  const edit = (patch: Partial<Doc>) => setDoc((d) => ({ ...d, ...patch, example: undefined, ctrlc: undefined }));
  const pickValue = loaded?.recorded && !examples.some((e) => e.id === loaded.recorded) ? `tr:${loaded.recorded}` : doc.example ? `ej:${doc.example}` : 'ej:';
  const stale = !!loaded && !loaded.recorded && loaded.trace.source !== doc.source;

  const shared = {
    doc,
    mode,
    server,
    running,
    runError,
    diagnostics,
    stale,
    pickValue,
    onPick: pick,
    onEdit: edit,
    onRun: () => run(doc),
    onEditMode: () => setMode('edit'),
    onMoreInput: (text: string, t: number) => {
      const d = { ...doc, stdin: doc.stdin + text + '\n' };
      setDoc(d);
      run(d, t);
    },
    onEof: (t: number) => {
      const d = { ...doc, stdinEof: true };
      setDoc(d);
      run(d, t);
    },
    // Ctrl+C en el paso t: se vuelve a ejecutar con SIGINT después de t (lo anterior no cambia).
    onCtrlC:
      server === 'ready'
        ? (t: number) => {
            const d = { ...doc, ctrlc: [...(doc.ctrlc ?? []).filter((x) => x < t), t] };
            setDoc(d);
            run(d, t + 1);
          }
        : undefined,
  };
  if (loaded) return <WithTrace key={loaded.key} loaded={loaded} {...shared} />;
  return <WithoutTrace {...shared} />;
}

interface Shared {
  doc: Doc;
  mode: 'edit' | 'view';
  server: Server;
  running: boolean;
  runError: string | null;
  diagnostics: Diagnostic[];
  stale: boolean;
  pickValue: string;
  onPick: (value: string) => void;
  onEdit: (patch: Partial<Doc>) => void;
  onRun: () => void;
  onEditMode: () => void;
  onMoreInput: (text: string, t: number) => void;
  onEof: (t: number) => void;
  onCtrlC?: (t: number) => void;
}

function WithTrace(props: Shared & { loaded: Loaded }) {
  const { loaded, doc, mode } = props;
  const trace = loaded.trace;
  const index = useMemo(() => indexTrace(trace), [trace]);
  const player = usePlayer(trace, loaded.initialT);
  usePlayerKeys(player);
  const t = player.t;
  const step = trace.steps[t];

  useEffect(() => {
    const ctrlc = trace.run.injections.map((i) => i.t);
    if (loaded.recorded) writeHash({ trace: loaded.recorded, t });
    else if (doc.example && doc.source === trace.source) writeHash({ example: doc.example, ctrlc, t });
    else writeHash({ code: trace.source, stdin: trace.stdin, eof: trace.run.stdinEof, ctrlc, t });
  }, [loaded.recorded, doc.example, doc.source, trace, t]);

  const marks: CodeMarks = useMemo(() => {
    if (mode === 'edit') return { executed: null, next: null, hover: null, cursors: [], diagnostics: props.diagnostics };
    const actor = step.actor;
    const actorThread = actor ? threadAt(step, actor.pid, actor.tid) : undefined;
    const cursors = step.processes.flatMap((p) =>
      p.state === 'reaped' || p.state === 'zombie'
        ? []
        : p.threads
            .filter((th) => th.state !== 'exited' && th.line !== null)
            .map((th) => ({ line: th.line!, ink: threadInk(index, p.pid, th.tid), label: `PID ${p.pid} · hilo ${threadLabel(index, th.tid)}`, running: actor?.tid === th.tid })),
    );
    return { executed: step.executed?.line ?? null, next: actorThread?.line ?? null, hover: player.hoverLine, cursors };
  }, [mode, step, index, player.hoverLine, props.diagnostics]);

  return (
    <Layout
      {...props}
      codeSource={mode === 'edit' ? doc.source : trace.source}
      marks={marks}
      onLineClick={player.toLine}
      side={
        <>
          {mode === 'view' && (
            <section className="panel narration" aria-live="polite" aria-label="Qué pasó en este paso">
              {describeStep(trace, index, t).map((n, i) => (
                <p key={i}>{n}</p>
              ))}
            </section>
          )}
          <TerminalPanel trace={trace} index={index} t={t} />
          <StdinPanel
            mode={mode}
            doc={props.doc}
            trace={trace}
            t={t}
            atEnd={t === player.last}
            onEdit={props.onEdit}
            onMoreInput={(text) => props.onMoreInput(text, player.last)}
            onEof={() => props.onEof(player.last)}
            busy={props.running}
          />
        </>
      }
      stage={
        <>
          <Canvas trace={trace} index={index} player={player} />
          {props.stale && mode === 'edit' && <div className="stage-note">Estás viendo la ejecución anterior. Presiona Ejecutar para ver tus cambios.</div>}
        </>
      }
      footer={
        <>
          <Controls player={player} index={index} onCtrlC={props.onCtrlC && !props.running ? () => props.onCtrlC!(player.t) : undefined} />
          <Timeline trace={trace} index={index} player={player} />
        </>
      }
    />
  );
}

function WithoutTrace(props: Shared) {
  const marks: CodeMarks = useMemo(() => ({ executed: null, next: null, hover: null, cursors: [], diagnostics: props.diagnostics }), [props.diagnostics]);
  return (
    <Layout
      {...props}
      mode="edit"
      codeSource={props.doc.source}
      marks={marks}
      onLineClick={() => {}}
      side={<StdinPanel mode="edit" doc={props.doc} t={0} atEnd={false} onEdit={props.onEdit} onMoreInput={() => {}} onEof={() => {}} busy={props.running} />}
      stage={
        <div className="stage-empty">
          {props.running ? (
            <p>
              <LoaderCircle className="spin" size={18} /> Compilando y ejecutando…
            </p>
          ) : props.server === 'offline' ? (
            <p>No hay servidor local: inicia la aplicación con ./pss o docker compose up para ejecutar programas. Mientras tanto puedes abrir las trazas grabadas de la galería.</p>
          ) : (
            <p>Escribe tu programa y presiona Ejecutar (Ctrl + Enter).</p>
          )}
        </div>
      }
      footer={null}
    />
  );
}

function Layout(
  props: Shared & {
    codeSource: string;
    marks: CodeMarks;
    onLineClick: (line: number) => void;
    side: React.ReactNode;
    stage: React.ReactNode;
    footer: React.ReactNode;
  },
) {
  const [theme, cycleTheme] = useTheme();
  const ThemeIcon = theme === 'dark' ? Moon : theme === 'light' ? Sun : Contrast;
  const editing = props.mode === 'edit';
  const errors = props.diagnostics.filter((d) => d.severity !== 'note');
  const recorded = catalog.filter((c) => !examples.some((e) => e.id === c.id));

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
          <select value={props.pickValue} onChange={(e) => props.onPick(e.target.value)}>
            <option value="ej:" disabled={props.pickValue !== 'ej:'}>
              Mi programa
            </option>
            <optgroup label="Programas">
              {examples.map((e) => (
                <option key={e.id} value={`ej:${e.id}`}>
                  {e.title}
                </option>
              ))}
            </optgroup>
            <optgroup label="Trazas grabadas">
              {recorded.map((c) => (
                <option key={c.id} value={`tr:${c.id}`}>
                  {c.title}
                </option>
              ))}
            </optgroup>
          </select>
        </label>
        <p className="trace-summary">
          {props.server === 'offline' && <span className="server-off">Sin servidor local: solo trazas grabadas</span>}
        </p>
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
          <section className={`panel code-panel${editing ? ' editing' : ''}`} aria-label="Código">
            <div className="panel-head">
              <h2>Código</h2>
              {!editing && <span className="badge">visualizando</span>}
              <div className="code-actions">
                {!editing && (
                  <button type="button" className="ghost small" onClick={props.onEditMode} title="Volver a editar el programa">
                    <Pencil size={14} /> Editar
                  </button>
                )}
                {editing && (
                  <button type="button" className="run" onClick={props.onRun} disabled={props.running} title="Compilar y ejecutar (Ctrl + Enter)">
                    {props.running ? <LoaderCircle className="spin" size={15} /> : <Play size={15} />} Ejecutar
                  </button>
                )}
              </div>
            </div>
            <CodePanel
              source={props.codeSource}
              marks={props.marks}
              onLineClick={props.onLineClick}
              editable={editing}
              onChange={(source) => props.onEdit({ source })}
              onRun={props.onRun}
            />
            {editing && errors.length > 0 && (
              <ul className="diagnostics" aria-label="Mensajes del compilador">
                {errors.map((d, i) => (
                  <li key={i} className={d.severity}>
                    {d.severity === 'error' ? <CircleX size={14} /> : <TriangleAlert size={14} />}
                    <span>
                      {d.line > 0 && <strong>línea {d.line}:</strong>} {d.message}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {props.runError && <p className="run-error">{props.runError}</p>}
            {!editing && (
              <div className="legend">
                <span className="legend-item">
                  <i className="sw executed" /> recién ejecutada
                </span>
                <span className="legend-item">
                  <i className="sw next" /> próxima
                </span>
                <span className="legend-item">clic en el margen: avanzar hasta esa línea</span>
              </div>
            )}
          </section>
          {props.side}
        </aside>
        <section className="stage" aria-label="Procesos, hilos, pipes y señales">
          {props.stage}
        </section>
      </main>

      {props.footer && <footer className="bottombar">{props.footer}</footer>}
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
