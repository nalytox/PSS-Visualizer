// Galería de trazas disponibles sin servidor. En la fase 0 son las sintéticas.
import type { Trace } from './types.ts';

export interface CatalogEntry {
  id: string;
  title: string;
  summary: string;
  load: () => Promise<Trace>;
}

const files = import.meta.glob<Trace>('@traces/synthetic/*.json', { import: 'default' });

const META: Record<string, { title: string; summary: string }> = {
  fork_pipe: { title: 'fork + pipe', summary: 'El hijo escribe "hola" en un pipe y el padre lo lee hasta EOF.' },
  threads_mutex: { title: 'Hilos con mutex', summary: 'Dos hilos incrementan un contador protegido por un mutex.' },
  signal_handler: { title: 'Señal con handler', summary: 'El padre envía SIGUSR1 y el hijo la atiende en su handler.' },
  structs_heap: { title: 'Structs y heap', summary: 'Structs anidados, una lista enlazada en el heap y un puntero colgante.' },
  fork_tree: { title: 'fork en un bucle (16 procesos)', summary: 'Cuatro vueltas de fork() producen 16 procesos. Sirve también como prueba de carga.' },
};

export const catalog: CatalogEntry[] = Object.entries(files)
  .map(([path, load]) => {
    const id = path.split('/').pop()!.replace(/\.json$/, '');
    const meta = META[id] ?? { title: id, summary: '' };
    return { id, ...meta, load };
  })
  .sort((a, b) => Object.keys(META).indexOf(a.id) - Object.keys(META).indexOf(b.id));
