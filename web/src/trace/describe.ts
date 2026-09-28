// Frases en lenguaje cercano que explican qué pasó en cada paso. El detalle técnico va en tooltips.
import { visible } from './bytes.ts';
import { processAt, threadAt, threadLabel, type TraceIndex } from './query.ts';
import type { BlockReason, Event, Fd, Step, Trace, Value } from './types.ts';

function valueText(v: Value | undefined): string {
  if (!v) return '';
  switch (v.kind) {
    case 'scalar':
      return v.repr ?? String(v.value);
    case 'pointer':
      return v.fn ?? (v.target === null ? 'NULL' : v.target);
    case 'array':
      return v.text !== undefined ? `"${visible(v.text)}"` : `[${v.length}]`;
    case 'struct':
    case 'union':
      return '{…}';
    case 'opaque':
      return v.note;
  }
}

export function fdText(fd: Fd): string {
  switch (fd.kind) {
    case 'stdin':
      return 'la entrada estándar';
    case 'terminal':
      return 'la consola';
    case 'pipe':
      return `el extremo de ${fd.end === 'r' ? 'lectura' : 'escritura'} del pipe ${fd.pipe}`;
    case 'file':
      return `el archivo ${fd.path}`;
    case 'other':
      return fd.label;
  }
}

function mutexName(step: Step, id: string): string {
  const m = step.sync.find((s) => s.id === id);
  return m?.name ?? id;
}

function who(index: TraceIndex, step: Step, pid: number, tid: number): string {
  const p = processAt(step, pid);
  if (p && p.threads.length > 1) return `el hilo ${threadLabel(index, tid)} (${tid})`;
  return `el proceso ${pid}`;
}

