
export function arrows(node: HTMLElement, role: 'tab' | 'radio') {
  const key = (e: KeyboardEvent) => {
    const items = [...node.querySelectorAll<HTMLElement>(`[role="${role}"]`)].filter((x) => !x.hasAttribute('disabled'));
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    const n = items.length;
    const j =
      e.key === 'ArrowRight' || e.key === 'ArrowDown'
        ? (i + 1) % n
        : e.key === 'ArrowLeft' || e.key === 'ArrowUp'
          ? (i - 1 + n) % n
          : e.key === 'Home'
            ? 0
            : e.key === 'End'
              ? n - 1
              : -1;
    if (j < 0) return;
    e.preventDefault();
    items[j].focus();
    items[j].click();
  };
  node.addEventListener('keydown', key);
  return { destroy: () => node.removeEventListener('keydown', key) };
}

export function overflow(node: HTMLElement) {
  const check = () => node.classList.toggle('more', node.scrollLeft + node.clientWidth < node.scrollWidth - 1);
  const ro = new ResizeObserver(check);
  ro.observe(node);
  node.addEventListener('scroll', check, { passive: true });
  const mo = new MutationObserver(() => {
    node.querySelector<HTMLElement>('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    check();
  });
  mo.observe(node, { subtree: true, attributes: true, attributeFilter: ['aria-selected'] });
  check();
  return {
    destroy() {
      ro.disconnect();
      mo.disconnect();
      node.removeEventListener('scroll', check);
    },
  };
}
