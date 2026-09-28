import { readFileSync } from 'node:fs';
import { TraceBuilder, charArray, hex, ptr, scalar, struct, variable } from './builder.ts';
import type { Value } from '../../web/src/trace/types.ts';

const source = readFileSync(new URL('./programs/structs_heap.c', import.meta.url), 'utf8');

export function build() {
  const b = new TraceBuilder({ source });
  const L = (s: string, n = 1) => b.L(s, n);
  const P = 1000;

  const PTS = 0x7fffffffc900;
  const R = 0x7fffffffc910;
  const LIST = 0x7fffffffc8e8;
  const SECOND = 0x7fffffffc8f0;
  const OLD = 0x7fffffffc8f8;
  const I = 0x7fffffffc8e4;
  const HEAD = 0x7fffffffc8b8;
  const VALUE = 0x7fffffffc8b4;
  const N = 0x7fffffffc8c8;
  const NODES = [0x4052a0, 0x4052c0, 0x4052e0];

  const point = (addr: number, x: number, y: number, uninit = false) =>
    struct(addr, [
      { name: 'x', type: 'int', size: 4, value: () => scalar(x), uninit },
      { name: 'y', type: 'int', size: 4, value: () => scalar(y), uninit },
    ]);
  const points = (uninit: boolean): Value => ({
    kind: 'array',
    length: 2,
    items: [point(PTS, 1, 2, uninit), point(PTS + 8, 3, 4, uninit)],
  });
  const rect = (uninit: boolean) =>
    struct(R, [
      { name: 'name', type: 'char[8]', size: 8, value: () => charArray(8, uninit ? '' : 'caja'), uninit },
      { name: 'corner', type: 'struct point', size: 8, value: (a) => point(a, 5, 6, uninit), uninit },
      { name: 'center', type: 'struct point *', size: 8, value: () => ptr(uninit ? null : hex(PTS + 8)), uninit },
    ]);
  const node = (addr: number, value: number | null, next: number | null | undefined) =>
    struct(addr, [
      { name: 'value', type: 'int', size: 4, value: () => scalar(value ?? 0), uninit: value === null },
      {
        name: 'next',
        type: 'struct node *',
        size: 8,
        offset: 8,
        value: () => ptr(next ? hex(next) : null),
        uninit: next === undefined,
      },
    ]);

  b.spawn(P, {
    line: L('struct point pts[2]'),
    locals: [variable('pts', 'struct point[2]', PTS, 16, points(true), true)],
  });
  b.initial();

  b.step(P, P, L('struct rect r'), () => {
    b.set(b.local(P, P, 'pts'), points(false));
    b.declare(P, P, variable('r', 'struct rect', R, 24, rect(true), true));
  });
  b.step(P, P, L('struct node *list = NULL;'), () => {
    b.set(b.local(P, P, 'r'), rect(false));
    b.declare(P, P, variable('list', 'struct node *', LIST, 8, ptr(null), true));
  });
  b.step(P, P, L('for (int i = 1'), () => b.set(b.local(P, P, 'list'), ptr(null)));

  const pushLine = L('list = push(list, i * 10);');
  b.step(P, P, pushLine, () => b.declare(P, P, variable('i', 'int', I, 4, scalar(1))));

  let head: number | null = null;
  for (let k = 0; k < 3; k++) {
    const addr = NODES[k];
    const val = (k + 1) * 10;
    b.step(P, P, undefined, () => {
      b.pushFrame(P, P, 'push', L('struct node *n = malloc'),
        [
          variable('head', 'struct node *', HEAD, 8, ptr(head === null ? null : hex(head))),
          variable('value', 'int', VALUE, 4, scalar(val)),
        ],
        [variable('n', 'struct node *', N, 8, ptr(null), true)]);
    });
    b.step(P, P, L('n->value = value;'), () => {
      b.call('malloc', 'malloc(16)');
      b.malloc(P, addr, 16, 'struct node', node(addr, null, undefined), L('struct node *n = malloc'));
      b.set(b.local(P, P, 'n'), ptr(hex(addr)));
    });
    b.step(P, P, L('n->next = head;'), () => {
      b.heapBlock(P, addr).value = node(addr, val, undefined);
    });
    b.step(P, P, L('return n;'), () => {
      b.heapBlock(P, addr).value = node(addr, val, head);
    });
    b.step(P, P, L('for (int i = 1'), () => {
      b.popFrame(P, P, ptr(hex(addr)));
      b.set(b.local(P, P, 'list'), ptr(hex(addr)));
    });
    const last = k === 2;
    b.step(P, P, last ? L('struct node *second') : pushLine, () => {
      if (last) {
        b.undeclare(P, P, 'i');
        b.declare(P, P, variable('second', 'struct node *', SECOND, 8, ptr(null), true));
      } else {
        b.set(b.local(P, P, 'i'), scalar(k + 2));
      }
    });
    head = addr;
  }

  b.step(P, P, L('struct node *old'), () => {
    b.set(b.local(P, P, 'second'), ptr(hex(NODES[1])));
    b.declare(P, P, variable('old', 'struct node *', OLD, 8, ptr(null), true));
  });
  b.step(P, P, L('free(list);'), () => b.set(b.local(P, P, 'old'), ptr(hex(NODES[2]))));
  b.step(P, P, L('list = NULL;', 2), () => {
    b.call('free');
    b.free(P, NODES[2]);
  });
  b.step(P, P, L('printf("second'), () => b.set(b.local(P, P, 'list'), ptr(null)));
  b.step(P, P, L('return 0;'), () => {
    b.call('printf');
    b.print(P, P, 'second->value = 20\n');
  });
  b.step(P, P, undefined, () => {
    b.popFrame(P, P, scalar(0));
    b.exit(P, 0);
  });

  return b.build({ kind: 'exited', code: 0 });
}
