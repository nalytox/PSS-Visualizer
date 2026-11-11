/* Generado por scripts/gen-types.ts desde schema/compact.schema.json. No editar a mano. */

/**
 * PID del actor de cada paso; 0 si no hubo actor.
 */
export type Column = string;
/**
 * TID del actor de cada paso; 0 si no hubo actor.
 */
export type Column1 = string;
/**
 * Próxima línea del actor al terminar el paso; 0 si no tiene.
 */
export type Column2 = string;
/**
 * Un bit por clase de evento del paso, en este orden: call, return, fork, threadCreate, exec, exitProcess, exitThread, wait, waitReaped, join, reparent, pipe, dup, close, read, write, writePipe, block, unblock, signalSend, signalDeliver, signalReturn, mutex, mutexAcquired, mutexReleased, cond, sem, malloc, free, memError, deadlock, other (stdinNeeded y truncated).
 */
export type Column3 = string;
/**
 * stepCount enteros sin signo de 32 bits, little-endian, comprimidos con gzip y en base64.
 *
 * This interface was referenced by `CompactTrace`'s JSON-Schema
 * via the `definition` "Column".
 */
export type Column4 = string;

/**
 * Traza compacta (versión 2) para ejecuciones largas. Los campos que no son pasos ni instantáneas son los mismos de trace.schema.json; los pasos van en bloques comprimidos. Ver schema/README.md.
 */
export interface CompactTrace {
  version: 2;
  arch: 'x86_64' | 'aarch64';
  source: string;
  stdin: string;
  /**
   * Igual que en trace.schema.json.
   */
  run: import('./types.ts').Trace['run'];
  /**
   * Igual que en trace.schema.json.
   */
  compile: import('./types.ts').Trace['compile'];
  /**
   * Igual que en trace.schema.json.
   */
  outcome: import('./types.ts').Trace['outcome'];
  truncated: boolean;
  /**
   * Igual que en trace.schema.json.
   */
  truncatedReason?: import('./types.ts').Trace['truncatedReason'];
  /**
   * Igual que en trace.schema.json.
   */
  output: import('./types.ts').Trace['output'];
  /**
   * Igual que en trace.schema.json.
   */
  summary: import('./types.ts').Trace['summary'];
  stepCount: number;
  index: StepIndex;
  chunks: Chunk[];
}
/**
 * Lo que la línea de tiempo y la navegación necesitan de cada paso, sin abrir bloques.
 *
 * This interface was referenced by `CompactTrace`'s JSON-Schema
 * via the `definition` "StepIndex".
 */
export interface StepIndex {
  actorPid: Column;
  actorTid: Column1;
  actorLine: Column2;
  flags: Column3;
  /**
   * Procesos en orden de aparición.
   */
  pids: number[];
  /**
   * Hilos en orden de aparición.
   */
  threads: {
    pid: number;
    tid: number;
  }[];
  /**
   * Cada fork, en orden.
   */
  forks: {
    parent: number;
    child: number;
  }[];
  /**
   * Mutex en orden de aparición.
   */
  mutexes: string[];
}
/**
 * Bloque de pasos consecutivos. data es {"steps": [...], "snapshots": {...}} con gzip y en base64: el primer paso completo y los siguientes como parches del anterior (con events vacío); cada instantánea completa o como {"$from": id, "$p": parche} de otra del mismo bloque.
 *
 * This interface was referenced by `CompactTrace`'s JSON-Schema
 * via the `definition` "Chunk".
 */
export interface Chunk {
  start: number;
  count: number;
  /**
   * Las instantáneas del bloque son m{firstSnapshot} en adelante.
   */
  firstSnapshot: number;
  data: string;
}
