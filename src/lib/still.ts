
export function isGifHeader(b: Uint8Array): boolean {
  return (
    b.length >= 6 &&
    b[0] === 0x47 &&
    b[1] === 0x49 &&
    b[2] === 0x46 &&
    b[3] === 0x38 &&
    (b[4] === 0x37 || b[4] === 0x39) &&
    b[5] === 0x61
  );
}

const known = new Map<string, Promise<boolean>>();
function isGif(url: string): Promise<boolean> {
  let p = known.get(url);
  if (!p) {
    p = (async () => {
      try {
        const r = await fetch(url, { cache: 'force-cache' });
        const reader = r.body?.getReader();
        if (!reader) return false;
        const { value } = await reader.read();
        void reader.cancel();
        return !!value && isGifHeader(value);
      } catch {
        return false;
      }
    })();
    known.set(url, p);
  }
  return p;
}

export function still(img: HTMLImageElement) {
  let canvas: HTMLCanvasElement | null = null;
  let gone = false;
  const host = img.closest('button, a') ?? img.parentElement;
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const show = (moving: boolean) => {
    if (!canvas) return;
    img.style.display = moving ? '' : 'none';
    canvas.style.display = moving ? 'none' : '';
  };
  const on = () => show(!reduced);
  const off = () => show(false);

  async function freeze() {
    if (
      canvas ||
      !img.naturalWidth ||
      !(await isGif(img.currentSrc || img.src)) ||
      gone
    )
      return;
    canvas = document.createElement('canvas');
    canvas.width = img.naturalWidth;
    canvas.height = img.naturalHeight;
    canvas.getContext('2d')?.drawImage(img, 0, 0);
    canvas.className = img.className;
    canvas.setAttribute('aria-hidden', 'true');
    img.after(canvas);
    off();
  }

  if (img.complete) void freeze();
  img.addEventListener('load', freeze);
  host?.addEventListener('pointerenter', on);
  host?.addEventListener('pointerleave', off);
  host?.addEventListener('focusin', on);
  host?.addEventListener('focusout', off);
  return {
    destroy() {
      gone = true;
      img.removeEventListener('load', freeze);
      host?.removeEventListener('pointerenter', on);
      host?.removeEventListener('pointerleave', off);
      host?.removeEventListener('focusin', on);
      host?.removeEventListener('focusout', off);
      canvas?.remove();
    },
  };
}
