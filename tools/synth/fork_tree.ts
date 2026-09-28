// Prueba de carga: 4 iteraciones de fork() producen 16 procesos.
import { readFileSync } from 'node:fs';
import { TraceBuilder, scalar, variable } from './builder.ts';

const source = readFileSync(new URL('./programs/fork_tree.c', import.meta.url), 'utf8');

export function build() {
  const b = new TraceBuilder({ source });
  const L = (s: string) => b.L(s);
  const I = 0x7fffffffc91c;
  const FOR = L('for (int i');
  const FORK = L('fork();');
  const PRINT = L('printf(');
  const RET = L('return 0;');

  interface Task {
    pid: number;
    i: number;
    phase: 'for' | 'fork' | 'print' | 'ret' | 'done';
  }
  let nextPid = 1001;
  b.spawn(1000, { line: FOR });
  b.initial();
  const tasks: Task[] = [{ pid: 1000, i: -1, phase: 'for' }];

  // Planificación round-robin sobre las tareas vivas, en orden de creación.
  let turn = 0;
  while (tasks.some((x) => x.phase !== 'done')) {
    const live = tasks.filter((x) => x.phase !== 'done');
    const task = live[turn % live.length];
    turn++;
    const { pid } = task;
    if (task.phase === 'for') {
      task.i++;
      const inLoop = task.i < 4;
      b.step(pid, pid, inLoop ? FORK : PRINT, () => {
        if (task.i === 0) b.declare(pid, pid, variable('i', 'int', I, 4, scalar(0)));
        else if (inLoop) b.set(b.local(pid, pid, 'i'), scalar(task.i));
        else b.undeclare(pid, pid, 'i');
      });
      task.phase = inLoop ? 'fork' : 'print';
    } else if (task.phase === 'fork') {
      const child = nextPid++;
      b.step(pid, pid, FOR, () => {
        b.fork(pid, child);
        b.moveTo(child, child, FOR);
      });
      task.phase = 'for';
      tasks.push({ pid: child, i: task.i, phase: 'for' });
    } else if (task.phase === 'print') {
      b.step(pid, pid, RET, () => {
        b.call('printf');
        b.print(pid, pid, `soy ${pid}\n`);
      });
      task.phase = 'ret';
    } else if (task.phase === 'ret') {
      b.step(pid, pid, undefined, () => {
        b.popFrame(pid, pid, scalar(0));
        b.exit(pid, 0);
      });
      task.phase = 'done';
    }
  }
  return b.build({ kind: 'exited', code: 0 });
}
