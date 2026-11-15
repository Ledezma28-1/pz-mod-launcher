export function initialOf(name: string, fallback: string): string {
  const first = [...name.trim()][0] ?? [...fallback][0] ?? '';
  return first.toLocaleUpperCase();
}
