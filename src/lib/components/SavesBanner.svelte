<script lang="ts">
  import Save from '@lucide/svelte/icons/save';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import { ago } from '../time';

  const gaps = $derived(app.saveGaps);
  const id = $derived(app.profileId!);
  let open = $state(false);
  let skipped = $state<Set<string>>(new Set());
  const allMissing = $derived([...new Set(gaps.flatMap((g) => g.missing))]);
  const picked = $derived(gaps.filter((g) => !skipped.has(g.dir)).map((g) => g.dir));
  const withMap = $derived([...new Set(gaps.filter((g) => !skipped.has(g.dir)).flatMap((g) => g.with_map))]);
  const mods = (n: number) => (n === 1 ? t('saves.mods_one') : t('saves.mods', { n }));

  function toggle(dir: string) {
    const s = new Set(skipped);
    if (s.has(dir)) s.delete(dir);
    else s.add(dir);
    skipped = s;
  }

  async function add() {
    const dirs = picked;
    try {
      const n = await api.savesAdd(id, dirs);
      app.notify(t('saves.done', { n }), 'ok', () => api.savesRestore(id, dirs).then(() => app.loadSaveGaps()));
    } catch (e) {
      app.notify(message(e), 'error');
    }
    await app.loadSaveGaps();
  }

  async function ignore() {
    try {
      await api.savesIgnore(id, picked);
      app.notify(t('saves.ignored'));
    } catch (e) {
      app.notify(message(e), 'error');
    }
    await app.loadSaveGaps();
  }
</script>

<section class="banner" aria-label={t('saves.title')}>
  <div class="head">
    <Save size={15} strokeWidth={2} />
    <p>
      <strong>{t('saves.title')}</strong>
      {gaps.length === 1
        ? t('saves.summary_one', { p: gaps[0].name, m: mods(allMissing.length) })
        : t('saves.summary', { s: t('saves.n', { n: gaps.length }), m: mods(allMissing.length) })}
    </p>
    <div class="actions">
      <button class="btn subtle" aria-expanded={open} onclick={() => (open = !open)}>
        {#if open}<ChevronDown size={14} strokeWidth={2} />{:else}<ChevronRight size={14} strokeWidth={2} />{/if}{t('saves.choose')}
      </button>
      <button class="btn" onclick={add} disabled={app.taken || picked.length === 0} title={t('saves.add_help')}>{t('saves.add')}</button>
      <button class="btn subtle" onclick={ignore} disabled={app.taken || picked.length === 0} title={t('saves.ignore_help')}>{t('saves.ignore')}</button>
    </div>
  </div>
  {#if open}
    <ul class="list">
      {#each gaps as g (g.dir)}
        <li>
          <label>
            <input type="checkbox" checked={!skipped.has(g.dir)} onchange={() => toggle(g.dir)} />
            <span class="name">{g.name}</span>
            <span class="meta">{g.mode} · {ago(g.modified)}</span>
          </label>
          <span class="ids mono" title={g.missing.join('\n')}>{t('saves.missing', { m: g.missing.slice(0, 4).join(', ') + (g.missing.length > 4 ? '…' : '') })}</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#if withMap.length}<p class="note">{t('saves.map_note', { m: withMap.join(', ') })}</p>{/if}
</section>

<style>
  .banner {
    margin: 10px 16px 0;
    padding: 7px 10px;
    border: 1px solid color-mix(in srgb, var(--warn) 45%, var(--line));
    border-radius: var(--radio);
    background: var(--warn-soft);
    color: var(--ink);
    font-size: var(--t-s);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head > :global(svg) {
    flex: none;
    color: var(--warn);
  }
  p {
    flex: 1;
    min-width: 0;
    margin: 0;
  }
  strong {
    font-weight: 600;
  }
  .actions {
    display: flex;
    flex: none;
    gap: 6px;
  }
  .list {
    margin: 8px 0 2px 23px;
    padding: 0;
    list-style: none;
  }
  .list li {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-height: 24px;
  }
  .list label {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .list input {
    margin: 0;
    accent-color: var(--accent);
  }
  .name {
    font-weight: 600;
  }
  .meta,
  .ids {
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
  .ids {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .note {
    margin: 6px 0 0 23px;
    color: var(--ink-2);
    font-size: var(--t-xs);
  }
</style>
