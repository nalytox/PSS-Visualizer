/* Generado por scripts/gen-types.ts desde schema/trace.schema.json. No editar a mano. */

/**
 * Entrada estándar usada en esta ejecución.
 */
export type Bytes = string;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Pid".
 */
export type Pid = number;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Tid".
 */
export type Tid = number;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SignalName".
 */
export type SignalName = string;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Outcome".
 */
export type Outcome =
  | OutcomeExited
  | OutcomeSignaled
  | OutcomeDeadlock
  | OutcomeAwaitingInput
  | OutcomeTruncated
  | OutcomeCompileError;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Event".
 */
export type Event =
  | EventCall
  | EventReturn
  | EventFork
  | EventThreadCreate
  | EventExec
  | EventExit
  | EventWait
  | EventJoin
  | EventReparent
  | EventPipe
  | EventDup
  | EventClose
  | EventRead
  | EventWrite
  | EventBlock
  | EventUnblock
  | EventSignalSend
  | EventSignalDeliver
  | EventSignalReturn
  | EventMutex
  | EventCond
  | EventSem
  | EventMalloc
  | EventFree
  | EventMemError
  | EventStdinNeeded
  | EventDeadlock
  | EventTruncated;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Value".
 */
export type Value = ScalarValue | PointerValue | StructValue | ArrayValue | OpaqueValue;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Addr".
 */
export type Addr = string;
/**
 * Si apunta a caracteres fuera de lo dibujado, el texto (hasta 64 bytes).
 */
export type Bytes1 = string;
/**
 * char[]: contenido hasta el primer \0.
 */
export type Bytes2 = string;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ExitStatus".
 */
export type ExitStatus = ExitCode | ExitSignal;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Fdnum".
 */
export type Fdnum = number;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Fd".
 */
export type Fd = FdStdin | FdTerminal | FdPipe | FdFile | FdOther;
/**
 * Truncado a 256 bytes.
 */
export type Bytes3 = string;
/**
 * Dirección del buffer destino.
 */
export type Addr1 = string;
/**
 * Truncado a 256 bytes.
 */
export type Bytes4 = string;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockReason".
 */
export type BlockReason =
  | BlockRead
  | BlockWrite
  | BlockWait
  | BlockJoin
  | BlockMutex
  | BlockCond
  | BlockSem
  | BlockSleep
  | BlockPause
  | BlockSigsuspend;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SignalSource".
 */
export type SignalSource = SourceProcess | SourceKernel | SourceTimer | SourceTerminal;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ProcessImage".
 */
export type ProcessImage = ImageUser | ImageBlackbox;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SignalAction".
 */
export type SignalAction = SignalActionHandler | SignalActionIgnore;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SnapshotId".
 */
export type SnapshotId = string;
/**
 * Primeros 256 bytes del buffer.
 */
export type Bytes5 = string;
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SyncObject".
 */
export type SyncObject = SyncMutex | SyncCond | SyncSem;
/**
 * Presente si es el frame de un handler.
 */
export type SignalName1 = string;
/**
 * Cada carácter U+0000–U+00FF representa exactamente un byte. El frontend decodifica como UTF-8 para mostrar.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Bytes".
 */
export type Bytes6 = string;

/**
 * Traza de ejecución de un programa C: lista ordenada de pasos globales con el estado completo del sistema después de cada paso.
 */
export interface Trace {
  version: 1;
  arch: 'x86_64' | 'aarch64';
  /**
   * Código C del programa.
   */
  source: string;
  stdin: Bytes;
  run: RunConfig;
  compile: CompileResult;
  outcome: Outcome;
  truncated: boolean;
  truncatedReason?: 'steps' | 'time' | 'processes' | 'threads' | 'memory' | 'output' | 'traceSize';
  steps: Step[];
  /**
   * Instantáneas de memoria completas, deduplicadas por contenido. Process.mem apunta aquí.
   */
  snapshots: {
    [k: string]: MemorySnapshot;
  };
  /**
   * Registro único de todo lo escrito a la terminal, ordenado por t.
   */
  output: OutputChunk[];
  summary: Summary;
}
/**
 * Todo lo necesario para reproducir la traza idéntica.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "RunConfig".
 */
