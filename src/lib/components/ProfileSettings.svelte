<script lang="ts">
  import Confirm from './Confirm.svelte';
  import { api, message } from '../api';
  import { app } from '../state.svelte';
  import { t } from '../i18n.svelte';
  import type { ProfileSettings } from '../types';

  const v = $derived(app.view!);
  let data = $state<ProfileSettings>({ ...app.view!.settings });
  let name = $state(app.view!.name);
  let saved = $state(false);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    try {
      if (name.trim() && name.trim() !== v.name) {
        await api.renameProfile(v.id, name.trim());
        await app.loadProfiles();
      }
      await app.run(() => api.settings(v.id, data));
      saved = true;
      setTimeout(() => (saved = false), 2000);
    } catch (err) {
      app.notify(message(err), 'error');
    }
  }

  async function deleteProfile() {
    try {
      await api.deleteProfile(v.id);
      app.profileId = null;
      app.view = null;
      await app.loadProfiles();
    } catch (err) {
      app.notify(message(err), 'error');
    }
  }
</script>

<form class="settings" onsubmit={save}>
  <label>
    <span class="tag">{t('profile.rename')}</span>
    <input class="field" bind:value={name} />
  </label>
  <label>
    <span class="tag">{t('settings.memory')}</span>
    <input class="field mono short" bind:value={data.xmx} placeholder="8g" />
    <span class="help">{t('settings.memory_help')}</span>
  </label>
  <label>
    <span class="tag">{t('settings.extra')}</span>
    <input class="field mono" bind:value={data.game_extra} />
    <span class="help">{t('settings.extra_help')}</span>
  </label>
  <label>
    <span class="tag">{t('settings.zb_use')}</span>
    <select class="field" bind:value={data.use_zb}>
      <option value="auto">{t('settings.zb_auto')}</option>
      <option value="si">{t('settings.zb_yes')}</option>
      <option value="no">{t('settings.zb_no')}</option>
    </select>
  </label>
  <label>
    <span class="tag">{t('settings.policy')}</span>
    <select class="field" bind:value={data.policy}>
      <option value="prompt">{t('settings.prompt')}</option>
      <option value="deny-new">{t('settings.deny-new')}</option>
      <option value="allow-all">{t('settings.allow-all')}</option>
    </select>
  </label>
  <label>
    <span class="tag">{t('settings.probe_tag')}</span>
    <span class="check"><input type="checkbox" bind:checked={data.probe} />{t('settings.probe')}</span>
    <span class="help">{t('settings.probe_help')}</span>
  </label>
  <div class="buttons">
    <button class="btn primary" type="submit" disabled={app.taken}>{t('settings.save')}</button>
    {#if saved}<span class="ok" role="status">{t('settings.saved')}</span>{/if}
    <button class="btn subtle right" type="button" onclick={() => app.duplicateProfile(v.id, v.name)}>{t('profile.duplicate')}</button>
    <Confirm text={t('profile.delete_confirm', { n: v.name })} yes={t('profile.delete')} onsi={deleteProfile}>
      {#snippet button(request)}
        <button class="btn danger subtle" type="button" onclick={request}>{t('profile.delete')}</button>
      {/snippet}
    </Confirm>
  </div>
</form>

<style>
  .settings {
    flex: 1;
    overflow: auto;
    display: grid;
    align-content: start;
    gap: 14px;
    padding: 18px 16px;
    max-width: 620px;
  }
  label {
    display: grid;
    grid-template-columns: 220px 1fr;
    align-items: center;
    gap: 4px 14px;
  }
  .tag {
    color: var(--ink-2);
    font-size: var(--t-s);
  }
  .field {
    width: 100%;
  }
  .field.short {
    width: 120px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .help {
    grid-column: 2;
    color: var(--ink-3);
    font-size: var(--t-xs);
  }
  .buttons {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 6px;
  }
  .ok {
    color: var(--ok);
    font-size: var(--t-s);
  }
  .right {
    margin-left: auto;
  }
</style>
