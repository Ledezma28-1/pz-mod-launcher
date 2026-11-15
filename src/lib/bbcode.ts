
export type Style = 'b' | 'i' | 'u' | 'strike' | 'h1' | 'h2' | 'h3' | 'spoiler';

export type Node =
  | { t: 'text'; v: string }
  | { t: 'br' }
  | { t: 'hr' }
  | { t: 'img'; src: string }
  | { t: 'video'; id: string }
  | { t: 'link'; href: string; children: Node[] }
  | { t: 'cita'; author: string | null; children: Node[] }
  | { t: 'code'; children: Node[] }
  | { t: 'list'; sorted: boolean; items: Node[][] }
  | { t: 'table'; rows: { header: boolean; cells: Node[][] }[] }
  | { t: 'estilo'; e: Style; children: Node[] };

const STYLES = new Set<string>(['b', 'i', 'u', 'strike', 'h1', 'h2', 'h3', 'spoiler']);
const CONTAINERS = new Set<string>([...STYLES, 'url', 'quote', 'code', 'list', 'olist', 'table', 'tr', 'th', 'td', '*']);
const LITERALS = new Set<string>(['img', 'noparse', 'previewyoutube']);

export function safeLink(u: string): string | null {
  const v = u.trim();
  try {
    const url = new URL(v.startsWith('www.') ? `https://${v}` : v);
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : null;
  } catch {
    return null;
  }
}

export function imageUrl(u: string): string | null {
  try {
    const url = new URL(u.trim());
    if (url.protocol === 'http:') url.protocol = 'https:';
    return url.protocol === 'https:' ? url.href : null;
  } catch {
    return null;
  }
}


type Raw = string | { tag: string; arg: string | null; children: Raw[] };

function fromEnd<T>(v: T[], f: (x: T) => boolean): number {
  for (let k = v.length - 1; k >= 0; k--) if (f(v[k])) return k;
  return -1;
}

function tree(s: string): Raw[] {
  const root = { tag: '', arg: null as string | null, children: [] as Raw[] };
  const stack: { tag: string; arg: string | null; children: Raw[] }[] = [root];
  const up = () => stack[stack.length - 1];
  const re = /\[(\/?)([a-z0-9*]+)(?:=([^\]]*))?\]/gi;
  let last = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s))) {
    if (m.index > last) up().children.push(s.slice(last, m.index));
    last = re.lastIndex;
    const closes = m[1] === '/';
    const tag = m[2].toLowerCase();
    const arg = m[3]?.replace(/^["']|["']$/g, '') ?? null;
    if (!closes && LITERALS.has(tag)) {
      const end = s.toLowerCase().indexOf(`[/${tag}]`, last);
      const content = s.slice(last, end < 0 ? s.length : end);
      up().children.push({ tag, arg, children: [content] });
      last = end < 0 ? s.length : end + tag.length + 3;
      re.lastIndex = last;
    } else if (!closes && tag === 'hr') {
      up().children.push({ tag, arg, children: [] });
    } else if (!closes && tag === '*') {
      const k = fromEnd(stack, (n) => n.tag === 'list' || n.tag === 'olist' || n.tag === '*');
      if (k > 0 && stack[k].tag === '*') stack.length = k;
      const node = { tag, arg, children: [] as Raw[] };
      up().children.push(node);
      stack.push(node);
    } else if (!closes && CONTAINERS.has(tag)) {
      const node = { tag, arg, children: [] as Raw[] };
      up().children.push(node);
      stack.push(node);
    } else if (closes && stack.some((n, k) => k > 0 && n.tag === tag)) {
      stack.length = fromEnd(stack, (n) => n.tag === tag);
    } else {
      up().children.push(m[0]);
    }
  }
  if (last < s.length) up().children.push(s.slice(last));
  return root.children;
}


function plainText(c: Raw[]): string {
  return c.map((x) => (typeof x === 'string' ? x : plainText(x.children))).join('');
}

function convert(list: Raw[]): Node[] {
  const out: Node[] = [];
  for (const c of list) {
    if (typeof c === 'string') {
      c.split(/\r?\n/).forEach((p, i) => {
        if (i > 0) out.push({ t: 'br' });
        if (p) out.push({ t: 'text', v: p });
      });
      continue;
    }
    const content = typeof c.children[0] === 'string' ? (c.children[0] as string) : '';
    switch (c.tag) {
      case 'hr':
        out.push({ t: 'hr' });
        break;
      case 'img': {
        const src = imageUrl(content);
        if (src) out.push({ t: 'img', src });
        else if (content.trim()) out.push({ t: 'text', v: content.trim() });
        break;
      }
      case 'previewyoutube': {
        const id = (c.arg ?? content).split(';')[0].trim();
        if (/^[\w-]{6,20}$/.test(id)) out.push({ t: 'video', id });
        break;
      }
      case 'code':
        out.push({ t: 'code', children: trim(convert(c.children)) });
        break;
      case 'noparse':
        out.push(...convert([content]));
        break;
      case 'url': {
        const children = convert(c.children);
        const href = safeLink(c.arg ?? plainText(c.children));
        if (href) out.push({ t: 'link', href, children: children.length ? children : [{ t: 'text', v: href }] });
        else out.push(...children);
        break;
      }
      case 'quote':
        out.push({ t: 'cita', author: c.arg ? c.arg.split(';')[0] : null, children: trim(convert(c.children)) });
        break;
      case 'list':
      case 'olist': {
        const items = c.children
          .filter((h): h is Exclude<Raw, string> => typeof h !== 'string' && h.tag === '*')
          .map((h) => trim(convert(h.children)));
        out.push({ t: 'list', sorted: c.tag === 'olist', items });
        break;
      }
      case 'table': {
        const rows = c.children
          .filter((h): h is Exclude<Raw, string> => typeof h !== 'string' && h.tag === 'tr')
          .map((tr) => {
            const cells = tr.children.filter((h): h is Exclude<Raw, string> => typeof h !== 'string' && (h.tag === 'td' || h.tag === 'th'));
            return { header: cells.some((x) => x.tag === 'th'), cells: cells.map((x) => trim(convert(x.children))) };
          })
          .filter((f) => f.cells.length);
        out.push({ t: 'table', rows });
        break;
      }
      default:
        if (STYLES.has(c.tag)) {
          const children = convert(c.children);
          out.push({ t: 'estilo', e: c.tag as Style, children: c.tag.startsWith('h') ? trim(children) : children });
        } else out.push(...convert(c.children));
    }
  }
  return out;
}

function trim(n: Node[]): Node[] {
  const r = [...n];
  while (r[0]?.t === 'br') r.shift();
  while (r[r.length - 1]?.t === 'br') r.pop();
  return r;
}

function compact(n: Node[]): Node[] {
  const block = (x: Node | undefined) =>
    !!x && (x.t === 'list' || x.t === 'table' || x.t === 'cita' || x.t === 'code' || x.t === 'hr' || (x.t === 'estilo' && x.e.startsWith('h')));
  const out: Node[] = [];
  for (let k = 0; k < n.length; k++) {
    const x = n[k];
    if (x.t === 'br') {
      if (block(out[out.length - 1]) || block(n[k + 1])) continue;
      let consecutive = 0;
      for (let j = out.length - 1; j >= 0 && out[j].t === 'br'; j--) consecutive++;
      if (consecutive >= 2) continue;
    }
    out.push(x);
  }
  return trim(out);
}

export function parse(s: string): Node[] {
  return compact(convert(tree(s)));
}
