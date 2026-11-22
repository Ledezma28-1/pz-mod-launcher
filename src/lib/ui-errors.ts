
const LENGTH = 600;
const MOST = 20;

export function errorNotes(): (e: unknown) => string | null {
  const seen = new Set<string>();
  return (e) => {
    const where = e instanceof Error ? (e.stack ?? '').split('\n').map((l) => l.trim()).filter((l) => l.startsWith('at ')) : [];
    const text = [e instanceof Error ? e.message : String(e), ...where].join(' | ').slice(0, LENGTH);
    if (seen.has(text) || seen.size >= MOST) return null;
    seen.add(text);
    return text;
  };
}
