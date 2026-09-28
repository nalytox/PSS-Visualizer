// Regenera traces/synthetic/*.json a partir de los guiones de esta carpeta.
import { writeFileSync } from 'node:fs';
import { build as forkPipe } from './fork_pipe.ts';
import { build as threadsMutex } from './threads_mutex.ts';
import { build as signalHandler } from './signal_handler.ts';
import { build as structsHeap } from './structs_heap.ts';
import { build as forkTree } from './fork_tree.ts';

const traces = {
  fork_pipe: forkPipe,
  threads_mutex: threadsMutex,
  signal_handler: signalHandler,
  structs_heap: structsHeap,
  fork_tree: forkTree,
};

for (const [name, build] of Object.entries(traces)) {
  const trace = build();
  const out = new URL(`../../traces/synthetic/${name}.json`, import.meta.url);
  writeFileSync(out, JSON.stringify(trace, null, 1) + '\n');
  console.log(`${name}: ${trace.steps.length} pasos, ${Object.keys(trace.snapshots).length} instantáneas`);
}
