import { readFileSync } from 'node:fs';
import { TraceBuilder, hex, opaque, ptr, scalar, variable } from './builder.ts';

const source = readFileSync(new URL('./programs/threads_mutex.c', import.meta.url), 'utf8');

export function build() {
  const b = new TraceBuilder({ source, policy: 'manual' });
  const L = (s: string, n = 1) => b.L(s, n);
  const P = 1000;
  const M = 1000;
  const A = 1001;
  const B = 1002;

  const COUNTER = 0x404060;
  const LOCK = 0x404080;
  const T1 = 0x7fffffffc918;
  const T2 = 0x7fffffffc920;
  const ID1 = 0x7fffffffc910;
  const ID2 = 0x7fffffffc914;
  // Cada hilo tiene su propia pila; la de B está 0x801000 bytes más abajo que la de A.
  const stackOf = (tid: number) => (tid === A ? 0 : 0x801000);
  const ARG = (tid: number) => 0x7ffff7bfeea8 - stackOf(tid);
  const ID = (tid: number) => 0x7ffff7bfeebc - stackOf(tid);
  const I = (tid: number) => 0x7ffff7bfeeb8 - stackOf(tid);
  const PTHREAD: Record<number, number> = { [A]: 0x7ffff7bff6c0, [B]: 0x7ffff73fe6c0 };

  b.spawn(P, {
    line: L('int id1 = 1'),
    globals: [
      variable('counter', 'int', COUNTER, 4, scalar(0)),
      variable('lock', 'pthread_mutex_t', LOCK, 40, opaque('mutex libre')),
    ],
    locals: [
      variable('t1', 'pthread_t', T1, 8, scalar(0), true),
      variable('t2', 'pthread_t', T2, 8, scalar(0), true),
      variable('id1', 'int', ID1, 4, scalar(0), true),
      variable('id2', 'int', ID2, 4, scalar(0), true),
    ],
  });
  b.addSync({ kind: 'mutex', id: 'm0', pid: P, addr: hex(LOCK), name: 'lock', owner: null, waiters: [] });
  b.initial();

  b.step(P, M, L('pthread_create(&t1'), () => {
    b.set(b.local(P, M, 'id1'), scalar(1));
    b.set(b.local(P, M, 'id2'), scalar(2));
  });

  const create = (tid: number, handle: string, idAddr: number) => {
    b.createThread(P, M, tid, 'worker', ptr(hex(idAddr)), L('int id = *(int *)arg;'),
      [variable('arg', 'void *', ARG(tid), 8, ptr(hex(idAddr)))],
      [variable('id', 'int', ID(tid), 4, scalar(0), true)]);
    b.set(b.local(P, M, handle), scalar(PTHREAD[tid], hex(PTHREAD[tid])));
  };

  const lockValue = () => {
    const m = b.mutex('m0');
    b.set(b.global(P, 'lock'), opaque(m.owner === null ? 'mutex libre' : `tomado por el hilo ${m.owner}`));
  };
  const setCounter = (v: number) => b.set(b.global(P, 'counter'), scalar(v));
  const forLine = L('for (int i = 0');
  const lockLine = L('pthread_mutex_lock(&lock);');
  const incLine = L('counter++;');
  const unlockLine = L('pthread_mutex_unlock(&lock);');
  const printfLine = L('printf("hilo');
  const retLine = L('return NULL;');

  const doLock = (tid: number) => {
    const r = b.lock(P, tid, 'm0');
    lockValue();
    return r;
  };
  const doUnlock = (tid: number) => {
    b.unlock(P, tid, 'm0');
    lockValue();
  };

  b.step(P, M, L('pthread_create(&t2'), () => create(A, 't1', ID1));
  b.step(P, A, forLine, () => b.set(b.local(P, A, 'id'), scalar(1)));
  b.step(P, M, L('pthread_join(t1'), () => create(B, 't2', ID2));
  b.step(P, B, forLine, () => b.set(b.local(P, B, 'id'), scalar(2)));
  b.step(P, M, undefined, () => b.block(P, M, { kind: 'join', tid: A }, 'pthread_join'));

  const enterLoop = (tid: number) =>
    b.step(P, tid, lockLine, () => {
      b.declare(P, tid, variable('i', 'int', I(tid), 4, scalar(0)));
    });
  enterLoop(A);
  enterLoop(B);

  let counter = 0;
  const inc = (tid: number) => b.step(P, tid, unlockLine, () => setCounter(++counter));
  const lockOk = (tid: number) => b.step(P, tid, incLine, () => doLock(tid));
  const lockBlocked = (tid: number) => b.step(P, tid, undefined, () => doLock(tid));
  const unlock = (tid: number) => b.step(P, tid, forLine, () => doUnlock(tid));
  const nextIter = (tid: number, i: number) =>
    b.step(P, tid, i < 2 ? lockLine : printfLine, () => {
      if (i < 2) b.set(b.local(P, tid, 'i'), scalar(i));
      else b.undeclare(P, tid, 'i');
    });

  lockOk(A);           // A toma el mutex
  lockBlocked(B);      // B lo encuentra ocupado y espera
  inc(A);
  unlock(A);           // al soltarlo, B despierta
  lockOk(B);
  nextIter(A, 1);
  inc(B);
  lockBlocked(A);
  unlock(B);
  lockOk(A);
  nextIter(B, 1);
  inc(A);
  lockBlocked(B);
  unlock(A);
  lockOk(B);
  nextIter(A, 2);
  inc(B);

  b.step(P, A, retLine, () => {
    b.call('printf');
    b.print(P, A, 'hilo 1 listo\n');
  });
  unlock(B);
  b.step(P, A, undefined, () => {
    b.threadExit(P, A, ptr(null));
    b.unblock(P, M);
  });
  nextIter(B, 2);
  b.step(P, M, L('pthread_join(t2'), () => {
    b.endCall(P, M);
    b.join(P, M, A);
  });
  b.step(P, B, retLine, () => {
    b.call('printf');
    b.print(P, B, 'hilo 2 listo\n');
  });
  b.step(P, M, undefined, () => b.block(P, M, { kind: 'join', tid: B }, 'pthread_join'));
  b.step(P, B, undefined, () => {
    b.threadExit(P, B, ptr(null));
    b.unblock(P, M);
  });
  b.step(P, M, L('printf("counter'), () => {
    b.endCall(P, M);
    b.join(P, M, B);
  });
  b.step(P, M, L('return 0;'), () => {
    b.call('printf');
    b.print(P, M, `counter = ${counter}\n`);
  });
  b.step(P, M, undefined, () => {
    b.popFrame(P, M, scalar(0));
    b.exit(P, 0);
  });

  return b.build({ kind: 'exited', code: 0 });
}
