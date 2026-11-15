
export const PALETTES = ['launcher', 'knox', 'military', 'blackout', 'contrast'] as const;
export type Palette = (typeof PALETTES)[number];

export const SWATCH: Record<Palette, { light: string[]; dark: string[] }> = {
  launcher: { light: ['#222222', '#eeeae0', '#1f4f7a'], dark: ['#121110', '#1a1917', '#7eaad8'] },
  knox: { light: ['#2a2219', '#ede2cc', '#23395b'], dark: ['#14100b', '#1c1712', '#8fb0d8'] },
  military: { light: ['#262a1f', '#e6e6d6', '#2d5170'], dark: ['#111310', '#171a14', '#8aaccb'] },
  blackout: { light: ['#222222', '#eeeae0', '#1f4f7a'], dark: ['#000000', '#000000', '#8ab6e6'] },
  contrast: { light: ['#000000', '#ffffff', '#004a59'], dark: ['#000000', '#000000', '#5ee0ff'] },
};
