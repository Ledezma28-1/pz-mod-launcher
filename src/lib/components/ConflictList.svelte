<script lang="ts">
  import CircleX from '@lucide/svelte/icons/circle-x';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Info from '@lucide/svelte/icons/info';
  import Minus from '@lucide/svelte/icons/minus';
  import Check from '@lucide/svelte/icons/check';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Confirm from './Confirm.svelte';
  import { app } from '../state.svelte';
  import { clusterTitle, fixHelp, fixText, findingText, i18n, luaText, pairText, t, whyText } from '../i18n.svelte';
  import { blockedMods, clusterByReason, type Cluster, type ModEntry } from '../conflict-groups';
  import type { Severity, Finding, Fix, ProbeReport } from '../types';
  import { api } from '../api';

  const v = $derived(app.view!);
  let filter = $state<Severity | 'all'>('all');
  const notices = $derived(v.findings.filter((h) => h.severity !== 'Detail'));
  const list = $derived(filter === 'all' ? notices : v.findings.filter((h) => h.severity === filter));
  const blocked = $derived(blockedMods(v.findings));

  const icons = { Blocks: CircleX, Warning: TriangleAlert, Info, Detail: Minus } as const;
  const classes = { Blocks: 'severity-blocks', Warning: 'severity-warning', Info: 'severity-info', Detail: 'severity-detail' } as const;
  const sevKey = { Blocks: 'blocks', Warning: 'warning', Info: 'info', Detail: 'detail' } as const;

  let probe = $state<ProbeReport | null>(null);
  $effect(() => {
    const id = v.id;
    if (!v.settings.probe) {
      probe = null;
      return;
    }
    api
      .probeReport(id)
      .then((r) => (probe = r))
      .catch(() => (probe = null));
  });

  function show(mods: string[]) {
    if (mods[0]) app.selection = mods[0];
  }

  const files = (h: Finding) => ('files' in h.kind ? h.kind.files : []);

  type Group = 'load' | 'java' | 'order' | 'lua' | 'files' | 'game';
  const GROUPS: Group[] = ['load', 'java', 'order', 'lua', 'files', 'game'];
  function groupOf(h: Finding): Group {
    const k = h.kind.kind;
    if (k.startsWith('Zb') || k === 'JavaPatchOverlap' || k === 'JavaLinkBroken') return 'java';
    if (k === 'Cycle' || k === 'Incompatible' || k === 'DuplicatedId') return 'order';
    if (['OverriddenFiles', 'OverridesGame', 'ScriptRedefined', 'TiledefRepeated', 'MapCellsShared', 'MapOverTown'].includes(k)) return 'files';
    if (k === 'ModifiedInstall') return 'game';
    if (k === 'LuaGlobalReplaced') return 'lua';
    return 'load';
  }
  const groups = $derived(
    GROUPS.map((g) => [g, list.filter((h) => groupOf(h) === g)] as const).filter(([, hs]) => hs.length > 0),
  );
  const sameSeverity = (hs: readonly Finding[]) => (hs.every((h) => h.severity === hs[0].severity) ? hs[0].severity : null);

  let open = $state<Record<string, boolean>>({});
  const quote = (s: string) => (i18n.language === 'en' ? `“${s}”` : `«${s}»`);
  const overlap = (h: Finding) => h.kind.kind === 'JavaPatchOverlap' && h.kind.replaced.length === 0;
  const many = (h: Finding) => h.kind.kind === 'Incompatible' && h.mods.length > 2;
  const declaredBy = (h: Finding, m: string) => (h.kind.kind === 'Incompatible' ? h.kind.declared.filter(([a]) => a === m).map(([, b]) => b) : []);
  const allDeclare = (h: Finding) => h.kind.kind === 'Incompatible' && new Set(h.kind.declared.map(([a, b]) => `${a}\n${b}`)).size === h.mods.length * (h.mods.length - 1);
  const keepOf = (h: Finding, m: string) => h.fixes.find((f) => f.action === 'Keep' && f.data[0] === m);
  function text(h: Finding): string {
    if (overlap(h)) return t('conflict.patch_overlap');
    if (many(h)) return t('conflict.incompat_many', { n: h.mods.length }) + (allDeclare(h) ? ' ' + t('conflict.incompat_all') : '');
    return findingText(h);
  }
  const named = (h: Finding) => many(h) || h.mods.every((m) => text(h).includes(quote(m)));
  const fixKey = (a: Fix) => JSON.stringify(a);
</script>

