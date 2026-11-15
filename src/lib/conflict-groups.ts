
import type { Finding, Fix, Severity } from './types';

export interface ModEntry {
  mod: string;
  severity: Severity;
  findings: Finding[];
  fixes: Fix[];
}

export interface Cluster {
  key: string;
  severity: Severity;
  entries: ModEntry[];
  disable: string[];
}

const RANK: Severity[] = ['Blocks', 'Warning', 'Info', 'Detail'];
const worse = (a: Severity, b: Severity) => (RANK.indexOf(a) <= RANK.indexOf(b) ? a : b);

export function reasonKey(h: Finding): string | null {
  const x = h.kind;
  switch (x.kind) {
    case 'OnlyB41':
      return 'b41';
    case 'GameVersion':
      return 'version';
    case 'NoValidFolder':
      return 'nofolder';
    case 'ModNotFound':
      return 'notfound';
    case 'RequireTypo':
      return 'typo';
    case 'MissingDependency':
      return `missing:${x.inactive_installed ? 'off' : 'none'}:${x.requires}`;
    case 'WontLoad':
      return `wontload:${x.because[x.because.length - 1] ?? ''}`;
    default:
      return null;
  }
}

export function clusterByReason(findings: Finding[]): Cluster[] {
  const byMod = new Map<string, ModEntry>();
  const alone: ModEntry[] = [];
  for (const h of findings) {
    const mod = h.mods.length === 1 ? h.mods[0] : null;
    if (mod === null || reasonKey(h) === null) {
      alone.push({ mod: h.mods.join(' · '), severity: h.severity, findings: [h], fixes: [...h.fixes] });
      continue;
    }
    const e = byMod.get(mod) ?? { mod, severity: h.severity, findings: [], fixes: [] };
    e.severity = worse(e.severity, h.severity);
    e.findings.push(h);
    for (const f of h.fixes) {
      if (!e.fixes.some((g) => JSON.stringify(g) === JSON.stringify(f))) e.fixes.push(f);
    }
    byMod.set(mod, e);
  }
  const clusters = new Map<string, Cluster>();
  for (const e of byMod.values()) {
    e.findings.sort((a, b) => RANK.indexOf(a.severity) - RANK.indexOf(b.severity));
    e.fixes.sort((a, b) => Number(a.action === 'Disable') - Number(b.action === 'Disable'));
    const key = `${e.severity}|${reasonKey(e.findings[0])}`;
    const c = clusters.get(key) ?? { key: reasonKey(e.findings[0])!, severity: e.severity, entries: [], disable: [] };
    c.entries.push(e);
    if (e.fixes.some((f) => f.action === 'Disable' && f.data === e.mod)) c.disable.push(e.mod);
    clusters.set(key, c);
  }
  const out = [...clusters.values()];
  for (const e of alone) {
    out.push({ key: '', severity: e.severity, entries: [e], disable: [] });
  }
  return out.sort((a, b) => RANK.indexOf(a.severity) - RANK.indexOf(b.severity));
}

export function blockedMods(findings: Finding[]): string[] {
  const out = new Set<string>();
  for (const h of findings) {
    if (h.severity !== 'Blocks') continue;
    for (const f of h.fixes) {
      if (f.action === 'Disable' && h.mods.includes(f.data)) out.add(f.data);
    }
  }
  return [...out];
}
