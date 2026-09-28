// Los bytes viajan en la traza como strings latin-1 (un carácter = un byte, ver 4.8 del contrato).

const decoder = new TextDecoder('utf-8', { fatal: false });

export function toUint8(bytes: string): Uint8Array {
  const out = new Uint8Array(bytes.length);
  for (let i = 0; i < bytes.length; i++) out[i] = bytes.charCodeAt(i) & 0xff;
  return out;
}

// Texto legible: decodifica UTF-8 para que "año" se vea bien.
export function decodeBytes(bytes: string): string {
  return decoder.decode(toUint8(bytes));
}

const SPECIAL: Record<string, string> = { '\n': '↵', '\0': '␀', '\t': '⇥', '\r': '␍' };

// Un carácter visible por unidad, para las cápsulas de un pipe.
export function capsules(bytes: string): string[] {
  return Array.from(decodeBytes(bytes), (ch) => SPECIAL[ch] ?? (ch < ' ' ? '·' : ch));
}

// Texto en una sola línea con los caracteres especiales a la vista.
export function visible(bytes: string): string {
  return capsules(bytes).join('');
}

export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  return `${Math.round(n / 1024)} KiB`;
}
