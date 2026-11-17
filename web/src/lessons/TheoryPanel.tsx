// Panel de teoría: en cada paso, la explicación de la línea que se acaba de ejecutar (si el ejemplo
// tiene lección) y las fichas de los conceptos que aparecen en sus eventos. Al inicio va la
// introducción de la lección y en el último paso, antes de las fichas, lo que hay que llevarse.
import { BookOpen, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import type { Step } from '../trace/types.ts';
import { concepts, conceptsOf } from './index.ts';
import { noteFor, type Concept, type Lesson } from './lesson.ts';
import { Markdown } from './Markdown.tsx';

export function ConceptCard({ concept }: { concept: Concept }) {
  return (
    <details className="concept" open>
      <summary>{concept.title}</summary>
      <Markdown text={concept.body} />
    </details>
  );
}

export function TheoryPanel(props: {
  lesson?: Lesson;
  step?: Step;
  t: number;
  last: number;
  onClose: () => void;
  onOpenLesson: () => void;
}) {
  const { lesson, step, t, last } = props;
  const body = useRef<HTMLDivElement>(null);
  const atStart = !step || t === 0;
  const atEnd = !!step && t === last;
  const note = lesson && step ? noteFor(lesson, step.executed?.line) : undefined;
  const cards = (step ? conceptsOf(step) : []).flatMap((id) => concepts.get(id) ?? []);
  const intro = !lesson && atStart ? concepts.get('proceso') : undefined;

  // Cada paso empieza a leerse desde arriba.
  useEffect(() => {
    body.current?.scrollTo?.({ top: 0 });
  }, [t]);

  return (
    <aside className="theory" aria-label="Teoría">
      <div className="theory-head">
        <h2>
          <BookOpen size={15} /> Teoría
        </h2>
        {lesson && (
          <button type="button" className="ghost small" onClick={props.onOpenLesson} title="Leer la lección entera de este ejemplo">
            Lección completa
          </button>
        )}
        <button type="button" className="ghost icon small" onClick={props.onClose} aria-label="Ocultar la teoría" title="Ocultar la teoría">
          <X size={15} />
        </button>
      </div>
      <div className="theory-body" ref={body}>
        {lesson && atStart && (
          <section className="theory-intro">
            <h3>{lesson.title}</h3>
            {lesson.goal && (
              <div className="theory-goal">
                <Markdown text={lesson.goal} />
              </div>
            )}
            <Markdown text={lesson.intro} />
          </section>
        )}
        {intro && <ConceptCard concept={intro} />}
        {note && (
          <section className="theory-line">
            <h3>
              Línea {note.line} <code>{note.code}</code>
            </h3>
            <Markdown text={note.body} />
          </section>
        )}
        {lesson && atEnd && (
          <section className="theory-end">
            {lesson.wrapUp && (
              <>
                <h3>Al terminar</h3>
                <Markdown text={lesson.wrapUp} />
              </>
            )}
            {lesson.practice && (
              <>
                <h3>Para practicar</h3>
                <Markdown text={lesson.practice} />
              </>
            )}
          </section>
        )}
        {cards.map((c) => (
          <ConceptCard key={c.id} concept={c} />
        ))}
        {!atStart && !atEnd && !note && cards.length === 0 && (
          <p className="theory-empty">
            {lesson
              ? 'Esta línea no tiene una nota propia: sigue a la siguiente o abre la lección completa.'
              : 'En este paso solo se ejecutó una línea de C, sin llamadas al sistema que explicar.'}
          </p>
        )}
      </div>
    </aside>
  );
}
