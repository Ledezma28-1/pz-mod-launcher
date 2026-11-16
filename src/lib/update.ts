
const REPO = 'https://github.com/Ledezma28-1/pz-mod-launcher';
export const RELEASES_API = 'https://api.github.com/repos/Ledezma28-1/pz-mod-launcher/releases/latest';

export const releaseUrl = (version: string) => `${REPO}/releases/tag/v${version.replace(/^v/i, '')}`;

export function issueUrl(app: string, game: string | null, zb: string | null): string {
  const q = new URLSearchParams({ template: 'bug.yml', app });
  if (game) q.set('game', game);
  if (zb) q.set('zb', zb);
  return `${REPO}/issues/new?${q}`;
}

export const justUpdated = (seen: string | null, current: string) => !!seen && isNewer(current, seen);

export type Release = { version: string; url: string };

function parts(v: string): { nums: number[]; pre: boolean } {
  const [core, pre] = v.trim().replace(/^v/i, '').split('-', 2);
  return { nums: core.split('.').map((n) => Number.parseInt(n, 10) || 0), pre: pre !== undefined };
}

export function isNewer(remote: string, local: string): boolean {
  const a = parts(remote);
  const b = parts(local);
  for (let i = 0; i < Math.max(a.nums.length, b.nums.length); i++) {
    const d = (a.nums[i] ?? 0) - (b.nums[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return b.pre && !a.pre;
}

export async function latestRelease(local: string, fetcher: typeof fetch = fetch): Promise<Release | null> {
  const r = await fetcher(RELEASES_API, { headers: { Accept: 'application/vnd.github+json' }, signal: AbortSignal.timeout(10_000) });
  if (!r.ok) throw new Error(`GitHub: HTTP ${r.status}`);
  const j = (await r.json()) as { tag_name?: string; html_url?: string };
  if (!j.tag_name || !j.html_url?.startsWith('https://github.com/')) return null;
  return isNewer(j.tag_name, local) ? { version: j.tag_name.replace(/^v/i, ''), url: j.html_url } : null;
}
