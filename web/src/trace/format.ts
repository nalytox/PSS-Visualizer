// Números como se leen en español: 1.000.000.
const NUMBER = new Intl.NumberFormat('es-CL');

export function fmt(n: number): string {
  return NUMBER.format(n);
}
