import { i18n, t } from './i18n.svelte';

export function ago(seconds: number | null | undefined): string {
  if (!seconds) return t('home.never');
  const d = Math.round(Date.now() / 1000 - seconds);
  const rtf = new Intl.RelativeTimeFormat(i18n.language, { numeric: 'auto' });
  if (d < 60) return rtf.format(0, 'second');
  if (d < 3600) return rtf.format(-Math.round(d / 60), 'minute');
  if (d < 86400) return rtf.format(-Math.round(d / 3600), 'hour');
  if (d < 86400 * 30) return rtf.format(-Math.round(d / 86400), 'day');
  return rtf.format(-Math.round(d / (86400 * 30)), 'month');
}
