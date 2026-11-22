<script lang="ts">
  import InfoIcon from '@lucide/svelte/icons/info';
  import TriangleAlert from '@lucide/svelte/icons/triangle-alert';
  import Confirm from './Confirm.svelte';
  import ProfileSummary from './ProfileSummary.svelte';
  import Check from '@lucide/svelte/icons/check';
  import CircleX from '@lucide/svelte/icons/circle-x';
  import Minus from '@lucide/svelte/icons/minus';
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import ExternalLink from '@lucide/svelte/icons/external-link';
  import Pin from '@lucide/svelte/icons/pin';
  import PinOff from '@lucide/svelte/icons/pin-off';
  import Trash from '@lucide/svelte/icons/trash';
  import PackageX from '@lucide/svelte/icons/package-x';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { findingText, i18n, t } from '../i18n.svelte';
  import type { Finding, ModDetails } from '../types';

  const v = $derived(app.view!);
  const row = $derived(app.row);
  let detail = $state<ModDetails | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let expanded = $state(false);
  let allPatches = $state(false);
  const PATCHES_SHOWN = 6;

  $effect(() => {
    const key = row?.key;
    detail = null;
    error = null;
    expanded = false;
    allPatches = false;
    if (!key) return;
    loading = true;
    let alive = true;
    api
      .detail(key)
      .then((d) => alive && (detail = d))
      .catch((e) => alive && (error = message(e)))
      .finally(() => alive && (loading = false));
    return () => (alive = false);
  });

  const active = $derived(new Set(v.rows.filter((f) => f.enabled).map((f) => f.id)));
  const installed = $derived(new Set(v.rows.map((f) => f.id)));
  const own = $derived(row ? v.findings.filter((h) => h.mods.includes(row.id)) : []);
  const overlap = (h: Finding) => h.kind.kind === 'JavaPatchOverlap' && h.kind.replaced.length === 0;
  const others = (h: Finding) =>
    h.mods
      .filter((m) => m !== row?.id)
      .map((m) => (i18n.language === 'en' ? `“${m}”` : `«${m}»`))
      .join(', ');
  const range = (a: string | null, b: string | null) => t('common.range', { a: a ?? '…', b: b ?? '…' });

  async function uninstall() {
    if (!detail || !row) return;
    const ws = detail.workshop_id;
    const { id: profile } = v;
    const modId = row.id;
    const key = detail.key;
    await app.run(async () => {
      if (ws) {
        await api.workshopUnsubscribe(ws);
        await api.removeMany(profile, v.rows.filter((f) => f.workshop_id === ws).map((f) => f.id));
      } else {
        await api.modTrash(key);
        await api.remove(profile, modId);
      }
    }, ws ? t('ws.unsubscribed_ok') : t('detail.trash_ok'));
  }
</script>