export function blockText(index: TraceIndex, step: Step, pid: number, tid: number, r: BlockReason): string {
  const subject = who(index, step, pid, tid);
  switch (r.kind) {
    case 'read':
      if (r.stdin) return `${cap(subject)} espera que escribas algo en la entrada estándar.`;
      return `${cap(subject)} está esperando datos del pipe ${r.pipe ?? `del fd ${r.fd}`}.`;
    case 'write':
      return `${cap(subject)} espera que haya espacio en el pipe ${r.pipe}: está lleno.`;
    case 'wait':
      return r.target === -1
        ? `${cap(subject)} espera a que termine alguno de sus hijos.`
        : `${cap(subject)} espera a que termine su hijo ${r.target}.`;
    case 'join':
      return `${cap(subject)} espera a que termine el hilo ${threadLabel(index, r.tid)} (${r.tid}).`;
    case 'mutex':
      return `${cap(subject)} quiere el mutex ${mutexName(step, r.id)}, pero lo tiene el hilo ${r.owner ?? '?'}: queda esperando.`;
    case 'cond':
      return `${cap(subject)} espera una señal en la variable de condición ${mutexName(step, r.id)}.`;
    case 'sem':
      return `${cap(subject)} espera que el semáforo ${mutexName(step, r.id)} sea mayor que cero.`;
    case 'sleep':
      return `${cap(subject)} duerme hasta el instante ${r.until} ms.`;
    case 'pause':
      return `${cap(subject)} se detiene hasta que llegue una señal (pause).`;
    case 'sigsuspend':
      return `${cap(subject)} espera una señal (sigsuspend).`;
  }
}

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function eventText(index: TraceIndex, step: Step, prev: Step | undefined, ev: Event): string | null {
  switch (ev.type) {
    case 'call':
    case 'return':
      return null;
    case 'fork':
      return `El proceso ${ev.parent} llamó a fork: nace el proceso ${ev.child}, una copia casi exacta. fork devuelve ${ev.child} al padre y 0 al hijo.`;
    case 'threadCreate':
      return `Se crea el hilo ${threadLabel(index, ev.tid)} (${ev.tid}), que empieza en ${ev.fn}(${valueText(ev.arg)}).`;
    case 'exec':
      return `El proceso ${ev.pid} reemplaza su programa por ${ev.path}. Conserva su PID y sus descriptores abiertos.${
        ev.blackbox ? ' El nuevo programa no tiene símbolos de depuración: se ve como una caja negra, solo con su salida.' : ''
      }`;
    case 'exit':
      if (ev.scope === 'thread') return `El hilo ${threadLabel(index, ev.tid ?? 0)} (${ev.tid}) terminó.`;
      {
        const p = processAt(step, ev.pid);
        const how = ev.signal ? `fue terminado por ${ev.signal}` : `terminó con código ${ev.code}`;
        if (p?.ppid === null) return ev.signal ? `El programa ${how}.` : `El programa terminó con código ${ev.code}.`;
        if (p?.state === 'reaped') return `El proceso ${ev.pid} ${how}. Como era huérfano, init lo recoge de inmediato.`;
        return `El proceso ${ev.pid} ${how}. Queda zombie hasta que su padre haga wait.`;
      }
    case 'wait':
      if (ev.reaped === undefined) return `El proceso ${ev.pid} preguntó con WNOHANG: ningún hijo terminó todavía, así que sigue sin esperar.`;
      return `El proceso ${ev.pid} recogió a su hijo ${ev.reaped}${
        !ev.status ? '' : 'code' in ev.status ? `, que había salido con código ${ev.status.code}` : `, que murió por ${ev.status.signal}`
      }.`;
    case 'join':
      return `El hilo ${threadLabel(index, ev.tid)} se unió al hilo ${threadLabel(index, ev.target)}: ya terminó y sus recursos se liberan.`;
    case 'reparent':
      return `El proceso ${ev.pid} quedó huérfano: ahora su padre es init (1).`;
    case 'pipe':
      return `El proceso ${ev.pid} creó el pipe ${ev.pipe}: fd ${ev.fds[0]} para leer y fd ${ev.fds[1]} para escribir.`;
    case 'dup':
      return `dup2: el fd ${ev.newfd} del proceso ${ev.pid} ahora apunta a lo mismo que el fd ${ev.oldfd}.`;
    case 'close':
      return `El proceso ${ev.pid} cerró el fd ${ev.fd} (${fdText(ev.was)}).`;
    case 'read':
      if (ev.eof) {
        return ev.pipe
          ? `read devolvió 0: el pipe ${ev.pipe} está vacío y ya nadie puede escribir en él (fin de archivo).`
          : 'read devolvió 0: se acabó la entrada.';
      }
      return `El proceso ${ev.pid} leyó ${ev.n} bytes ${ev.pipe ? `del pipe ${ev.pipe}` : 'de la entrada estándar'}: "${visible(ev.bytes)}".`;
    case 'write':
      if (ev.terminal) return `El proceso ${ev.pid} escribió en la consola: "${visible(ev.bytes)}".`;
      return `El proceso ${ev.pid} escribió ${ev.n} bytes en el pipe ${ev.pipe}: "${visible(ev.bytes)}".`;
    case 'block':
      return blockText(index, step, ev.pid, ev.tid, ev.reason);
    case 'unblock': {
      const before = prev && threadAt(prev, ev.pid, ev.tid)?.blockedOn;
      const subject = cap(who(index, step, ev.pid, ev.tid));
      return before ? `${subject} ya puede continuar.` : `${subject} despierta.`;
    }
    case 'signalSend':
      switch (ev.from.kind) {
        case 'process':
          return `El proceso ${ev.from.pid} envía ${ev.signal} al proceso ${ev.to} con ${ev.from.via}().`;
        case 'kernel':
          return ev.signal === 'SIGCHLD'
            ? `El kernel avisa al proceso ${ev.to} con SIGCHLD: uno de sus hijos terminó.`
            : `El kernel envía ${ev.signal} al proceso ${ev.to}.`;
        case 'timer':
          return `Venció la alarma: ${ev.signal} para el proceso ${ev.to}.`;
        case 'terminal':
          return `Desde la terminal llega ${ev.signal} (Ctrl+C) al proceso ${ev.to}.`;
      }
      return null;
    case 'signalDeliver':
      switch (ev.action) {
        case 'handler':
          return `Llega ${ev.signal} al proceso ${ev.pid}: se interrumpe lo que hacía y se ejecuta ${ev.handler}().`;
        case 'ignore':
          return `${ev.signal} llegó al proceso ${ev.pid} y se ignoró.`;
        case 'terminate':
        case 'core':
          return `${ev.signal} terminó al proceso ${ev.pid}.`;
        case 'stop':
          return `${ev.signal} detuvo al proceso ${ev.pid}.`;
        case 'continue':
          return `${ev.signal} reanudó al proceso ${ev.pid}.`;
      }
      return null;
    case 'signalReturn':
      return `El handler de ${ev.signal} terminó: el proceso ${ev.pid} vuelve a donde estaba.`;
    case 'mutex':
      if (ev.result === 'acquired') return `El hilo ${threadLabel(index, ev.tid)} tomó el mutex ${mutexName(step, ev.id)}.`;
      if (ev.result === 'released') return `El hilo ${threadLabel(index, ev.tid)} soltó el mutex ${mutexName(step, ev.id)}.`;
      return null;
    case 'cond':
      return ev.op === 'signal' || ev.op === 'broadcast'
        ? `El hilo ${threadLabel(index, ev.tid)} avisa por la variable de condición ${mutexName(step, ev.id)}.`
        : null;
    case 'sem':
      return `Semáforo ${mutexName(step, ev.id)}: ${ev.op} deja el contador en ${ev.value}.`;
    case 'malloc':
      return `${ev.fn} reservó ${ev.size} bytes en el heap (${ev.addr ?? 'NULL'}).`;
    case 'free':
      return ev.error ? `free(${ev.addr}) es inválido: ${ev.error === 'doubleFree' ? 'ese bloque ya estaba liberado' : 'no es un bloque del heap'}.` : `free liberó el bloque ${ev.addr}.`;
    case 'memError':
      return ev.kind === 'useAfterFree' ? `Acceso a memoria ya liberada (${ev.addr}).` : `Acceso inválido a memoria (${ev.addr}): segmentation fault.`;
    case 'stdinNeeded':
      return 'El programa pide más entrada: escribe algo en stdin para continuar.';
    case 'deadlock':
      return 'Deadlock: todas las tareas vivas están bloqueadas esperándose entre sí.';
    case 'truncated':
      return `La traza se cortó: ${ev.reason}.`;
  }
}

export function describeStep(trace: Trace, index: TraceIndex, t: number): string[] {
  const step = trace.steps[t];
  const prev = trace.steps[t - 1];
  if (t === 0) {
    const main = step.processes[0]?.threads[0];
    return [`Estado inicial: el programa está por ejecutar la línea ${main?.line ?? '?'} de main.`];
  }
  const out = step.events.map((ev) => eventText(index, step, prev, ev)).filter((x): x is string => !!x);
  if (!step.actor && prev && step.clock > prev.clock) {
    out.unshift(`Todos están esperando: el reloj avanza hasta ${step.clock} ms.`);
  }
  if (out.length === 0 && step.actor && step.executed) {
    const ret = step.events.find((e) => e.type === 'return');
    const subject = who(index, step, step.actor.pid, step.actor.tid);
    out.push(
      ret && ret.type === 'return'
        ? `${cap(subject)} retornó de ${ret.fn}()${ret.value ? ` con ${valueText(ret.value)}` : ''}.`
        : `${cap(subject)} ejecutó la línea ${step.executed.line}.`,
    );
  }
  return out;
}
