// Motor de la introducción (sección 11). Un capítulo es un guion declarativo: tweens que llevan una
// propiedad de un elemento de un valor a otro entre dos instantes, y subtítulos por escena. El
// fotograma es una función pura del tiempo: el mismo t dibuja siempre lo mismo, así que adelantar,
// retroceder y arrastrar la barra no acumulan errores.

export type Ease = 'linear' | 'inOut' | 'out' | 'in';

export interface Tween {
  id: string;
  prop: string;
  from: number;
  to: number;
  start: number; // segundos
  end: number;
  ease?: Ease;
}

export interface Caption {
  start: number;
  end: number;
  text: string;
}

export type Values = (id: string, prop: string, fallback?: number) => number;

const EASES: Record<Ease, (u: number) => number> = {
  linear: (u) => u,
  in: (u) => u * u,
  out: (u) => 1 - (1 - u) * (1 - u),
  inOut: (u) => (u < 0.5 ? 2 * u * u : 1 - (-2 * u + 2) ** 2 / 2),
};

export function ease(kind: Ease, u: number): number {
  return EASES[kind](Math.min(1, Math.max(0, u)));
}

export interface Timeline {
  duration: number;
  values: (t: number) => Values;
}

// Índice de tweens por elemento y propiedad, ordenados por inicio.
export function timeline(tweens: Tween[], duration: number): Timeline {
  const byKey = new Map<string, Tween[]>();
  for (const tw of tweens) {
    const key = `${tw.id}.${tw.prop}`;
    if (!byKey.has(key)) byKey.set(key, []);
    byKey.get(key)!.push(tw);
  }
  for (const list of byKey.values()) list.sort((a, b) => a.start - b.start);
  return {
    duration,
    values: (t) => (id, prop, fallback = 0) => {
      const list = byKey.get(`${id}.${prop}`);
      if (!list) return fallback;
      // Antes del primer tween vale su "from"; después, el último tween que ya empezó manda.
      let current = list[0];
      for (const tw of list) if (tw.start <= t) current = tw;
      if (t <= current.start) return current.from;
      if (t >= current.end) return current.to;
      const u = (t - current.start) / (current.end - current.start);
      return current.from + (current.to - current.from) * ease(current.ease ?? 'inOut', u);
    },
  };
}

// Atajos para escribir guiones.
export const tw = (id: string, prop: string, from: number, to: number, start: number, end: number, e: Ease = 'inOut'): Tween => ({
  id,
  prop,
  from,
  to,
  start,
  end,
  ease: e,
});

export const show = (id: string, start: number, dur = 0.5): Tween => tw(id, 'o', 0, 1, start, start + dur);
export const hide = (id: string, start: number, dur = 0.5): Tween => tw(id, 'o', 1, 0, start, start + dur);

export function captionAt(captions: Caption[], t: number): Caption | undefined {
  return captions.find((c) => t >= c.start && t < c.end) ?? (t >= (captions.at(-1)?.end ?? 0) ? captions.at(-1) : undefined);
}

// Fotogramas clave para prefers-reduced-motion: el final de cada escena.
export function keyframes(captions: Caption[]): number[] {
  return captions.map((c) => c.end - 0.01);
}

// Texto que se escribe letra a letra: cuántos caracteres se ven en t.
export function typed(text: string, v: number): string {
  return text.slice(0, Math.round(Math.min(1, Math.max(0, v)) * text.length));
}
