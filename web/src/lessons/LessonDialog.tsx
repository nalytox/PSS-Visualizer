// La lección completa de un ejemplo, para leerla de corrido. Cada nota de línea puede llevar al
// paso que ejecuta esa línea, donde el panel Teoría la muestra.
import { X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import type { Lesson } from './lesson.ts';
import { Markdown } from './Markdown.tsx';

export function LessonDialog({ lesson, onClose, onGoToLine }: { lesson: Lesson; onClose: () => void; onGoToLine?: (line: number) => void }) {
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    dialog.current?.focus();
  }, []);
  return (
    <div className="lesson-backdrop" onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div
        className="lesson-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="lesson-title"
        tabIndex={-1}
        ref={dialog}
        onKeyDown={(e) => {
          // La lección tapa al reproductor: sus atajos no deben llegar al de atrás.
          e.stopPropagation();
          if (e.key === 'Escape') onClose();
        }}
      >
        <header className="lesson-head">
          <h2 id="lesson-title">{lesson.title}</h2>
          <button type="button" className="ghost icon" onClick={onClose} aria-label="Cerrar la lección" title="Cerrar (Esc)">
            <X size={18} />
          </button>
        </header>
        <div className="lesson-body">
          {lesson.goal && (
            <div className="theory-goal">
              <Markdown text={lesson.goal} />
            </div>
          )}
          <Markdown text={lesson.intro} />
          <h3>Paso a paso</h3>
          {lesson.lines.map((n) => (
            <section key={n.line} className="lesson-line">
              <h4>
                Línea {n.line} <code>{n.code}</code>
                {onGoToLine && (
                  <button
                    type="button"
                    className="ghost small"
                    onClick={() => {
                      onGoToLine(n.line);
                      onClose();
                    }}
                    title="Avanzar la ejecución hasta el paso que ejecuta esta línea"
                  >
                    Ir a esta línea
                  </button>
                )}
              </h4>
              <Markdown text={n.body} />
            </section>
          ))}
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
        </div>
      </div>
    </div>
  );
}
