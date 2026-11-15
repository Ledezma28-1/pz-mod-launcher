
const channel = (c: number) => {
  const v = c / 255;
  return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
};
const rgb = (hex: string) =>
  [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const luminance = (hex: string) => {
  const [r, g, b] = rgb(hex).map(channel);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

export function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

export function deltaE(a: string, b: string): number {
  const lab = (hex: string) => {
    const [r, g, b] = rgb(hex).map(channel);
    const f = (t: number) =>
      t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116;
    const x = f((0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047);
    const y = f(0.2126 * r + 0.7152 * g + 0.0722 * b);
    const z = f((0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883);
    return [116 * y - 16, 500 * (x - y), 200 * (y - z)];
  };
  const [p, q] = [lab(a), lab(b)];
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]);
}

const tokens = (body: string) =>
  Object.fromEntries(
    [...body.matchAll(/(--[\w-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)].map((m) => [
      m[1],
      m[2].toLowerCase(),
    ]),
  );

export function parsePalettes(
  css: string,
): Record<string, Record<string, string>> {
  const block = (selector: string) => {
    const i = css.indexOf(selector + ' {');
    if (i < 0) return null;
    return tokens(css.slice(i, css.indexOf('}', i)));
  };
  const light = block(':root') ?? {};
  const dark = { ...light, ...(block(`:root[data-mode='dark']`) ?? {}) };
  const out: Record<string, Record<string, string>> = {
    'launcher-light': light,
    'launcher-dark': dark,
  };
  for (const m of css.matchAll(
    /:root\[data-palette='([\w-]+)'\](\[data-mode='dark'\])? \{/g,
  )) {
    const id = m[1];
    if (out[`${id}-light`]) continue;
    const own = block(`:root[data-palette='${id}']`) ?? {};
    out[`${id}-light`] = { ...light, ...own };
    out[`${id}-dark`] = {
      ...dark,
      ...own,
      ...(block(`:root[data-palette='${id}'][data-mode='dark']`) ?? {}),
    };
  }
  return out;
}
