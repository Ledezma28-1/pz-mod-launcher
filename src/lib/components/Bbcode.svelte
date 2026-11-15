<script lang="ts">
  import PlayCircle from '@lucide/svelte/icons/circle-play';
  import { api } from '../api';
  import { parse, type Node } from '../bbcode';
  import { t } from '../i18n.svelte';
  import { still } from '../still';

  let { text }: { text: string } = $props();
  const nodes = $derived(parse(text));
  let seen = $state<Set<string>>(new Set());
  let unmet = $state<Set<string>>(new Set());

  function open(e: MouseEvent, href: string) {
    e.preventDefault();
    api.openUrl(href);
  }
</script>

{#snippet paint(list: Node[], path: string)}
  {#each list as n, k (k)}
    {@const id = `${path}.${k}`}
    {#if n.t === 'text'}{n.v}{:else if n.t === 'br'}<br />{:else if n.t === 'hr'}<hr />{:else if n.t === 'img'}{#if unmet.has(n.src)}<a href={n.src} title={n.src} onclick={(e) => open(e, n.src)}>{t('bb.image')}</a
        >{:else}<img
          src={n.src}
          alt=""
          loading="lazy"
          referrerpolicy="no-referrer"
          use:still
          onerror={() => (unmet = new Set([...unmet, n.src]))}
        />{/if}{:else if n.t === 'video'}<a class="video" href="https://www.youtube.com/watch?v={n.id}" onclick={(e) => open(e, `https://www.youtube.com/watch?v=${n.id}`)}
        ><PlayCircle size={15} strokeWidth={1.75} />{t('bb.video')}</a
      >{:else if n.t === 'link'}<a href={n.href} title={n.href} onclick={(e) => open(e, n.href)}>{@render paint(n.children, id)}</a
      >{:else if n.t === 'code'}<pre>{@render paint(n.children, id)}</pre>{:else if n.t === 'cita'}<blockquote>
        {#if n.author}<cite>{n.author}</cite>{/if}{@render paint(n.children, id)}
      </blockquote>{:else if n.t === 'list'}{#if n.sorted}<ol>
          {#each n.items as el, j (j)}<li>{@render paint(el, `${id}.${j}`)}</li>{/each}
        </ol>{:else}<ul>
          {#each n.items as el, j (j)}<li>{@render paint(el, `${id}.${j}`)}</li>{/each}
        </ul>{/if}{:else if n.t === 'table'}<div class="table">
        <table>
          <tbody>
            {#each n.rows as f, j (j)}
              <tr>
                {#each f.cells as c, x (x)}
                  {#if f.header}<th>{@render paint(c, `${id}.${j}.${x}`)}</th>{:else}<td>{@render paint(c, `${id}.${j}.${x}`)}</td>{/if}
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>{:else if n.e === 'b'}<strong>{@render paint(n.children, id)}</strong>{:else if n.e === 'i'}<em>{@render paint(n.children, id)}</em
      >{:else if n.e === 'u'}<u>{@render paint(n.children, id)}</u>{:else if n.e === 'strike'}<s>{@render paint(n.children, id)}</s
      >{:else if n.e === 'h1'}<h1>{@render paint(n.children, id)}</h1>{:else if n.e === 'h2'}<h2>{@render paint(n.children, id)}</h2
      >{:else if n.e === 'h3'}<h3>{@render paint(n.children, id)}</h3>{:else}<button
        class="spoiler"
        class:seen={seen.has(id)}
        title={seen.has(id) ? undefined : t('bb.spoiler')}
        onclick={() => (seen = new Set([...seen, id]))}>{@render paint(n.children, id)}</button
      >{/if}
  {/each}
{/snippet}

<div class="bb">{@render paint(nodes, 'r')}</div>

<style>
  .bb {
    font-size: inherit;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  .bb :global(h1),
  .bb :global(h2),
  .bb :global(h3) {
    margin: 14px 0 6px;
    line-height: 1.3;
    font-weight: 600;
  }
  .bb :global(h1) {
    font-size: 1.35em;
  }
  .bb :global(h2) {
    font-size: 1.18em;
  }
  .bb :global(h3) {
    font-size: 1.05em;
  }
  .bb :global(h1:first-child),
  .bb :global(h2:first-child),
  .bb :global(h3:first-child) {
    margin-top: 0;
  }
  .bb :global(a) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .bb :global(ul),
  .bb :global(ol) {
    margin: 6px 0;
    padding-left: 22px;
  }
  .bb :global(li) {
    margin: 2px 0;
  }
  .bb :global(blockquote) {
    margin: 8px 0;
    padding: 6px 10px;
    border-left: 1px solid var(--line-strong);
    border-radius: 0 var(--radio) var(--radio) 0;
    background: var(--surface-2);
    color: var(--ink-2);
  }
  .bb :global(cite) {
    display: block;
    font-style: normal;
    font-weight: 600;
    font-size: 0.92em;
  }
  .bb :global(pre) {
    margin: 8px 0;
    padding: 8px 10px;
    overflow-x: auto;
    border: 1px solid var(--line);
    border-radius: var(--radio);
    background: var(--surface-2);
    font-family: var(--mono);
    font-size: 0.9em;
    white-space: pre-wrap;
  }
  .bb :global(hr) {
    margin: 12px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }
  .bb :global(img),
  .bb :global(canvas) {
    display: block;
    max-width: 100%;
    height: auto;
    margin: 8px 0;
    border-radius: var(--radio);
  }
  .bb :global(.table) {
    margin: 8px 0;
    overflow-x: auto;
  }
  .bb :global(table) {
    border-collapse: collapse;
    font-size: 0.95em;
  }
  .bb :global(th),
  .bb :global(td) {
    padding: 4px 8px;
    border: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
  }
  .bb :global(th) {
    background: var(--surface-2);
    font-weight: 600;
  }
  .spoiler {
    display: inline;
    padding: 0 2px;
    border: 0;
    border-radius: var(--radio);
    background: var(--ink);
    color: transparent;
    font: inherit;
    text-align: inherit;
    cursor: pointer;
  }
  .spoiler :global(*) {
    visibility: hidden;
  }
  .spoiler.seen {
    background: var(--surface-2);
    color: inherit;
    cursor: text;
  }
  .spoiler.seen :global(*) {
    visibility: visible;
  }
  .video {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
</style>
