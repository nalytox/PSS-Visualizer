// Panel de código fijo a la izquierda (como en Python Tutor), con un cursor de color por hilo.
// En modo edición se escribe el programa; al visualizar queda de solo lectura con las marcas.
import { cpp } from '@codemirror/lang-cpp';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { Compartment, EditorState, RangeSet, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, GutterMarker, gutter, keymap, lineNumbers, type DecorationSet } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import type { Diagnostic } from '../trace/types.ts';
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
  diagnostics?: Diagnostic[];
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
  for (const d of m.diagnostics ?? []) {
    if (d.severity !== 'note' && !ranges.some((r) => r.from === state.doc.line(Math.min(Math.max(1, d.line), state.doc.lines)).from)) {
      add(d.line, d.severity === 'error' ? 'cm-diag-error' : 'cm-diag-warning');
    }
  }
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

export function CodePanel(props: {
  source: string;
  marks: CodeMarks;
  onLineClick: (line: number) => void;
  editable?: boolean;
  onChange?: (source: string) => void;
  onRun?: () => void;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const editableConf = useRef(new Compartment());
  const cb = useRef(props);
  cb.current = props;

  useEffect(() => {
    if (!host.current) return;
    const v = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: props.source,
        extensions: [
          marksField,
          cursorGutter((line) => !cb.current.editable && cb.current.onLineClick(line)),
          lineNumbers({
            domEventHandlers: {
              mousedown: (vw, block) => {
                if (!cb.current.editable) cb.current.onLineClick(vw.state.doc.lineAt(block.from).number);
                return !cb.current.editable;
              },
            },
          }),
          history(),
          keymap.of([
            { key: 'Mod-Enter', run: () => (cb.current.onRun?.(), true) },
            indentWithTab,
            ...defaultKeymap,
            ...historyKeymap,
          ]),
          cpp(),
          syntaxHighlighting(highlight),
          lineDecorations,
          editableConf.current.of(editableExt(!!props.editable)),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) cb.current.onChange?.(u.state.doc.toString());
          }),
          EditorView.contentAttributes.of({ 'aria-label': 'Código fuente del programa' }),
          theme,
        ],
      }),
    });
    view.current = v;
    return () => v.destroy();
    // El editor se crea una sola vez; documento, modo y marcas se actualizan abajo.
  }, []);

  useEffect(() => {
    view.current?.dispatch({ effects: editableConf.current.reconfigure(editableExt(!!props.editable)) });
  }, [props.editable]);

  useEffect(() => {
    const v = view.current;
    if (!v || v.state.doc.toString() === props.source) return;
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: props.source } });
  }, [props.source]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const effects: StateEffect<unknown>[] = [setMarks.of(props.marks)];
    const focus = props.marks.hover ?? props.marks.next ?? props.marks.diagnostics?.find((d) => d.severity === 'error')?.line ?? null;
    if (focus !== null && focus >= 1 && focus <= v.state.doc.lines) {
      effects.push(EditorView.scrollIntoView(v.state.doc.line(focus).from, { y: 'nearest', yMargin: 40 }));
    }
    v.dispatch({ effects });
  }, [props.marks]);

  return <div ref={host} className={`code-host${props.editable ? ' editing' : ''}`} />;
}

function editableExt(editable: boolean): Extension {
  // Sin edición el contenido no recibe foco: se hace enfocable para poder desplazarlo con el teclado.
  return [
    EditorState.readOnly.of(!editable),
    EditorView.editable.of(editable),
    ...(editable ? [] : [EditorView.contentAttributes.of({ tabindex: '0' })]),
  ];
}
