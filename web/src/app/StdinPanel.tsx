// Entrada estándar precargada. Al editar se escribe; al visualizar se ve qué parte ya se consumió.
// Si el programa pide más de lo que hay (y la entrada no termina con EOF), aquí se agrega.
import { useState } from 'react';
import { Keyboard, Send } from 'lucide-react';
import { visible } from '../trace/bytes.ts';
import type { Trace } from '../trace/types.ts';

interface Props {
  mode: 'edit' | 'view';
  doc: { stdin: string; stdinEof: boolean };
  trace?: Trace;
  t: number;
  atEnd: boolean;
  busy: boolean;
  onEdit: (patch: { stdin?: string; stdinEof?: boolean }) => void;
  onMoreInput: (text: string) => void;
  onEof: () => void;
}

export function StdinPanel(props: Props) {
  const [more, setMore] = useState('');
  const { trace, t } = props;

  if (props.mode === 'edit' || !trace) {
    return (
      <section className="panel stdin" aria-label="Entrada estándar">
        <div className="panel-head">
          <h2>Entrada (stdin)</h2>
        </div>
        <textarea
          className="stdin-edit"
          value={props.doc.stdin}
          onChange={(e) => props.onEdit({ stdin: e.target.value })}
          placeholder="Lo que el programa leerá con scanf, fgets o read"
          rows={3}
          spellCheck={false}
          aria-label="Contenido de la entrada estándar"
        />
        <label className="check">
          <input type="checkbox" checked={props.doc.stdinEof} onChange={(e) => props.onEdit({ stdinEof: e.target.checked })} />
          <span>Terminar con EOF (como <code>./prog &lt; archivo</code>). Si no, el programa te pedirá más cuando se acabe.</span>
        </label>
      </section>
    );
  }

  const state = trace.steps[t].stdin;
  const consumed = trace.stdin.slice(0, state.consumed);
  const rest = trace.stdin.slice(state.consumed);
  const waiting = trace.outcome.kind === 'awaitingInput' && props.atEnd;
  const send = () => {
    props.onMoreInput(more);
    setMore('');
  };

  return (
    <section className={`panel stdin${waiting ? ' waiting' : ''}`} aria-label="Entrada estándar">
      <div className="panel-head">
        <h2>Entrada (stdin)</h2>
        <span className="hint">{state.eof ? 'termina con EOF' : 'queda abierta: se pedirá más'}</span>
      </div>
      {trace.stdin.length === 0 && !waiting ? (
        <div className="stdin-empty">El programa no ha leído entrada.</div>
      ) : (
        trace.stdin.length > 0 && (
          <div className="stdin-body" title="Tachado: lo que el programa ya leyó">
            <span className="stdin-consumed">{visible(consumed)}</span>
            <span className="stdin-rest">{visible(rest)}</span>
          </div>
        )
      )}
      {waiting && (
        <form
          className="stdin-more"
          onSubmit={(e) => {
            e.preventDefault();
            send();
          }}
        >
          <p>
            <Keyboard size={16} /> El programa está esperando que escribas algo.
          </p>
          <div className="stdin-more-row">
            <input value={more} onChange={(e) => setMore(e.target.value)} placeholder="Escribe una línea y presiona Enter" aria-label="Más entrada" autoFocus spellCheck={false} />
            <button type="submit" className="run" disabled={props.busy}>
              <Send size={14} /> Enviar
            </button>
          </div>
          <button type="button" className="ghost small" onClick={props.onEof} disabled={props.busy} title="Cierra la entrada: read devolverá 0">
            Enviar EOF (Ctrl + D)
          </button>
        </form>
      )}
    </section>
  );
}
