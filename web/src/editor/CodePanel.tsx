// Panel de código fijo a la izquierda (como en Python Tutor), con un cursor de color por hilo.
// En la fase 0 es de solo lectura: muestra el programa de la traza.
import { cpp } from '@codemirror/lang-cpp';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { EditorState, RangeSet, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, gutter, lineNumbers, type DecorationSet } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { useEffect, useRef } from 'react';

export interface ThreadCursor {
  line: number;
  ink: string;
  label: string;
  running: boolean;
}

export interface CodeMarks {
  executed: number | null;
  next: number | null;
  hover: number | null;
  cursors: ThreadCursor[];
}

const setMarks = StateEffect.define<CodeMarks>();
const EMPTY: CodeMarks = { executed: null, next: null, hover: null, cursors: [] };

const marksField = StateField.define<CodeMarks>({
  create: () => EMPTY,
  update: (value, tr) => {
    for (const e of tr.effects) if (e.is(setMarks)) return e.value;
    return value;
  },
});

const lineDecorations = EditorView.decorations.compute([marksField], (state): DecorationSet => {
  const m = state.field(marksField);
  const ranges: ReturnType<ReturnType<typeof Decoration.line>["range"]>[] = [];
  const add = (line: number | null, cls: string) => {
    if (line === null || line < 1 || line > state.doc.lines) return;
    ranges.push(Decoration.line({ class: cls }).range(state.doc.line(line).from));
  };
  add(m.executed, 'cm-executed');
  if (m.next !== m.executed) add(m.next, 'cm-next');
  else add(m.next, 'cm-next cm-same');
  if (m.hover !== null && m.hover !== m.next && m.hover !== m.executed) add(m.hover, 'cm-hover');
  ranges.sort((a, b) => a.from - b.from);
  return Decoration.set(ranges, true);
});

class CursorMarker extends GutterMarker {
  constructor(readonly cursors: ThreadCursor[]) {
    super();
  }
  eq(other: CursorMarker) {
    return JSON.stringify(other.cursors) === JSON.stringify(this.cursors);
  }
  toDOM() {
    const wrap = document.createElement('span');
    wrap.className = 'pss-thread-cursors';
    for (const c of this.cursors.slice(0, 4)) {
      const dot = document.createElement('span');
      dot.className = `pss-thread-cursor${c.running ? ' running' : ''}`;
      dot.style.setProperty('--ink', c.ink);
      dot.title = c.label;
      wrap.appendChild(dot);
    }
    return wrap;
  }
}

function cursorGutter(onLineClick: (line: number) => void): Extension {
  return gutter({
    class: 'cm-thread-gutter',
    markers: (view) => {
      const m = view.state.field(marksField);
      const byLine = new Map<number, ThreadCursor[]>();
      for (const c of m.cursors) {
        if (c.line < 1 || c.line > view.state.doc.lines) continue;
        byLine.set(c.line, [...(byLine.get(c.line) ?? []), c]);
      }
      const ranges = [...byLine.entries()]
        .sort((a, b) => a[0] - b[0])
        .map(([line, cs]) => new CursorMarker(cs).range(view.state.doc.line(line).from));
      return RangeSet.of(ranges);
    },
    initialSpacer: () => new CursorMarker([]),
    domEventHandlers: {
      mousedown: (view, block) => {
        onLineClick(view.state.doc.lineAt(block.from).number);
        return true;
      },
    },
  });
}

const highlight = HighlightStyle.define([
  { tag: [tags.keyword, tags.controlKeyword, tags.modifier], color: 'var(--code-keyword)', fontWeight: '600' },
  { tag: [tags.typeName, tags.standard(tags.typeName)], color: 'var(--code-type)' },
  { tag: [tags.string, tags.character], color: 'var(--code-string)' },
  { tag: [tags.number, tags.bool, tags.null], color: 'var(--code-number)' },
  { tag: [tags.comment, tags.lineComment, tags.blockComment], color: 'var(--text-2)', fontStyle: 'italic' },
  { tag: [tags.processingInstruction, tags.macroName], color: 'var(--code-macro)' },
  { tag: tags.function(tags.variableName), color: 'var(--code-fn)' },
]);

const theme = EditorView.theme({
  '&': { height: '100%', fontSize: '13px', backgroundColor: 'transparent', color: 'var(--text)' },
  '.cm-scroller': { fontFamily: 'var(--font-code)', lineHeight: '1.65' },
  '.cm-gutters': { backgroundColor: 'transparent', border: 'none', color: 'var(--text-2)' },
  '.cm-content': { caretColor: 'transparent' },
});

export function CodePanel(props: { source: string; marks: CodeMarks; onLineClick: (line: number) => void }) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const click = useRef(props.onLineClick);
  click.current = props.onLineClick;

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.source,
        extensions: [
          marksField,
          cursorGutter((line) => click.current(line)),
          lineNumbers({ domEventHandlers: { mousedown: (vw, block) => (click.current(vw.state.doc.lineAt(block.from).number), true) } }),
          cpp(),
          syntaxHighlighting(highlight),
          lineDecorations,
          EditorState.readOnly.of(true),
          EditorView.editable.of(false),
          EditorView.contentAttributes.of({ 'aria-label': 'Código fuente del programa (solo lectura)' }),
          theme,
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
    // El editor se crea una sola vez; el documento y las marcas se actualizan abajo.
  }, []);

  useEffect(() => {
    const v = view.current;
    if (!v || v.state.doc.toString() === props.source) return;
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: props.source } });
  }, [props.source]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const effects: StateEffect<unknown>[] = [setMarks.of(props.marks)];
    const focus = props.marks.hover ?? props.marks.next;
    if (focus !== null && focus >= 1 && focus <= v.state.doc.lines) {
      effects.push(EditorView.scrollIntoView(v.state.doc.line(focus).from, { y: 'nearest', yMargin: 40 }));
    }
    v.dispatch({ effects });
  }, [props.marks]);

  return <div ref={host} className="code-host" />;
}
