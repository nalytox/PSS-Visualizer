// Programas de ejemplo (examples/*.c), con su entrada precargada si tienen un .stdin al lado.
const sources = import.meta.glob<string>('@examples/*.c', { query: '?raw', import: 'default', eager: true });
const stdins = import.meta.glob<string>('@examples/*.stdin', { query: '?raw', import: 'default', eager: true });

export interface Example {
  id: string;
  title: string;
  source: string;
  stdin: string;
}

const TITLES: Record<string, string> = {
  '01_structs': '01 · Structs y punteros',
  '02_lista_enlazada': '02 · Lista enlazada con malloc',
  entrada_estandar: 'Leer números de stdin',
};

const base = (path: string) => path.split('/').pop()!.replace(/\.(c|stdin)$/, '');

export const examples: Example[] = Object.entries(sources)
  .map(([path, source]) => {
    const id = base(path);
    const stdinPath = Object.keys(stdins).find((p) => base(p) === id);
    return { id, title: TITLES[id] ?? id, source, stdin: stdinPath ? stdins[stdinPath] : '' };
  })
  .sort((a, b) => a.id.localeCompare(b.id));

export const blankProgram = `#include <stdio.h>

int main(void) {
    printf("hola\\n");
    return 0;
}
`;
