// Cliente del servidor local (crates/server).
import type { TaskRef, Trace } from '../trace/types.ts';

type Policy = Trace['run']['policy'];

export interface RunRequest {
  source: string;
  stdin: string;
  stdinEof: boolean;
  injections?: { t: number; signal: string }[];
  policy?: Policy;
  seed?: number;
  schedule?: TaskRef[];
}

export async function serverAvailable(): Promise<boolean> {
  try {
    const res = await fetch('/api/health', { cache: 'no-store' });
    if (!res.ok) return false;
    const body = await res.json();
    return body.ok === true && body.canRun === true;
  } catch {
    return false;
  }
}

export async function runProgram(req: RunRequest, signal?: AbortSignal): Promise<Trace> {
  const res = await fetch('/api/run', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(req),
    signal,
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(body.error ?? `error ${res.status}`);
  }
  return (await res.json()) as Trace;
}