export interface RunConfig {
  policy: 'round_robin' | 'random' | 'manual';
  seed: number;
  /**
   * true: al agotarse el stdin, read devuelve 0 (como `< archivo`).
   */
  stdinEof: boolean;
  /**
   * Prefijo de planificación forzado.
   */
  schedule: TaskRef[];
  injections: Injection[];
  limits: Limits;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "TaskRef".
 */
export interface TaskRef {
  pid: Pid;
  tid: Tid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Injection".
 */
export interface Injection {
  t: number;
  signal: SignalName;
  target: 'foreground';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Limits".
 */
export interface Limits {
  maxProcesses: number;
  maxThreadsPerProcess: number;
  maxSteps: number;
  wallTimeMs: number;
  memoryBytes: number;
  outputBytesPerProcess: number;
  traceBytesCompressed: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "CompileResult".
 */
export interface CompileResult {
  ok: boolean;
  command: string;
  diagnostics: Diagnostic[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Diagnostic".
 */
export interface Diagnostic {
  line: number;
  col: number;
  severity: 'error' | 'warning' | 'note';
  message: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeExited".
 */
export interface OutcomeExited {
  kind: 'exited';
  code: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeSignaled".
 */
export interface OutcomeSignaled {
  kind: 'signaled';
  signal: SignalName;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeDeadlock".
 */
export interface OutcomeDeadlock {
  kind: 'deadlock';
  tasks: TaskRef[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeAwaitingInput".
 */
export interface OutcomeAwaitingInput {
  kind: 'awaitingInput';
  pid: Pid;
  tid: Tid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeTruncated".
 */
export interface OutcomeTruncated {
  kind: 'truncated';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutcomeCompileError".
 */
export interface OutcomeCompileError {
  kind: 'compileError';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Step".
 */
export interface Step {
  /**
   * Reloj global; t = 0 es el estado inicial, detenido en la primera línea de main.
   */
  t: number;
  /**
   * Tarea que avanzó; null en t = 0 o en pasos solo del kernel.
   */
  actor: TaskRef | null;
  executed?: ExecutedLine;
  /**
   * Tareas listas que el planificador podía elegir.
   */
  choices: TaskRef[];
  /**
   * Reloj virtual en ms.
   */
  clock: number;
  events: Event[];
  processes: Process[];
  pipes: Pipe[];
  stdin: StdinState;
  signals: InFlightSignal[];
  timers: Timer[];
  sync: SyncObject[];
}
/**
 * Línea que acaba de ejecutar el actor (flecha verde de Python Tutor).
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ExecutedLine".
 */
export interface ExecutedLine {
  line: number;
  fn: string;
}
/**
 * Llamada atómica a libc (printf, strlen…).
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventCall".
 */
export interface EventCall {
  type: 'call';
  fn: string;
  summary?: string;
}
/**
 * Retorno de una función del usuario.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventReturn".
 */
export interface EventReturn {
  type: 'return';
  fn: string;
  value?: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ScalarValue".
 */
export interface ScalarValue {
  kind: 'scalar';
  /**
   * Enteros de 64 bits que no caben exactos en un double, NaN e Inf van como string.
   */
  value: number | string | boolean;
  /**
   * Representación para mostrar: 'a', RED, 3.14.
   */
  repr?: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "PointerValue".
 */
export interface PointerValue {
  kind: 'pointer';
  /**
   * null = NULL.
   */
  target: Addr | null;
  /**
   * Puntero a función: nombre del destino.
   */
  fn?: string;
  /**
   * Apunta a memoria válida que no se dibuja (literales de texto, datos de libc): no es un puntero colgante.
   */
  outside?: true;
  text?: Bytes1;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "StructValue".
 */
export interface StructValue {
  kind: 'struct' | 'union';
  fields: Field[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Field".
 */
export interface Field {
  name: string;
  type: string;
  addr: Addr;
  size: number;
  value: Value;
  uninit?: true;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ArrayValue".
 */
export interface ArrayValue {
  kind: 'array';
  /**
   * Cantidad total de elementos; items puede traer menos.
   */
  length: number;
  items: Value[];
  text?: Bytes2;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OpaqueValue".
 */
export interface OpaqueValue {
  kind: 'opaque';
  note: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventFork".
 */
export interface EventFork {
  type: 'fork';
  parent: Pid;
  child: Pid;
  vfork?: true;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventThreadCreate".
 */
export interface EventThreadCreate {
  type: 'threadCreate';
  pid: Pid;
  creator: Tid;
  tid: Tid;
  fn: string;
  arg: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventExec".
 */
export interface EventExec {
  type: 'exec';
  pid: Pid;
  path: string;
  argv: string[];
  blackbox: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventExit".
 */
export interface EventExit {
  type: 'exit';
  pid: Pid;
  tid?: Tid;
  scope: 'process' | 'thread';
  code?: number;
  signal?: SignalName;
  retval?: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventWait".
 */
export interface EventWait {
  type: 'wait';
  pid: Pid;
  target: number;
  reaped?: Pid;
  status?: ExitStatus;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ExitCode".
 */
export interface ExitCode {
  code: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ExitSignal".
 */
export interface ExitSignal {
  signal: SignalName;
  core: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventJoin".
 */
export interface EventJoin {
  type: 'join';
  tid: Tid;
  target: Tid;
  retval?: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventReparent".
 */
export interface EventReparent {
  type: 'reparent';
  pid: Pid;
  from: Pid;
  to: 1;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventPipe".
 */
export interface EventPipe {
  type: 'pipe';
  pid: Pid;
  pipe: string;
  /**
   * @minItems 2
   * @maxItems 2
   */
  fds: [Fdnum, Fdnum];
}
/**
 * dup y dup2.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventDup".
 */
export interface EventDup {
  type: 'dup';
  pid: Pid;
  oldfd: Fdnum;
  newfd: Fdnum;
  replaced?: Fd;
}
/**
 * Entrada estándar precargada.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "FdStdin".
 */
export interface FdStdin {
  kind: 'stdin';
  cloexec?: boolean;
}
/**
 * Consola (fd 1 y 2 por defecto).
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "FdTerminal".
 */
export interface FdTerminal {
  kind: 'terminal';
  cloexec?: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "FdPipe".
 */
export interface FdPipe {
  kind: 'pipe';
  pipe: string;
  end: 'r' | 'w';
  cloexec?: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "FdFile".
 */
export interface FdFile {
  kind: 'file';
  path: string;
  mode: 'r' | 'w' | 'rw' | 'a';
  cloexec?: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "FdOther".
 */
export interface FdOther {
  kind: 'other';
  label: string;
  cloexec?: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventClose".
 */
export interface EventClose {
  type: 'close';
  pid: Pid;
  fd: Fdnum;
  was: Fd;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventRead".
 */
export interface EventRead {
  type: 'read';
  pid: Pid;
  tid: Tid;
  fd: Fdnum;
  pipe?: string;
  stdin?: true;
  bytes: Bytes3;
  n: number;
  eof: boolean;
  into?: Addr1;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventWrite".
 */
export interface EventWrite {
  type: 'write';
  pid: Pid;
  tid: Tid;
  fd: Fdnum;
  pipe?: string;
  terminal?: true;
  bytes: Bytes4;
  n: number;
  epipe?: true;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventBlock".
 */
export interface EventBlock {
  type: 'block';
  pid: Pid;
  tid: Tid;
  reason: BlockReason;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockRead".
 */
export interface BlockRead {
  kind: 'read';
  fd: Fdnum;
  pipe?: string;
  stdin?: true;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockWrite".
 */
export interface BlockWrite {
  kind: 'write';
  fd: Fdnum;
  pipe: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockWait".
 */
export interface BlockWait {
  kind: 'wait';
  /**
   * Pid esperado; -1 = cualquier hijo.
   */
  target: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockJoin".
 */
export interface BlockJoin {
  kind: 'join';
  tid: Tid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockMutex".
 */
export interface BlockMutex {
  kind: 'mutex';
  id: string;
  owner: Tid | null;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockCond".
 */
export interface BlockCond {
  kind: 'cond';
  id: string;
  mutex: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockSem".
 */
export interface BlockSem {
  kind: 'sem';
  id: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockSleep".
 */
export interface BlockSleep {
  kind: 'sleep';
  /**
   * ms del reloj virtual.
   */
  until: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockPause".
 */
export interface BlockPause {
  kind: 'pause';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "BlockSigsuspend".
 */
export interface BlockSigsuspend {
  kind: 'sigsuspend';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventUnblock".
 */
export interface EventUnblock {
  type: 'unblock';
  pid: Pid;
  tid: Tid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventSignalSend".
 */
export interface EventSignalSend {
  type: 'signalSend';
  from: SignalSource;
  /**
   * Pid destino; negativo = grupo de procesos.
   */
  to: number;
  signal: SignalName;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SourceProcess".
 */
export interface SourceProcess {
  kind: 'process';
  pid: Pid;
  via: 'kill' | 'raise';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SourceKernel".
 */
export interface SourceKernel {
  kind: 'kernel';
  cause: 'SIGCHLD' | 'SIGPIPE' | 'fault';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SourceTimer".
 */
export interface SourceTimer {
  kind: 'timer';
  pid: Pid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SourceTerminal".
 */
export interface SourceTerminal {
  kind: 'terminal';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventSignalDeliver".
 */
export interface EventSignalDeliver {
  type: 'signalDeliver';
  pid: Pid;
  tid: Tid;
  signal: SignalName;
  action: 'handler' | 'ignore' | 'terminate' | 'core' | 'stop' | 'continue';
  handler?: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventSignalReturn".
 */
export interface EventSignalReturn {
  type: 'signalReturn';
  pid: Pid;
  tid: Tid;
  signal: SignalName;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventMutex".
 */
export interface EventMutex {
  type: 'mutex';
  op: 'lock' | 'trylock' | 'unlock';
  id: string;
  tid: Tid;
  result: 'acquired' | 'blocked' | 'busy' | 'released';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventCond".
 */
export interface EventCond {
  type: 'cond';
  op: 'wait' | 'signal' | 'broadcast' | 'wake';
  id: string;
  tid: Tid;
  woke?: Tid[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventSem".
 */
export interface EventSem {
  type: 'sem';
  op: 'wait' | 'trywait' | 'post';
  id: string;
  tid: Tid;
  value: number;
  result: 'acquired' | 'blocked' | 'busy' | 'posted';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventMalloc".
 */
export interface EventMalloc {
  type: 'malloc';
  pid: Pid;
  fn: string;
  addr: Addr | null;
  size: number;
  oldAddr?: Addr;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventFree".
 */
export interface EventFree {
  type: 'free';
  pid: Pid;
  addr: Addr;
  error?: 'doubleFree' | 'invalidPointer';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventMemError".
 */
export interface EventMemError {
  type: 'memError';
  pid: Pid;
  tid: Tid;
  kind: 'useAfterFree' | 'segfault';
  addr: Addr;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventStdinNeeded".
 */
export interface EventStdinNeeded {
  type: 'stdinNeeded';
  pid: Pid;
  tid: Tid;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventDeadlock".
 */
export interface EventDeadlock {
  type: 'deadlock';
  tasks: TaskRef[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "EventTruncated".
 */
export interface EventTruncated {
  type: 'truncated';
  reason: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Process".
 */
export interface Process {
  pid: Pid;
  /**
   * 1 = adoptado por init.
   */
  ppid: Pid | null;
  pgid: Pid;
  state: 'running' | 'ready' | 'blocked' | 'stopped' | 'zombie' | 'reaped';
  createdAt: number;
  image: ProcessImage;
  exit?: ExitStatus;
  fds: {
    [k: string]: Fd;
  };
  signals: ProcessSignals;
  threads: Thread[];
  mem: SnapshotId | null;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ImageUser".
 */
export interface ImageUser {
  kind: 'user';
  path: string;
}
/**
 * Binario sin símbolos cargado con exec: se muestra como caja negra.
 *
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ImageBlackbox".
 */
export interface ImageBlackbox {
  kind: 'blackbox';
  path: string;
  argv: string[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ProcessSignals".
 */
export interface ProcessSignals {
  mask: SignalName[];
  pending: SignalName[];
  /**
   * Solo las disposiciones distintas de la por defecto.
   */
  actions: {
    [k: string]: SignalAction;
  };
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SignalActionHandler".
 */
export interface SignalActionHandler {
  action: 'handler';
  fn: string;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SignalActionIgnore".
 */
export interface SignalActionIgnore {
  action: 'ignore';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Thread".
 */
export interface Thread {
  tid: Tid;
  main: boolean;
  state: 'running' | 'ready' | 'blocked' | 'stopped' | 'exited';
  /**
   * Próxima línea a ejecutar (flecha roja).
   */
  line: number | null;
  fn: string | null;
  /**
   * Función de libc en curso.
   */
  inCall?: string;
  blockedOn?: BlockReason;
  start?: ThreadStart;
  /**
   * Ids de mutex tomados.
   */
  holds: string[];
  inHandler?: SignalName;
  retval?: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "ThreadStart".
 */
export interface ThreadStart {
  fn: string;
  arg: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Pipe".
 */
export interface Pipe {
  id: string;
  createdBy: Pid;
  size: number;
  buffer: Bytes5;
  capacity: number;
  readers: PipeEnd[];
  writers: PipeEnd[];
  broken?: boolean;
  warnings: PipeWarning[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "PipeEnd".
 */
export interface PipeEnd {
  pid: Pid;
  fd: Fdnum;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "PipeWarning".
 */
export interface PipeWarning {
  kind: 'unclosedEnd';
  pid: Pid;
  fd: Fdnum;
  end: 'r' | 'w';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "StdinState".
 */
export interface StdinState {
  size: number;
  consumed: number;
  eof: boolean;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "InFlightSignal".
 */
export interface InFlightSignal {
  signal: SignalName;
  from: SignalSource;
  to: Pid;
  status: 'pending' | 'blocked';
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Timer".
 */
export interface Timer {
  pid: Pid;
  signal: 'SIGALRM';
  fireAt: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SyncMutex".
 */
export interface SyncMutex {
  kind: 'mutex';
  id: string;
  pid: Pid;
  addr: Addr;
  /**
   * Nombre de la variable según DWARF.
   */
  name?: string;
  waiters: Tid[];
  owner: Tid | null;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SyncCond".
 */
export interface SyncCond {
  kind: 'cond';
  id: string;
  pid: Pid;
  addr: Addr;
  /**
   * Nombre de la variable según DWARF.
   */
  name?: string;
  waiters: Tid[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "SyncSem".
 */
export interface SyncSem {
  kind: 'sem';
  id: string;
  pid: Pid;
  addr: Addr;
  /**
   * Nombre de la variable según DWARF.
   */
  name?: string;
  waiters: Tid[];
  value: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "MemorySnapshot".
 */
export interface MemorySnapshot {
  globals: Var[];
  /**
   * Clave = tid; frame [0] = el más reciente.
   */
  stacks: {
    [k: string]: Frame[];
  };
  heap: HeapBlock[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Var".
 */
export interface Var {
  name: string;
  type: string;
  addr: Addr;
  size: number;
  value: Value;
  uninit?: true;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Frame".
 */
export interface Frame {
  fn: string;
  line: number;
  params: Var[];
  locals: Var[];
  signal?: SignalName1;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "HeapBlock".
 */
export interface HeapBlock {
  addr: Addr;
  size: number;
  /**
   * Tipo inferido del puntero que lo apunta.
   */
  type?: string;
  allocAt: number;
  allocLine: number;
  freedAt?: number;
  value: Value;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "OutputChunk".
 */
export interface OutputChunk {
  t: number;
  pid: Pid;
  fd: Fdnum;
  stream: 'stdout' | 'stderr';
  bytes: Bytes6;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Summary".
 */
export interface Summary {
  leaks: Leak[];
  memErrors: MemErrorRef[];
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "Leak".
 */
export interface Leak {
  pid: Pid;
  addr: Addr;
  size: number;
  type?: string;
  allocAt: number;
  allocLine: number;
}
/**
 * This interface was referenced by `Trace`'s JSON-Schema
 * via the `definition` "MemErrorRef".
 */
export interface MemErrorRef {
  t: number;
  pid: Pid;
  tid: Tid;
  kind: 'useAfterFree' | 'doubleFree' | 'invalidFree' | 'segfault';
  addr: Addr;
}
