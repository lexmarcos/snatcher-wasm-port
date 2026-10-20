// A "segmented choice": a row or grid of toggle buttons that mirrors a <select>. Phones and tablets use it instead of the native list, which
// opens over the picture (on Android) and takes two taps; here one tap chooses and the picture changes at once. The <select> stays in the page
// (desktop, tests, and as the one place that holds the value); CSS shows one or the other (docs/design-system.md "Choice").
export interface Seg { set(value: string | null): void; relabel(): void }

export function makeSeg(host: HTMLElement, items: { value: string; label: () => string }[], pick: (value: string) => void): Seg {
  const buttons = items.map((it) => {
    const b = document.createElement("button");
    b.type = "button"; b.className = "btn"; b.dataset.value = it.value; b.setAttribute("aria-pressed", "false");
    b.onclick = () => pick(it.value);
    host.append(b);
    return b;
  });
  const seg: Seg = {
    set(value) { for (const b of buttons) b.setAttribute("aria-pressed", String(b.dataset.value === value)); },
    relabel() { items.forEach((it, i) => { buttons[i].textContent = it.label(); }); },
  };
  seg.relabel();
  return seg;
}
