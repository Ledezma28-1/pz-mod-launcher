export type Language = 'es' | 'en';

export function languageFor(saved: string | null, system: string | undefined): Language {
  if (saved === 'es' || saved === 'en') return saved;
  return system?.toLowerCase().startsWith('es') ? 'es' : 'en';
}
