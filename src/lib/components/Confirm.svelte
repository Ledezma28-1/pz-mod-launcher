<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { t } from '../i18n.svelte';

  let {
    text,
    yes,
    onsi,
    button,
  }: { text: string; yes: string; onsi: () => unknown; button: Snippet<[(e?: Event) => void]> } = $props();

  let open = $state(false);
  let anchor: HTMLSpanElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();
  let cancel: HTMLButtonElement | undefined = $state();
  let pos = $state({ top: 0, left: 0, up: false });

  const WIDTH = 300;
  let opener: HTMLElement | null = null;
  function giveBack() {
    const o = opener;
    opener = null;
    if (o?.isConnected) queueMicrotask(() => o.focus());
  }

  async function request(e?: Event) {
    e?.preventDefault();
    e?.stopPropagation();
    if (!anchor) return;
    const z = Number(document.documentElement.style.zoom) || 1;
    const b = anchor.getBoundingClientRect();
    const r = { top: b.top / z, bottom: b.bottom / z, right: b.right / z };
    const [an, al] = [innerWidth / z, innerHeight / z];
    const left = Math.max(8, Math.min(r.right - WIDTH, an - WIDTH - 8));
    const up = r.bottom + 150 > al;
    pos = { top: up ? r.top - 4 : r.bottom + 4, left, up };
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    open = true;
    await tick();
    cancel?.focus();
  }

  function close() {
    open = false;
    giveBack();
  }

  function accept() {
    open = false;
    giveBack();
    onsi();
  }

  function toBody(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function isolate(e: Event) {
    e.stopPropagation();
    if (!(e.target instanceof HTMLButtonElement)) e.preventDefault();
  }
</script>

<svelte:window
  onpointerdown={(e) => {
    if (open && box && !box.contains(e.target as Node) && !anchor?.contains(e.target as Node)) close();
  }}
  onkeydown={(e) => {
    if (open && e.key === 'Escape') close();
  }}
  onscrollcapture={(e) => {
    if (open && box && !box.contains(e.target as Node)) close();
  }}
  onresize={() => open && close()}
/>

<span class="anchor" bind:this={anchor}>{@render button(request)}</span>
{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="box"
    class:up={pos.up}
    role="alertdialog"
    aria-label={text}
    tabindex="-1"
    bind:this={box}
    use:toBody
    style:top="{pos.top}px"
    style:left="{pos.left}px"
    style:width="{WIDTH}px"
    onclick={isolate}
  >
    <p>{text}</p>
    <div class="actions">
      <button class="btn" type="button" bind:this={cancel} onclick={close}>{t('common.cancel')}</button>
      <button class="btn danger strong" type="button" onclick={accept}>{yes}</button>
    </div>
  </div>
{/if}

<style>
  .anchor {
    display: inline-flex;
  }
  .box {
    position: fixed;
    z-index: 50;
    padding: 10px 12px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radio);
    background: var(--surface);
    color: var(--ink);
    box-shadow: var(--shadow-menu);
    font-size: var(--t-s);
    white-space: normal;
    text-align: left;
    cursor: default;
  }
  .box.up {
    transform: translateY(-100%);
  }
  p {
    margin: 0 0 10px;
    line-height: 1.45;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
  .strong {
    border-color: color-mix(in srgb, var(--danger) 45%, var(--line-strong));
  }
</style>