<aside class="detail" aria-label={t('detail.title')}>
  {#if !row}
    <ProfileSummary />
  {:else}
    <div class="head">
      {#if detail?.poster}
        <img class="poster" src={detail.poster} alt="" />
      {/if}
      <h2>{row.name}</h2>
      <div class="sub mono">{row.id}</div>
      <div class="actions">
        <button class="btn" class:primary={!row.enabled} onclick={() => app.runView(() => api.setEnabled(v.id, row.id, !row.enabled))}>
          {row.enabled ? t('detail.disable') : t('detail.set_enabled')}
        </button>
        <button class="btn subtle" onclick={() => app.run(() => api.pin(v.id, row.id, !row.pinned))} title={t('table.pinned')}>
          {#if row.pinned}<PinOff size={14} strokeWidth={1.75} />{t('detail.unpin')}{:else}<Pin size={14} strokeWidth={1.75} />{t('detail.pin')}{/if}
        </button>
      </div>
    </div>

    {#if own.length}
      <section>
        <h3>{t('detail.conflicts')}</h3>
        <ul class="findings">
          {#each own as h, i (i)}
            {@const Icon = h.severity === 'Blocks' ? CircleX : h.severity === 'Warning' ? TriangleAlert : h.severity === 'Detail' ? Minus : InfoIcon}
            {@const sev = h.severity === 'Blocks' ? 'blocks' : h.severity === 'Warning' ? 'warning' : h.severity === 'Detail' ? 'detail' : 'info'}
            <li class="severity-{sev}">
              <Icon size={14} strokeWidth={2} />
              <span><span class="sr-only">{t(`sev.${sev}`)}: </span>{overlap(h) ? t('detail.patch_overlap', { m: others(h) }) : findingText(h)}</span>
            </li>
          {/each}
        </ul>
        {#if own.some(overlap)}<p class="note">{t('conflict.patch_note')}</p>{/if}
      </section>
    {/if}

    {#if loading}
      <p class="dim">{t('detail.loading')}</p>
    {:else if error}
      <p class="severity-blocks err"><CircleX size={14} strokeWidth={2} />{error}</p>
    {:else if detail}
      {#if detail.description}
        <section>
          <p class="desc" class:short={!expanded}>{detail.description}</p>
          {#if detail.description.length > 220}
            <button class="link" onclick={() => (expanded = !expanded)}>{expanded ? t('detail.less') : t('detail.more')}</button>
          {/if}
        </section>
      {/if}

      <section>
        <dl>
          {#if detail.author}<dt>{t('detail.author')}</dt><dd>{detail.author}</dd>{/if}
          <dt>{t('detail.folder')}</dt>
          <dd class="mono">{detail.folder_version ?? 'common'}</dd>
          <dt>{t('detail.category')}</dt>
          <dd>{t(`cat.${detail.category}` as Parameters<typeof t>[0])}{#if detail.guessed_category}<span class="dim-cat">{' · '}{t('detail.guessed')}</span>{/if}</dd>
          {#if detail.versions.length > 1}
            <dt>{t('detail.versions')}</dt><dd class="mono">{detail.versions.join(', ')}</dd>
          {/if}
          {#if detail.version_min || detail.version_max}
            <dt>{t('detail.game')}</dt><dd class="mono">{range(detail.version_min, detail.version_max)}</dd>
          {/if}
          {#if detail.requires.length}
            <dt>{t('detail.requires')}</dt>
            <dd>
              <ul class="deps">
                {#each detail.requires as r (r)}
                  <li>
                    {#if active.has(r)}
                      <Check size={13} strokeWidth={2} class="ok" /><span>{r}</span>
                    {:else if installed.has(r)}
                      <Minus size={13} strokeWidth={2} class="warning" /><span>{r} <span class="dim">· {t('detail.inactive')}</span></span>
                    {:else}
                      <CircleX size={13} strokeWidth={2} class="bad" /><span>{r} <span class="dim">· {t('detail.missing')}</span></span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </dd>
          {/if}
          {#if detail.incompatible.length}
            <dt>{t('detail.incompatible')}</dt><dd>{detail.incompatible.join(', ')}</dd>
          {/if}
        </dl>
      </section>

      {#if detail.java}
        <section>
          <h3>{t('detail.java')}</h3>
          <dl>
            <dt>{t('detail.jar')}</dt>
            <dd class:mono={!!detail.java.chosen} class:severity-blocks={!detail.java.chosen}>
              {detail.java.chosen ? detail.java.chosen.split('/').pop() : t('detail.jar_none')}
            </dd>
            <dt>{t('detail.range')}</dt>
            <dd class="mono">{detail.java.candidates.map((c) => range(c.min, c.max)).join(' · ')}</dd>
            {#if detail.java.pkg}<dt>{t('detail.package')}</dt><dd class="mono">{detail.java.pkg}</dd>{/if}
            {#if detail.java.chosen}
              {@const patches = detail.java.patches}
              <dt>{t('detail.patches')}</dt>
              <dd>
                {#if patches.length}
                  <ul class="patches">
                    {#each allPatches ? patches : patches.slice(0, PATCHES_SHOWN) as p (`${p.class_name}.${p.method_name}|${p.replaces}`)}
                      {@const full = p.method_name ? `${p.class_name}.${p.method_name}` : p.class_name}
                      <li>
                        <span class="mono" title={full}>{full.slice(p.class_name.lastIndexOf('.') + 1)}</span>{#if p.replaces}<span class="dim"> · {t('detail.patches_replaces')}</span>{/if}
                      </li>
                    {/each}
                  </ul>
                  {#if patches.length > PATCHES_SHOWN && !allPatches}
                    <button class="link" onclick={() => (allPatches = true)}>{t('detail.patches_more', { n: patches.length })}</button>
                  {/if}
                {:else}
                  <span class="dim">{t('detail.patches_none')}</span>
                {/if}
              </dd>
            {/if}
          </dl>
        </section>
      {/if}

      <section class="footer">
        <button class="btn subtle" onclick={() => api.openFolder(detail!.folder).catch((e) => app.notify(message(e), 'error'))}>
          <FolderOpen size={14} strokeWidth={1.75} />{t('detail.folder_open')}
        </button>
        {#if detail.workshop_id}
          <button class="btn subtle" onclick={() => api.openWorkshop(detail!.workshop_id!).catch((e) => app.notify(message(e), 'error'))}>
            <ExternalLink size={14} strokeWidth={1.75} />{t('detail.steam')}
          </button>
        {/if}
        <button class="btn subtle danger" onclick={() => app.run(() => api.remove(v.id, row.id))}>
          <Trash size={14} strokeWidth={1.75} />{t('detail.remove')}
        </button>
        <Confirm
          text={detail.workshop_id ? t('detail.unsubscribe_confirm', { n: row.name }) : t('detail.trash_confirm', { n: row.name })}
          yes={detail.workshop_id ? t('detail.unsubscribe') : t('detail.trash')}
          onsi={uninstall}
        >
          {#snippet button(request)}
            <button class="btn subtle danger" onclick={request}>
              <PackageX size={14} strokeWidth={1.75} />{detail!.workshop_id ? t('detail.unsubscribe') : t('detail.trash')}
            </button>
          {/snippet}
        </Confirm>
      </section>
    {/if}
  {/if}
</aside>

<style>
  .detail {
    height: 100%;
    overflow: auto;
    background: var(--surface);
    border-left: 1px solid var(--line);
  }
  .head {
    padding: 16px 18px 14px;
    border-bottom: 1px solid var(--line);
  }
  .poster {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: var(--radio);
    margin-bottom: 12px;
    background: var(--surface-2);
  }
  h2 {
    font-size: var(--t-l);
    line-height: 1.3;
  }
  .sub {
    margin-top: 2px;
    color: var(--ink-3);
    font-size: var(--t-xs);
    word-break: break-all;
  }
  .actions {
    display: flex;
    gap: 6px;
    margin-top: 12px;
  }
  section {
    padding: 12px 18px;
    border-bottom: 1px solid var(--line);
  }
  h3 {
    margin-bottom: 8px;
    font-size: var(--t-s);
    color: var(--ink-2);
  }
  .findings li {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: 6px;
    align-items: start;
  }
  .findings li :global(svg) {
    margin-top: 2px;
  }
  .err {
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .findings {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: var(--t-s);
  }
  .note {
    margin: 8px 0 0 20px;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .desc {
    margin: 0;
    white-space: pre-line;
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .desc.short {
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .link {
    margin-top: 4px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: var(--t-s);
    cursor: pointer;
  }
  dl {
    display: grid;
    grid-template-columns: 124px 1fr;
    gap: 6px 10px;
    margin: 0;
    font-size: var(--t-s);
  }
  dt {
    color: var(--ink-3);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .deps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }
  .deps li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .deps :global(.ok) {
    color: var(--ok);
  }
  .deps :global(.warning) {
    color: var(--warn);
  }
  .deps :global(.bad) {
    color: var(--danger);
  }
  .dim {
    color: var(--ink-3);
  }
  .patches {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }
  p.dim,
  p.severity-blocks {
    margin: 0;
    padding: 12px 18px;
  }
  .footer {
    display: flex;
    flex-wrap: wrap;
    gap: 2px;
    border-bottom: 0;
  }
  .dim-cat {
    color: var(--ink-3);
  }
</style>
