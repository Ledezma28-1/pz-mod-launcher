
export const DISCLAIMER_VERSION = 1;

export function shouldShow(stored: string | null): boolean {
  return stored !== String(DISCLAIMER_VERSION);
}
