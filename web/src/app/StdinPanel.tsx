// Entrada estándar precargada. En la fase 1 se podrá editar y pedir más cuando se agote.
import { decodeBytes, visible } from '../trace/bytes.ts';
import type { Trace } from '../trace/types.ts';

export function StdinPanel({ trace, t }: { trace: Trace; t: number }) {
  const state = trace.steps[t].stdin;
  const consumed = trace.stdin.slice(0, state.consumed);
  const rest = trace.stdin.slice(state.consumed);
  return (
    <section className="panel stdin" aria-label="Entrada estándar">
      <div className="panel-head">
        <h2>Entrada (stdin)</h2>
        <span className="hint">{state.eof ? 'termina con EOF' : 'queda abierta: se pedirá más'}</span>
      </div>
      {trace.stdin.length === 0 ? (
        <div className="stdin-empty">Este programa no lee de la entrada estándar.</div>
      ) : (
        <div className="stdin-body" title={decodeBytes(trace.stdin)}>
          <span className="stdin-consumed">{visible(consumed)}</span>
          <span className="stdin-rest">{visible(rest)}</span>
        </div>
      )}
    </section>
  );
}
