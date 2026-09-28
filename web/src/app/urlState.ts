// Estado compartible por URL (sección 10): el programa, su stdin y el paso actual. El código viaja
// comprimido (deflate + base64url) para que la dirección no sea enorme.
export interface UrlState {
  trace?: string; // traza grabada de la galería
  example?: string; // ejemplo sin modificar
  code?: string;
  stdin?: string;
  eof?: boolean;
  ctrlc?: number[]; // pasos después de los cuales se presionó Ctrl+C
  t: number;
}

export async function readHash(): Promise<UrlState> {
  const params = new URLSearchParams(window.location.hash.slice(1));
  const t = Number(params.get('t') ?? 0);
  const state: UrlState = { t: Number.isFinite(t) && t >= 0 ? Math.floor(t) : 0 };
  if (params.get('traza')) state.trace = params.get('traza')!;
  if (params.get('ejemplo')) state.example = params.get('ejemplo')!;
  try {
    if (params.get('codigo')) state.code = await inflate(params.get('codigo')!);
    if (params.get('stdin')) state.stdin = await inflate(params.get('stdin')!);
  } catch {
    // Un enlace dañado no debe impedir abrir la aplicación.
  }
  state.eof = params.get('eof') === '1';
  const ctrlc = (params.get('ctrlc') ?? '').split(',').filter(Boolean).map(Number).filter((n) => Number.isInteger(n) && n >= 0);
  if (ctrlc.length > 0) state.ctrlc = ctrlc;
  return state;
}

export async function writeHash(s: UrlState): Promise<void> {
  const params = new URLSearchParams();
  if (s.trace) params.set('traza', s.trace);
  if (s.example) params.set('ejemplo', s.example);
  if (s.code !== undefined) params.set('codigo', await deflate(s.code));
  if (s.stdin) params.set('stdin', await deflate(s.stdin));
  if (s.eof) params.set('eof', '1');
  if (s.ctrlc && s.ctrlc.length > 0) params.set('ctrlc', s.ctrlc.join(','));
  params.set('t', String(s.t));
  const hash = '#' + params.toString();
  if (window.location.hash !== hash) window.history.replaceState(null, '', hash);
}

async function deflate(text: string): Promise<string> {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream('deflate-raw'));
  const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
  let bin = '';
  bytes.forEach((b) => (bin += String.fromCharCode(b)));
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

async function inflate(data: string): Promise<string> {
  const bin = atob(data.replace(/-/g, '+').replace(/_/g, '/'));
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return await new Response(stream).text();
}
