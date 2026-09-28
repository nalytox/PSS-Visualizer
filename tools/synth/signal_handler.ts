import { readFileSync } from 'node:fs';
import { TraceBuilder, hex, opaque, ptr, scalar, struct, variable } from './builder.ts';

const source = readFileSync(new URL('./programs/signal_handler.c', import.meta.url), 'utf8');

export function build() {
  const b = new TraceBuilder({ source, policy: 'manual' });
  const L = (s: string, n = 1) => b.L(s, n);
  const P = 1000;
  const C = 1001;

  const GOT = 0x40405c;
  const SA = 0x7fffffffc890;
  const PID = 0x7fffffffc88c;
  const SIG = 0x7fffffffc2cc;
  const ON_USR1 = 0x401236;

  const sigaction = (handler: string | null, uninit = false) =>
    struct(SA, [
      { name: 'sa_handler', type: 'void (*)(int)', size: 8, value: () => (handler ? ptr(hex(ON_USR1), handler) : ptr(null)), uninit },
      { name: 'sa_mask', type: 'sigset_t', size: 128, value: () => opaque('{}'), uninit },
      { name: 'sa_flags', type: 'int', size: 4, offset: 136, value: () => scalar(0), uninit },
      { name: 'sa_restorer', type: 'void (*)(void)', size: 8, offset: 144, value: () => ptr(null), uninit },
    ]);

  b.spawn(P, {
    line: L('struct sigaction sa'),
    globals: [variable('got', 'volatile sig_atomic_t', GOT, 4, scalar(0))],
    locals: [variable('sa', 'struct sigaction', SA, 152, sigaction(null, true), true)],
  });
  b.initial();

  b.step(P, P, L('sa.sa_handler = on_usr1;'), () => b.set(b.local(P, P, 'sa'), sigaction(null)));
  b.step(P, P, L('sigaction(SIGUSR1'), () => b.set(b.local(P, P, 'sa'), sigaction('on_usr1')));
  b.step(P, P, L('pid_t pid = fork();'), () => {
    b.call('sigaction', 'SIGUSR1 → on_usr1');
    b.setAction(P, 'SIGUSR1', 'on_usr1');
    b.declare(P, P, variable('pid', 'pid_t', PID, 4, scalar(0), true));
  });
  b.step(P, P, L('if (pid == 0)'), () => {
    b.fork(P, C);
    b.moveTo(C, C, L('if (pid == 0)'));
    b.set(b.local(P, P, 'pid'), scalar(C));
    b.set(b.local(C, C, 'pid'), scalar(0));
  });

  b.step(C, C, L('while (!got)'));
  b.step(P, P, L('sleep(1);'));
  b.step(C, C, L('pause();'));
  b.step(P, P, undefined, () => b.block(P, P, { kind: 'sleep', until: 1000 }, 'sleep'));
  b.step(C, C, undefined, () => b.block(C, C, { kind: 'pause' }, 'pause'));

  // Todos duermen o esperan: el reloj virtual salta al próximo despertar.
  b.kernelStep(() => b.unblock(P, P), 1000);

  b.step(P, P, L('kill(pid, SIGUSR1);'), () => b.endCall(P, P));
  b.step(P, P, L('wait(NULL);'), () => {
    b.sendSignal({ kind: 'process', pid: P, via: 'kill' }, C, 'SIGUSR1');
    b.unblock(C, C);
  });

  // Entrega: el hilo del hijo se desvía al handler; no ejecuta ninguna línea propia.
  b.step(
    C,
    C,
    undefined,
    () => {
      b.endCall(C, C);
      b.deliverSignal(C, C, 'SIGUSR1', 'handler', {
        fn: 'on_usr1',
        line: L('got = 1;'),
        params: [variable('sig', 'int', SIG, 4, scalar(10, 'SIGUSR1'))],
        locals: [],
      });
    },
    { executed: false },
  );

  b.step(P, P, undefined, () => b.block(P, P, { kind: 'wait', target: -1 }, 'wait'));
  b.step(C, C, L('printf("hijo: recibí'), () => b.set(b.global(C, 'got'), scalar(1)));
  b.step(C, C, L('}', 1), () => {
    b.call('printf');
    b.print(C, C, 'hijo: recibí la señal 10\n');
  });
  b.step(C, C, L('while (!got)'), () => b.signalReturn(C, C, 'SIGUSR1'));
  b.step(C, C, L('printf("hijo: termino'));
  b.step(C, C, L('return 0;'), () => {
    b.call('printf');
    b.print(C, C, 'hijo: termino\n');
  });
  b.step(C, C, undefined, () => {
    b.popFrame(C, C, scalar(0));
    b.exit(C, 0);
    b.sendSignal({ kind: 'kernel', cause: 'SIGCHLD' }, P, 'SIGCHLD');
    b.unblock(P, P);
  });

  b.step(P, P, L('printf("padre'), () => {
    b.endCall(P, P);
    b.reap(P, C);
    b.deliverSignal(P, P, 'SIGCHLD', 'ignore');
  });
  b.step(P, P, L('return 0;', 2), () => {
    b.call('printf');
    b.print(P, P, 'padre: listo\n');
  });
  b.step(P, P, undefined, () => {
    b.popFrame(P, P, scalar(0));
    b.exit(P, 0);
  });

  return b.build({ kind: 'exited', code: 0 });
}