{#snippet icon(s: Severity)}
  {@const Icon = icons[s]}
  <span class="icon {classes[s]}"><Icon size={15} strokeWidth={2} /><span class="sr-only">{t(`sev.${sevKey[s]}`)}</span></span>
{/snippet}

{#snippet fixButtons(fixes: Fix[])}
  {#if fixes.length}
    <div class="fixes">
      {#each fixes as a (fixKey(a))}
        {#if a.action === 'RemoveCopy'}
          <Confirm text={t('conflict.fix.RemoveCopy_confirm', { d: a.data })} yes={fixText(a)} onsi={() => app.fix(a)}>
            {#snippet button(request)}
              <button class="btn danger" onclick={request} disabled={app.taken}>{fixText(a)}</button>
            {/snippet}
          </Confirm>
        {:else}
          <button class="btn" onclick={() => app.fix(a)} disabled={app.taken} title={fixHelp(a)}>{fixText(a)}</button>
        {/if}
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet entry(e: ModEntry, withIcon: boolean)}
  <li class="row" class:sel={app.selection === e.mod}>
    {#if withIcon}{@render icon(e.severity)}{:else}<span></span>{/if}
    <button class="link line" onclick={() => show([e.mod])} title={e.findings.map(findingText).join('\n')}>
      <span class="mod mono">{e.mod}</span>
      <span class="why">{e.findings.map(whyText).join(' · ')}</span>
    </button>
    {@render fixButtons(e.fixes)}
  </li>
{/snippet}

{#snippet cluster(c: Cluster, withIcon: boolean)}
  {@const expanded = open[c.key + c.severity] ?? false}
  {@const n = c.entries.length}
  <li class="cluster" class:sel={c.entries.some((e) => e.mod === app.selection)}>
    <div class="row head">
      {#if withIcon}{@render icon(c.severity)}{:else}<span></span>{/if}
      <button class="link line" aria-expanded={expanded} onclick={() => (open[c.key + c.severity] = !expanded)}>
        <ChevronRight class="chevron" size={14} strokeWidth={2} />
        <span class="title">{clusterTitle(c)}</span>
        {#if !expanded}<span class="names mono">{c.entries.map((e) => e.mod).join(' · ')}</span>{/if}
      </button>
      {#if c.disable.length > 1}
        <div class="fixes shown">
          <button class="btn" onclick={() => app.disableMany(c.disable)} disabled={app.taken}>{t('conflict.disable_n', { n: c.disable.length })}</button>
        </div>
      {/if}
    </div>
    {#if expanded}
      <ul class="sub" aria-label={clusterTitle(c)}>
        {#each c.entries as e (e.mod)}{@render entry(e, false)}{/each}
      </ul>
    {:else}
      <span class="sr-only">{t('conflict.show_n', { n })}</span>
    {/if}
  </li>
{/snippet}

<div class="conflicts">
  {#if notices.length === 0 && v.counts.detail === 0}
    <div class="empty"><Check size={18} strokeWidth={2} /><span>{t('conflict.empty')}</span></div>
  {:else}
    <div class="filters" role="radiogroup" aria-label={t('profile.tab.conflicts')}>
      {#each [['all', t('conflict.filter.all'), notices.length], ['Blocks', t('sev.blocks'), v.counts.blocks], ['Warning', t('sev.warning'), v.counts.warning], ['Info', t('sev.info'), v.counts.info], ['Detail', t('sev.detail'), v.counts.detail]] as [id, label, n] (id)}
        <button role="radio" aria-checked={filter === id} class="filter" class:minor={id === 'Info' || id === 'Detail'} onclick={() => (filter = id as typeof filter)}>
          {label} <span class="num">{n}</span>
        </button>
      {/each}
    </div>
    {#if blocked.length > 0 && (filter === 'all' || filter === 'Blocks')}
      <div class="summary" role="status">
        <CircleX size={16} strokeWidth={2} />
        <strong>{t('conflict.summary', { n: blocked.length })}</strong>
        <div class="actions">
          {#if filter === 'all'}
            <button class="btn subtle" onclick={() => (filter = 'Blocks')}>{t('conflict.only_blocks')}</button>
          {/if}
          <Confirm text={t('conflict.disable_all_confirm', { n: blocked.length })} yes={t('conflict.disable_all')} onsi={() => app.disableMany(blocked)}>
            {#snippet button(request)}
              <button class="btn" onclick={request} disabled={app.taken}>{t('conflict.disable_all')}</button>
            {/snippet}
          </Confirm>
        </div>
      </div>
    {/if}
    {#if notices.length === 0 && filter === 'all'}
      <div class="empty"><Check size={18} strokeWidth={2} /><span>{t('conflict.only_detail', { n: v.counts.detail })}</span></div>
    {/if}
    {#if probe}
      <p class="probe" class:severity-warning={probe.mismatches.length > 0} role="status">
        {probe.mismatches.length
          ? t('conflict.probe_bad', { b: probe.mismatches.length, n: probe.total, f: probe.mismatches[0].name })
          : t('conflict.probe_ok', { n: probe.total })}
      </p>
    {/if}
    {#each groups as [g, hs] (g)}
      {@const one = sameSeverity(hs)}
      <section class="group">
        <h3>
          {#if one}{@render icon(one)}{/if}
          {t(`conflict.group.${g}`)} <span class="num">{hs.length}</span>
        </h3>
        <ul>
          {#if g === 'load'}
            {#each clusterByReason(hs) as c (c.id)}
              {#if c.entries.length > 1}{@render cluster(c, !one)}{:else}{@render entry(c.entries[0], !one)}{/if}
            {/each}
          {:else}
            {#each hs as h, i (i)}
              <li class="row" class:sel={h.mods.includes(app.selection ?? '')}>
                {#if !one}{@render icon(h.severity)}{:else}<span></span>{/if}
                <div class="text">
                  {#if h.mods.length && !named(h)}<div class="who mono">{h.mods.join(' · ')}</div>{/if}
                  <button class="link" onclick={() => show(h.mods)}>{text(h)}</button>
                  {#if h.kind.kind === 'OverriddenFiles'}
                    {#each h.kind.pairs as p, j (j)}
                      <div class="pair" class:dim={!p.total || p.related || p.accepted}>{pairText(p)}</div>
                      {#if p.files.length}<div class="files mono">{p.files.join(' · ')}</div>{/if}
                      {#if p.lost_objects.length}<div class="files mono">{p.lost_objects.join(' · ')}{p.total_lost > p.lost_objects.length ? ' · …' : ''}</div>{/if}
                    {/each}
                  {:else if h.kind.kind === 'LuaGlobalReplaced'}
                    {#each h.kind.functions as f, j (j)}
                      <div class="pair">{luaText(f, h.kind.wins)}</div>
                    {/each}
                    {#if h.kind.total > h.kind.functions.length}<div class="pair dim">{t('conflict.lua_more', { n: h.kind.total - h.kind.functions.length })}</div>{/if}
                  {:else if h.kind.kind === 'JavaLinkBroken'}
                    <div class="files mono">{h.kind.missing.join(' · ')}{h.kind.total > h.kind.missing.length ? ' · …' : ''}</div>
                  {:else if h.kind.kind === 'JavaPatchOverlap'}
                    <div class="files mono" title={h.kind.methods.join('\n')}>{h.kind.methods.join(' · ')}</div>
                  {:else if many(h)}
                    <ul class="keep">
                      {#each h.mods as m (m)}
                        {@const keep = keepOf(h, m)}
                        {@const declared = allDeclare(h) ? [] : declaredBy(h, m)}
                        <li>
                          <span class="mod mono">{m}</span>
                          {#if declared.length}<span class="dim">{t('conflict.declares_many', { b: declared.map(quote).join(', ') })}</span>{/if}
                          {#if keep}
                            <button class="btn" onclick={() => app.fix(keep)} disabled={app.taken} title={fixHelp(keep)}>{t('conflict.keep_this')}</button>
                          {/if}
                        </li>
                      {/each}
                    </ul>
                  {:else if files(h).length}
                    <div class="files mono">{files(h).join(' · ')}</div>
                  {/if}
                </div>
                {@render fixButtons(many(h) ? [] : h.fixes)}
              </li>
            {/each}
          {/if}
        </ul>
        {#if hs.some(overlap)}<p class="note">{t('conflict.patch_note')}</p>{/if}
      </section>
    {/each}
  {/if}
</div>

<style>
  .probe {
    margin: 0;
    padding: 8px 16px;
    font-size: var(--t-s);
    color: var(--ink-2);
  }
  .conflicts {
    container-type: inline-size;
    flex: 1;
    min-height: 0;
    overflow: auto;
    background: var(--surface);
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 24px 16px;
    color: var(--ok);
  }
  .filters {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    gap: 4px;
    padding: 8px 16px;
    background: var(--ground);
    border-bottom: 1px solid var(--line);
  }
  .filter {
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio-pill);
    background: var(--surface);
    font-size: var(--t-s);
    cursor: pointer;
  }
  .filter.minor {
    border-color: var(--line);
    background: transparent;
    color: var(--ink-2);
  }
  .filter.minor:first-of-type,
  .filter:not(.minor) + .filter.minor {
    margin-left: 12px;
  }
  .filter .num {
    color: var(--ink-3);
    margin-left: 2px;
  }
  .filter[aria-checked='true'] {
    background: var(--ink);
    border-color: var(--ink);
    color: var(--surface);
  }
  .filter[aria-checked='true'] .num {
    color: inherit;
    opacity: 0.75;
  }

  .summary {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    padding: 10px 16px;
    background: var(--danger-soft);
    border-bottom: 1px solid var(--line);
    color: var(--danger);
  }
  .summary strong {
    color: var(--ink);
    font-weight: 600;
  }
  .summary .actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .group h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    padding: 16px 16px 6px;
    border-bottom: 1px solid var(--line);
    color: var(--ink-2);
    font-size: var(--t-xs);
    font-weight: 600;
  }
  .group h3 .num {
    color: var(--ink-3);
    font-weight: 400;
  }
  .group h3 .icon {
    padding: 0;
  }
  .note {
    margin: 0;
    padding: 8px 16px 4px 46px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }

  .row {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto;
    gap: 8px;
    align-items: start;
    min-height: 42px;
    padding: 7px 16px;
    border-bottom: 1px solid var(--line);
  }
  .row.sel,
  .cluster.sel > .head {
    background: var(--accent-soft);
  }
  .cluster {
    border-bottom: 1px solid var(--line);
  }
  .cluster > .head {
    border-bottom: 0;
  }
  .sub {
    padding-bottom: 4px;
  }
  .sub .row {
    padding-left: 46px;
    border-bottom: 0;
  }
  .icon {
    padding-top: 5px;
    display: inline-flex;
  }
  .text {
    min-width: 0;
    padding-top: 4px;
  }
  .who {
    margin-bottom: 2px;
    color: var(--ink-2);
    font-size: var(--t-xs);
    overflow-wrap: anywhere;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    text-align: left;
    line-height: 1.45;
    cursor: pointer;
  }
  .link:hover .why,
  .link:hover .title,
  .text > .link:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .line {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
    padding-top: 4px;
  }
  .mod {
    flex: none;
    color: var(--ink);
    font-size: var(--t-s);
  }
  .why {
    min-width: 0;
    color: var(--ink-2);
  }
  .title {
    flex: none;
    font-weight: 600;
  }
  .names {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .line :global(.chevron) {
    flex: none;
    align-self: center;
    color: var(--ink-3);
    transition: transform var(--dur) var(--ease);
  }
  .line[aria-expanded='true'] :global(.chevron) {
    transform: rotate(90deg);
  }
  .keep {
    margin-top: 4px;
  }
  .keep li {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 10px;
    min-height: 30px;
    font-size: var(--t-s);
  }
  .keep .dim {
    color: var(--ink-3);
  }
  .keep .btn {
    height: 24px;
    margin-left: auto;
    display: none;
  }
  .keep li:hover .btn,
  .row:focus-within .keep .btn,
  .row.sel .keep .btn {
    display: inline-flex;
  }
  @media (hover: none) {
    .keep .btn {
      display: inline-flex;
    }
  }
  .pair {
    margin-top: 4px;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .pair.dim {
    color: var(--ink-3);
  }
  .files {
    margin-top: 3px;
    color: var(--ink-3);
    font-size: var(--t-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 70ch;
  }
  .fixes {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
    max-width: 420px;
    display: none;
  }
  .row:hover > .fixes,
  .row:focus-within > .fixes,
  .row.sel > .fixes,
  .fixes.shown {
    display: flex;
  }
  @media (hover: none) {
    .fixes {
      display: flex;
    }
  }
  .fixes .btn {
    height: auto;
    min-height: 26px;
    padding-block: 3px;
    white-space: normal;
    text-align: left;
  }
  @container (max-width: 620px) {
    .row {
      grid-template-columns: 22px minmax(0, 1fr);
    }
    .row > .fixes {
      grid-column: 2;
      justify-content: flex-start;
      max-width: none;
    }
    .line {
      flex-wrap: wrap;
      row-gap: 0;
    }
    .names {
      flex-basis: 100%;
    }
  }
</style>
