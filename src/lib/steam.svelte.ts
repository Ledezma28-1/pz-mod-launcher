
import { api, message } from './api';
import type { WorkshopCollection, WorkshopQuery, Featured, WorkshopDetails, WorkshopPage } from './types';

export const PAGE_TTL = 600_000;
export const pageCache = new Map<string, { p: WorkshopPage; when: number }>();
const inProgress = new Map<string, Promise<WorkshopPage>>();
export const key = (c: WorkshopQuery) => JSON.stringify(c);

export function fetchPage(c: WorkshopQuery): Promise<WorkshopPage> {
  const k = key(c);
  let pr = inProgress.get(k);
  if (!pr) {
    pr = api.workshopSearch(c).then((p) => {
      pageCache.set(k, { p, when: Date.now() });
      for (const e of p.items) {
        const f = detailsCache.get(e.id);
        if (f && f.card.updated !== e.updated) detailsCache.delete(e.id);
        const col = collectionCache.get(e.id);
        if (col && col.collection.updated !== e.updated) collectionCache.delete(e.id);
      }
      saveSoon();
      return p;
    });
    pr.finally(() => inProgress.delete(k)).catch(() => {});
    inProgress.set(k, pr);
  }
  return pr;
}

export const detailsCache = new Map<number, { card: WorkshopDetails; requirements: WorkshopDetails[] }>();
export const collectionCache = new Map<number, WorkshopCollection>();
const checked = new Map<number, number>();

export async function revalidate(id: number): Promise<boolean> {
  if (Date.now() - (checked.get(id) ?? 0) < PAGE_TTL) return false;
  checked.set(id, Date.now());
  const saved = detailsCache.get(id)?.card.updated ?? collectionCache.get(id)?.collection.updated;
  try {
    const [r] = await api.workshopCheck([id]);
    if (!r || saved === undefined || r[1] === saved) return false;
    detailsCache.delete(id);
    collectionCache.delete(id);
    saveSoon();
    return true;
  } catch {
    return false;
  }
}

export function recentlyFetched(id: number) {
  checked.set(id, Date.now());
  saveSoon();
}

export function image(url: string, updated: number, width?: number): string {
  if (!url) return url;
  const q = [updated ? `t=${updated}` : ''];
  if (width && url.startsWith('https://images.steamusercontent.com/')) q.push(`imw=${width}&imh=${width}&ima=fit&impolicy=Letterbox&letterbox=false`);
  const extra = q.filter(Boolean).join('&');
  return extra ? `${url}${url.includes('?') ? '&' : '?'}${extra}` : url;
}

const MAX_PAGES = 40;
const MAX_CARDS = 300;
const MAX_COLLECTIONS = 60;
type Saved = {
  version: 2;
  pages: [string, { p: WorkshopPage; when: number }][];
  cards: [number, { card: WorkshopDetails; requirements: WorkshopDetails[] }][];
  collections: [number, WorkshopCollection][];
  checked: [number, number][];
};
const last = <K, V>(m: Map<K, V>, n: number) => [...m.entries()].slice(-n);
function trim<K, V>(m: Map<K, V>, n: number) {
  for (const k of m.keys()) {
    if (m.size <= n) break;
    m.delete(k);
  }
}

export function touch(id: number) {
  for (const m of [detailsCache, collectionCache] as Map<number, unknown>[]) {
    const v = m.get(id);
    if (v !== undefined) {
      m.delete(id);
      m.set(id, v);
    }
  }
}
let timer: ReturnType<typeof setTimeout> | null = null;
function saveSoon() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    trim(pageCache, MAX_PAGES);
    trim(detailsCache, MAX_CARDS);
    trim(collectionCache, MAX_COLLECTIONS);
    trim(checked, MAX_CARDS + MAX_COLLECTIONS);
    const g: Saved = {
      version: 2,
      pages: last(pageCache, MAX_PAGES),
      cards: last(detailsCache, MAX_CARDS),
      collections: last(collectionCache, MAX_COLLECTIONS),
      checked: last(checked, MAX_CARDS + MAX_COLLECTIONS),
    };
    api.workshopCacheWrite(JSON.stringify(g)).catch(() => {});
  }, 2000);
}

async function readSaved() {
  try {
    const t = await api.workshopCacheRead();
    if (!t) return;
    const g = JSON.parse(t) as Saved;
    if (g.version !== 2) return;
    for (const [k, v] of g.pages) if (!pageCache.has(k)) pageCache.set(k, v);
    for (const [k, v] of g.cards) if (!detailsCache.has(k)) detailsCache.set(k, v);
    for (const [k, v] of g.collections) if (!collectionCache.has(k)) collectionCache.set(k, v);
    for (const [k, v] of g.checked) if (!checked.has(k)) checked.set(k, v);
  } catch {
  }
}



class Steam {
  featured = $state<Featured | null>(null);
  reading = $state(true);
  updating = $state(false);
  errorDest = $state<string | null>(null);
  tags = $state<string[]>([]);
  private preloaded = false;

  async readFeatured() {
    try {
      this.featured = await api.featuredRead();
    } catch {
    } finally {
      this.reading = false;
    }
  }

  async updateFeatured() {
    if (this.updating) return;
    this.updating = true;
    this.errorDest = null;
    try {
      this.featured = await api.featuredRefresh();
    } catch (e) {
      this.errorDest = message(e);
    } finally {
      this.updating = false;
    }
  }

  forget() {
    if (timer) clearTimeout(timer);
    timer = null;
    pageCache.clear();
    detailsCache.clear();
    collectionCache.clear();
    checked.clear();
    this.featured = null;
  }

  async preload() {
    if (this.preloaded) return;
    this.preloaded = true;
    api
      .workshopTags()
      .then((e) => (this.tags = e))
      .catch(() => {});
    await Promise.all([this.readFeatured(), readSaved()]);
  }

  async revalidateFeatured() {
    if (this.reading) await this.readFeatured();
    const parsed = (this.featured?.parsed ?? 0) * 1000;
    if (Date.now() - parsed >= PAGE_TTL) await this.updateFeatured();
  }
}

export const steam = new Steam();
