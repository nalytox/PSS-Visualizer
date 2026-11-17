// Markdown mínimo de las lecciones: párrafos, listas con "- ", bloques ``` y, dentro del texto,
// `código`, **negrita** y *cursiva*. Se arman elementos de React: nunca se inserta HTML.
import type { ReactNode } from 'react';

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*)/g;

function inline(text: string): ReactNode[] {
  return text.split(INLINE).map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`') && part.length > 1) return <code key={i}>{part.slice(1, -1)}</code>;
    // Dentro de la negrita y la cursiva puede haber `código`.
    if (part.startsWith('**') && part.endsWith('**') && part.length > 3) return <strong key={i}>{inline(part.slice(2, -2))}</strong>;
    if (part.startsWith('*') && part.endsWith('*') && part.length > 2) return <em key={i}>{inline(part.slice(1, -1))}</em>;
    return part;
  });
}

export function Markdown({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; ) {
    const line = lines[i];
    if (!line.trim()) {
      i++;
      continue;
    }
    if (line.startsWith('```')) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith('```')) code.push(lines[i++]);
      i++;
      blocks.push(
        <pre key={blocks.length} className="md-code">
          <code>{code.join('\n')}</code>
        </pre>,
      );
      continue;
    }
    if (/^\s*- /.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*- /.test(lines[i])) {
        let item = lines[i++].replace(/^\s*- /, '');
        // Una línea con sangría continúa el ítem anterior.
        while (i < lines.length && /^\s{2,}\S/.test(lines[i]) && !/^\s*- /.test(lines[i])) item += ' ' + lines[i++].trim();
        items.push(item);
      }
      blocks.push(
        <ul key={blocks.length}>
          {items.map((it, k) => (
            <li key={k}>{inline(it)}</li>
          ))}
        </ul>,
      );
      continue;
    }
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !lines[i].startsWith('```') && !/^\s*- /.test(lines[i])) para.push(lines[i++].trim());
    blocks.push(<p key={blocks.length}>{inline(para.join(' '))}</p>);
  }
  return <>{blocks}</>;
}
