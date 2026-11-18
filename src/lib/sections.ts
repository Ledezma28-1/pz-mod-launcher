
import type { Finding, Row } from './types';

export type ViewItem =
  | {
      type: 'header';
      name: string | null;
      active: number;
      total: number;
      folded: boolean;
      problems: { blocks: number; warning: number };
      rows: Row[];
    }
  | { type: 'mod'; row: Row };

export function buildItems(
  rows: Row[],
  sections: string[],
  folded: Set<string>,
  visible: (r: Row) => boolean,
  findings: Finding[],
  selected: string | null,
  filtering = false,
): ViewItem[] {
  if (sections.length === 0) return rows.filter(visible).map((row) => ({ type: 'mod', row }));
  const known = new Set(sections);
  const groups = new Map<string | null, Row[]>([[null, []], ...sections.map((s) => [s, []] as [string, Row[]])]);
  for (const r of rows) groups.get(r.section !== null && known.has(r.section) ? r.section : null)!.push(r);
  const out: ViewItem[] = [];
  for (const [name, members] of groups) {
    if (name === null && members.length === 0) continue;
    const shown = members.filter(visible);
    if (filtering && shown.length === 0) continue;
    const ids = new Set(members.map((r) => r.id));
    const problems = { blocks: 0, warning: 0 };
    for (const f of findings) {
      if (!f.mods.some((m) => ids.has(m))) continue;
      if (f.severity === 'Blocks') problems.blocks++;
      else if (f.severity === 'Warning') problems.warning++;
    }
    const open =
      name === null || !folded.has(name) || (filtering && shown.length > 0) || (selected !== null && ids.has(selected));
    out.push({
      type: 'header',
      name,
      active: members.filter((r) => r.enabled).length,
      total: members.length,
      folded: !open,
      problems,
      rows: members,
    });
    if (open) for (const row of shown) out.push({ type: 'mod', row });
  }
  return out;
}

export function checkState(rows: Row[]): 'all' | 'some' | 'none' {
  const on = rows.filter((r) => r.enabled).length;
  return on === 0 ? 'none' : on === rows.length ? 'all' : 'some';
}

export function foldedKey(profileId: string): string {
  return `secciones-plegadas:${profileId}`;
}
