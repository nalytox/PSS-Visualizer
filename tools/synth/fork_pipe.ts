import { readFileSync } from 'node:fs';
import { TraceBuilder, charArray, intArray, scalar, variable } from './builder.ts';

const source = readFileSync(new URL('./programs/fork_pipe.c', import.meta.url), 'utf8');

export function build() {
  const b = new TraceBuilder({ source, policy: 'manual' });
  const P = 1000;
  const C = 1001;
  const L = (s: string, n = 1) => b.L(s, n);

  const FD = 0x7fffffffc918;
  const PID = 0x7fffffffc910;
  const BUF = 0x7fffffffc920;
  const N = 0x7fffffffc914;
  const STATUS = 0x7fffffffc90c;
  const MSG = 0x7fffffffc920;

  b.spawn(P, {
    line: L('pipe(fd);'),
    locals: [variable('fd', 'int[2]', FD, 8, intArray([0, 0]), true)],
  });
  b.initial();

  b.step(P, P, L('pid_t pid = fork();'), () => {
    b.pipe(P, 3, 4);
    b.set(b.local(P, P, 'fd'), intArray([3, 4]));
    b.declare(P, P, variable('pid', 'pid_t', PID, 4, scalar(0), true));
  });

  b.step(P, P, L('if (pid == 0)'), () => {
    b.fork(P, C);
    b.moveTo(C, C, L('if (pid == 0)'));
    b.set(b.local(P, P, 'pid'), scalar(C));
    b.set(b.local(C, C, 'pid'), scalar(0));
  });

  b.step(C, C, L('close(fd[0]);'));
  b.step(P, P, L('close(fd[1]);', 2));

  b.step(C, C, L('char msg[]'), () => {
    b.close(C, 3);
    b.declare(C, C, variable('msg', 'char[6]', MSG, 6, charArray(6, ''), true));
  });
  b.step(P, P, L('char buf[16]'), () => {
    b.close(P, 4);
    b.declare(P, P, variable('buf', 'char[16]', BUF, 16, charArray(16, ''), true));
  });

  b.step(C, C, L('write(fd[1]'), () => {
    b.set(b.local(C, C, 'msg'), charArray(6, 'hola\n'));
  });
  b.step(P, P, L('while ((n = read'), () => {
    b.set(b.local(P, P, 'buf'), charArray(16, ''));
    b.declare(P, P, variable('n', 'int', N, 4, scalar(0), true));
  });

  // El padre llega al read antes de que el hijo escriba: el pipe está vacío y queda bloqueado.
  b.step(P, P, undefined, () => {
    b.block(P, P, { kind: 'read', fd: 3, pipe: 'p0' }, 'read');
  });

  b.step(C, C, L('close(fd[1]);', 1), () => {
    b.call('strlen', 'strlen(msg) = 5');
    b.writePipe(C, C, 4, 'hola\n');
    b.unblock(P, P);
  });

  b.step(P, P, L("buf[n] = '\\0';"), () => {
    b.endCall(P, P);
    b.readPipe(P, P, 3, 15, BUF);
    b.set(b.local(P, P, 'buf'), charArray(16, 'hola\n'));
    b.set(b.local(P, P, 'n'), scalar(5));
  });

  b.step(C, C, L('return 0;', 1), () => {
    b.close(C, 4);
  });

  b.step(P, P, L('printf("padre'));

  b.step(C, C, undefined, () => {
    b.popFrame(C, C, scalar(0));
    b.exit(C, 0);
  });

  b.step(P, P, L('while ((n = read'), () => {
    b.call('printf');
    b.print(P, P, 'padre leyó: hola\n');
  });

  // Ya no quedan escritores: read devuelve 0 (EOF) y el bucle termina.
  b.step(P, P, L('close(fd[0]);', 2), () => {
    b.readPipe(P, P, 3, 15, BUF);
    b.set(b.local(P, P, 'n'), scalar(0));
  });

  b.step(P, P, L('wait(&status);'), () => {
    b.close(P, 3);
    b.declare(P, P, variable('status', 'int', STATUS, 4, scalar(0), true));
  });

  b.step(P, P, L('printf("hijo'), () => {
    b.reap(P, C);
    b.set(b.local(P, P, 'status'), scalar(0));
  });

  b.step(P, P, L('return 0;', 2), () => {
    b.call('printf');
    b.print(P, P, 'hijo terminó con 0\n');
  });

  b.step(P, P, undefined, () => {
    b.popFrame(P, P, scalar(0));
    b.exit(P, 0);
  });

  return b.build({ kind: 'exited', code: 0 });
}
