// Verifica el contraste WCAG AA (4,5:1) del texto sobre cada fondo relevante, en ambos temas.
import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

const css = readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');

function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector + ' {');
  const body = css.slice(start, css.indexOf('\n}', start));
  const vars: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*(#[0-9a-f]{6})/gi)) vars[m[1]] = m[2].toLowerCase();
  return vars;
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const light = block(':root');
const dark = { ...light, ...block(":root[data-theme='dark']") };

describe.each([
  ['claro', light],
  ['oscuro', dark],
])('tema %s', (_name, t) => {
  const pairs: [string, string][] = [
    ['text', 'bg'],
    ['text', 'surface'],
    ['text', 'surface-2'],
    ['text-2', 'surface'],
    ['text-2', 'bg'],
    ['text', 'changed'],
    ['text', 'blocked-soft'],
    ['text', 'orange-soft'],
    ['capsule-text', 'capsule'],
    ['stderr', 'surface'],
    ['blackbox-text', 'blackbox'],
    ...Array.from({ length: 8 }, (_, i) => ['text', `proc-${i}`] as [string, string]),
  ];
  it.each(pairs)('%s sobre %s cumple AA', (fg, bg) => {
    expect(t[fg], fg).toBeDefined();
    expect(t[bg], bg).toBeDefined();
    expect(contrast(t[fg], t[bg])).toBeGreaterThanOrEqual(4.5);
  });
});

it('el tema oscuro por preferencia del sistema repite exactamente el tema oscuro explícito', () => {
  const media = css.slice(css.indexOf("@media (prefers-color-scheme: dark)"));
  const auto = block(":root:not([data-theme='light'])");
  expect(media.length).toBeGreaterThan(0);
  expect(auto).toEqual(block(":root[data-theme='dark']"));
});

describe.each([
  ['claro', light],
  ['oscuro', dark],
])('código, tema %s', (_name, t) => {
  it.each(['code-keyword', 'code-type', 'code-string', 'code-number', 'code-fn'])('%s sobre surface cumple AA', (fg) => {
    expect(contrast(t[fg], t['surface'])).toBeGreaterThanOrEqual(4.5);
  });
});
