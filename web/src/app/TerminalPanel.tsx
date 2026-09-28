// Consola global: todo lo que llegó a la terminal hasta el paso actual, con el color de cada proceso.
import { decodeBytes } from '../trace/bytes.ts';
import { outputUpTo, type TraceIndex } from '../trace/query.ts';
import type { Trace } from '../trace/types.ts';
import { inkOf } from '../player/SceneContext.tsx';

interface Line {
  pid: number;
  text: string;
  stderr: boolean;
  t: number;
}

export function terminalLines(trace: Trace, t: number): Line[] {
  const lines: Line[] = [];
  const open = new Map<number, Line>();
  for (const c of outputUpTo(trace, t)) {
    for (const ch of decodeBytes(c.bytes)) {
      let line = open.get(c.pid);
      if (!line) {
        line = { pid: c.pid, text: '', stderr: c.stream === 'stderr', t: c.t };
        open.set(c.pid, line);
        lines.push(line);
      }
      if (ch === '\n') open.delete(c.pid);
      else line.text += ch;
    }
  }
  return lines;
}

export function TerminalPanel({ trace, index, t }: { trace: Trace; index: TraceIndex; t: number }) {
  const lines = terminalLines(trace, t);
  return (
    <section className="panel terminal" aria-label="Terminal">
      <div className="panel-head">
        <h2>Terminal</h2>
        <span className="hint">lo que ve el usuario, en orden</span>
      </div>
      <div className="terminal-body" role="log">
        {lines.length === 0 && <div className="terminal-empty">Todavía no se imprimió nada.</div>}
        {lines.map((l, i) => (
          <div key={i} className={`terminal-line${l.stderr ? ' stderr' : ''}${l.t === t ? ' fresh' : ''}`}>
            <span className="terminal-pid" style={{ borderColor: inkOf(index, l.pid) }}>
              {l.pid}
            </span>
            <span className="terminal-text">{l.text}</span>
          </div>
        ))}
      </div>
    </section>
  );
}
