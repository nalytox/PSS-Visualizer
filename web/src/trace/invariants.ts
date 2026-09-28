// Reglas de coherencia que el JSON Schema no puede expresar. Se aplican a toda traza en las pruebas.
import type { Trace } from './types.ts';

export function checkInvariants(trace: Trace): string[] {
  const errors: string[] = [];
  const err = (t: number, msg: string) => errors.push(`t=${t}: ${msg}`);

  trace.steps.forEach((step, i) => {
    if (step.t !== i) err(step.t, `se esperaba t=${i}`);
    const pids = new Set(step.processes.map((p) => p.pid));

    if (step.actor) {
      const p = step.processes.find((x) => x.pid === step.actor!.pid);
      if (!p) err(step.t, `el actor ${step.actor.pid} no existe`);
      else if (!p.threads.some((th) => th.tid === step.actor!.tid)) err(step.t, `el hilo actor ${step.actor.tid} no existe`);
    }
    if (i > 0 && step.clock < trace.steps[i - 1].clock) err(step.t, 'el reloj virtual retrocede');
    // Lo que el actor ejecuta es la línea donde estaba detenido al terminar el paso anterior.
    if (i > 0 && step.actor && step.executed) {
      const before = trace.steps[i - 1].processes.find((x) => x.pid === step.actor!.pid)?.threads.find((th) => th.tid === step.actor!.tid);
      if (before && before.line !== step.executed.line) {
        err(step.t, `el actor ejecuta la línea ${step.executed.line} pero estaba detenido en la ${before.line}`);
      }
    }

    for (const p of step.processes) {
      if (p.mem !== null && !(p.mem in trace.snapshots)) err(step.t, `instantánea ${p.mem} inexistente`);
      if (p.state === 'reaped' && p.mem !== null) err(step.t, `el proceso recogido ${p.pid} conserva memoria`);
      if (p.ppid !== null && p.ppid !== 1 && !pids.has(p.ppid)) err(step.t, `ppid ${p.ppid} de ${p.pid} no existe`);
      const running = p.threads.filter((th) => th.state === 'running');
      if (running.length > 1) err(step.t, `más de un hilo ejecutando en ${p.pid}`);
      if (running.length === 1 && (!step.actor || step.actor.tid !== running[0].tid)) {
        err(step.t, `el hilo ${running[0].tid} figura ejecutando pero no es el actor`);
      }
      for (const th of p.threads) {
        if (th.state === 'blocked' && !th.blockedOn) err(step.t, `hilo ${th.tid} bloqueado sin motivo`);
        if (th.state !== 'blocked' && th.blockedOn) err(step.t, `hilo ${th.tid} con motivo de bloqueo sin estar bloqueado`);
        if (p.mem !== null && th.state !== 'exited') {
          const frames = trace.snapshots[p.mem]?.stacks[String(th.tid)];
          if (!frames || frames.length === 0) err(step.t, `hilo ${th.tid} sin pila`);
          else if (frames[0].fn !== th.fn) err(step.t, `hilo ${th.tid}: fn ${th.fn} no coincide con el frame ${frames[0].fn}`);
        }
      }
      for (const [fd, entry] of Object.entries(p.fds)) {
        if (entry.kind !== 'pipe') continue;
        const pipe = step.pipes.find((x) => x.id === entry.pipe);
        const ends = entry.end === 'r' ? pipe?.readers : pipe?.writers;
        if (!ends?.some((e) => e.pid === p.pid && e.fd === Number(fd))) {
          err(step.t, `fd ${fd} de ${p.pid} apunta a ${entry.pipe}:${entry.end} pero el pipe no lo registra`);
        }
      }
    }

    for (const pipe of step.pipes) {
      if (pipe.buffer.length !== Math.min(pipe.size, 256)) err(step.t, `buffer de ${pipe.id} no coincide con size`);
      if (pipe.size > pipe.capacity) err(step.t, `${pipe.id} excede su capacidad`);
      for (const [list, end] of [[pipe.readers, 'r'], [pipe.writers, 'w']] as const) {
        for (const e of list) {
          const fd = step.processes.find((x) => x.pid === e.pid)?.fds[String(e.fd)];
          if (!fd || fd.kind !== 'pipe' || fd.pipe !== pipe.id || fd.end !== end) {
            err(step.t, `${pipe.id} registra ${e.pid}:${e.fd} (${end}) pero ese fd no es ese extremo`);
          }
        }
      }
    }

    for (const s of step.sync) {
      if (!pids.has(s.pid)) err(step.t, `objeto de sincronización ${s.id} de un proceso inexistente`);
    }
  });

  let lastT = 0;
  for (const chunk of trace.output) {
    if (chunk.t < lastT) err(chunk.t, 'la salida no está ordenada por t');
    lastT = chunk.t;
  }
  return errors;
}
