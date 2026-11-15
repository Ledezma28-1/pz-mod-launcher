
export type Channel = 'estable' | 'beta';
export type Copy = { channel: string; id: number; version: string | null };
export type Local = {
  subscribed: boolean;
  downloading: boolean;
  downloaded: number;
  total: number;
  installed?: boolean;
  update?: boolean;
} | null;
export type Action =
  | { kind: 'subscribe' }
  | { kind: 'downloading'; pct: number }
  | { kind: 'install'; v: string }
  | { kind: 'switch'; v: string; from: Channel; fromV: string }
  | { kind: 'update'; v: string }
  | { kind: 'current' };

export function compareVersions(a: string, b: string): number {
  const [ma, ...ra] = a.trim().split('-');
  const [mb, ...rb] = b.trim().split('-');
  const pa = ma.split('.').map((x) => parseInt(x) || 0);
  const pb = mb.split('.').map((x) => parseInt(x) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  const sa = ra.join('-').toLowerCase();
  const sb = rb.join('-').toLowerCase();
  if (!sa || !sb) return (sa ? -1 : 0) - (sb ? -1 : 0);
  const [, la, na] = sa.match(/^([a-z]*)\.?(\d*)/)!;
  const [, lb, nb] = sb.match(/^([a-z]*)\.?(\d*)/)!;
  if (la !== lb) return la < lb ? -1 : 1;
  return (parseInt(na) || 0) - (parseInt(nb) || 0);
}

export function channelOf(
  inGame: string | null,
  copies: Copy[],
  remembered: string | null = null,
): Channel | null {
  if (!inGame) return null;
  const withVersion = copies.filter((c) => c.version);
  const same = withVersion.filter((c) => compareVersions(c.version!, inGame) === 0);
  const pick = same.find((c) => c.channel === remembered) ?? same[0];
  if (pick) return pick.channel as Channel;
  if (remembered && withVersion.some((c) => c.channel === remembered))
    return remembered as Channel;
  return withVersion.length === 1 ? (withVersion[0].channel as Channel) : null;
}

export function zbAction(
  channel: Channel,
  copy: Copy | undefined,
  local: Local,
  inGame: string | null,
  inGameChannel: Channel | null,
): Action {
  const v = copy?.version;
  if (local && !local.subscribed) return { kind: 'subscribe' };
  if (!v) {
    if (!local?.subscribed) return { kind: 'subscribe' };
    return {
      kind: 'downloading',
      pct: local.total ? Math.round((100 * local.downloaded) / local.total) : 0,
    };
  }
  if (!inGame) return { kind: 'install', v };
  if (inGameChannel && inGameChannel !== channel)
    return { kind: 'switch', v, from: inGameChannel, fromV: inGame };
  const d = compareVersions(v, inGame);
  if (d > 0) return { kind: 'update', v };
  if (d === 0) return { kind: 'current' };
  return { kind: 'install', v };
}

export function shouldInstall(
  pending: { channel: Channel; from: string | null } | null,
  copies: Copy[],
  local: Local,
): boolean {
  if (!pending) return false;
  const v = copies.find((c) => c.channel === pending.channel)?.version;
  if (!v) return false;
  if (v !== pending.from) return true;
  return !!local && local.subscribed && !local.downloading && !!local.installed && !local.update;
}

export function updateToNotify(
  inGame: string | null,
  inGameChannel: Channel | null,
  copies: Copy[],
  notified: string | null,
): string | null {
  if (!inGame || !inGameChannel) return null;
  const v = copies.find((c) => c.channel === inGameChannel)?.version;
  if (!v || compareVersions(v, inGame) <= 0 || v === notified) return null;
  return v;
}
