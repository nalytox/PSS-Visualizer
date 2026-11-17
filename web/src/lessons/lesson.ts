// Lecciones de los programas de ejemplo: examples/<id>.md, junto a su .c. Formato:
//
//   # Título
//   > **Objetivo:** …
//   (teoría de introducción)
//   ## Línea 7 · `pid_t pid = fork();`      ← nota de esa línea; el código debe estar en ella.
//                                             Las notas van en el orden en que se ejecutan.
//   ## Al terminar                           ← lo que hay que llevarse
//   ## Para practicar                        ← ejercicios
//
// Las fichas de conceptos (lessons/conceptos.md) usan `## id · Título`.

export interface LineNote {
  line: number;
  code: string;
  body: string;
}

export interface Lesson {
  id: string;
  title: string;
  goal: string;
  intro: string;
  lines: LineNote[];
  wrapUp: string;
  practice: string;
}

export interface Concept {
  id: string;
  title: string;
  body: string;
}

const LINE = /^## Línea (\d+) · `(.+)`\s*$/;

export function parseLesson(id: string, text: string): Lesson {
  const lesson: Lesson = { id, title: id, goal: '', intro: '', lines: [], wrapUp: '', practice: '' };
  const sections = text.replace(/\r\n/g, '\n').split(/^(?=## )/m);
  const head = sections.shift() ?? '';
  const headLines = head.split('\n');
  const titleAt = headLines.findIndex((l) => l.startsWith('# '));
  if (titleAt >= 0) lesson.title = headLines[titleAt].slice(2).trim();
  const rest = headLines.slice(titleAt + 1);
  const goal: string[] = [];
  while (rest.length && (rest[0].startsWith('>') || (!rest[0].trim() && goal.length === 0))) {
    const l = rest.shift()!;
    if (l.startsWith('>')) goal.push(l.replace(/^>\s?/, ''));
  }
  lesson.goal = goal.join('\n').trim();
  lesson.intro = rest.join('\n').trim();
  for (const section of sections) {
    const [first, ...body] = section.split('\n');
    const text = body.join('\n').trim();
    const m = LINE.exec(first);
    if (m) lesson.lines.push({ line: Number(m[1]), code: m[2], body: text });
    else if (/^## Al terminar/.test(first)) lesson.wrapUp = text;
    else if (/^## Para practicar/.test(first)) lesson.practice = text;
    else throw new Error(`${id}: sección desconocida «${first}»`);
  }
  // Las notas quedan en el orden del archivo: el de la ejecución, no el de los números de línea.
  return lesson;
}

export function parseConcepts(text: string): Map<string, Concept> {
  const out = new Map<string, Concept>();
  for (const section of text.replace(/\r\n/g, '\n').split(/^(?=## )/m)) {
    const m = /^## ([a-z0-9-]+) · (.+)$/m.exec(section.split('\n')[0]);
    if (!m) continue;
    out.set(m[1], { id: m[1], title: m[2].trim(), body: section.split('\n').slice(1).join('\n').trim() });
  }
  return out;
}

// La nota de una línea, si la lección la explica.
export function noteFor(lesson: Lesson, line: number | null | undefined): LineNote | undefined {
  return line ? lesson.lines.find((n) => n.line === line) : undefined;
}
