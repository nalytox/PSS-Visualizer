// Galería de trazas disponibles sin servidor. En la fase 0 son las sintéticas.
import type { Trace } from './types.ts';

export interface CatalogEntry {
  id: string;
  title: string;
  summary: string;
  load: () => Promise<Trace>;
}

const files = {
  ...import.meta.glob<Trace>('@traces/reference/*.json', { import: 'default' }),
  ...import.meta.glob<Trace>('@traces/synthetic/*.json', { import: 'default' }),
};

const META: Record<string, { title: string; summary: string }> = {
  '01_structs': { title: '01 · Structs y punteros', summary: 'Struct anidado, arreglo de structs y punteros a struct (traza real).' },
  '02_lista_enlazada': { title: '02 · Lista enlazada', summary: 'malloc, lista enlazada, free parcial y fugas al final (traza real).' },
  entrada_estandar: { title: 'Entrada estándar', summary: 'scanf lee números del stdin precargado (traza real).' },
  '03_fork_simple': { title: '03 · fork simple', summary: 'Un fork, printf en ambos procesos y wait: el zombie se recoge (traza real).' },
  '04_fork_bucle': { title: '04 · fork en un bucle', summary: 'Tres vueltas de fork() producen 8 procesos; los huérfanos pasan a init (traza real).' },
  '05_exec': { title: '05 · fork + exec', summary: 'El hijo ejecuta ls: conserva su PID y se vuelve una caja negra (traza real).' },
  '06_pipe_padre_hijo': { title: '06 · pipe padre e hijo', summary: 'El hijo escribe en un pipe y el padre lee hasta EOF (traza real).' },
  '07_pipe_sin_cerrar': { title: '07 · pipe sin cerrar', summary: 'El lector olvida cerrar fd[1]: nunca ve EOF y queda bloqueado para siempre (traza real).' },
  '08_pipeline': { title: '08 · ls | grep c | wc -l', summary: 'Tres procesos encadenados con pipes y dup2 (traza real).' },
  '09_sigusr1': { title: '09 · SIGUSR1 entre procesos', summary: 'El padre envía SIGUSR1 con kill y el hijo la atiende en su handler (traza real).' },
  '10_sigchld': { title: '10 · SIGCHLD', summary: 'El kernel avisa al padre cuando el hijo termina y el handler lo recoge (traza real).' },
  '11_sigint': { title: '11 · Ctrl+C', summary: 'Un bucle con handler de SIGINT: presiona Ctrl+C para interrumpirlo (traza real).' },
  '16_fork_bomb': { title: '16 · fork bomb', summary: 'fork en un bucle infinito: la traza se corta al llegar al límite de procesos (traza real).' },
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
