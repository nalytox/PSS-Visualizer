// Estado compartible por URL (sección 10). En la fase 0: la traza y el paso; desde la fase 1 se
// suman el código, el stdin, la semilla y la política de planificación.
export interface UrlState {
  trace?: string;
  t: number;
}

export function readHash(): UrlState {
  const params = new URLSearchParams(window.location.hash.slice(1));
  const t = Number(params.get('t') ?? 0);
  return { trace: params.get('traza') ?? undefined, t: Number.isFinite(t) && t >= 0 ? Math.floor(t) : 0 };
}

export function writeHash(s: UrlState): void {
  const params = new URLSearchParams();
  if (s.trace) params.set('traza', s.trace);
  params.set('t', String(s.t));
  const hash = '#' + params.toString();
  if (window.location.hash !== hash) window.history.replaceState(null, '', hash);
}
