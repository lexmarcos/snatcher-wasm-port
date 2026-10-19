// Phone and tablet platform helpers: device detection, screen wake lock, haptics, fullscreen with a fallback for iPhone, the "rotate" hint.
// Everything degrades silently: each feature is feature-detected and a missing one only means that its control is hidden.
// Documentation: docs/re/web.md section 11 ("what was only checked in emulation").

const nav = (): any => (typeof navigator !== "undefined" ? navigator : {});
const mm = (q: string): boolean => { try { return typeof matchMedia === "function" && matchMedia(q).matches; } catch { return false; } };

/** iPhone, iPod and iPad (iPadOS 13+ reports "MacIntel" with touch points). */
export function isIOS(): boolean {
  const n = nav();
  return /iPhone|iPad|iPod/.test(n.userAgent ?? "") || (n.platform === "MacIntel" && (n.maxTouchPoints ?? 0) > 1);
}
export const isIPhone = (): boolean => /iPhone|iPod/.test(nav().userAgent ?? "");
/** The page was opened from the home screen (no browser bar). */
export const isStandalone = (): boolean => mm("(display-mode: standalone)") || mm("(display-mode: fullscreen)") || (nav() as any).standalone === true;
/** Main pointer is a finger (phones, tablets), at least one touch point. */
export const isCoarsePrimary = (): boolean => mm("(pointer: coarse)") && (nav().maxTouchPoints ?? 0) > 0;
/** Some touch screen exists (also next to a mouse). */
export const hasTouch = (): boolean => (nav().maxTouchPoints ?? 0) > 0 || "ontouchstart" in globalThis;
export const canVibrate = (): boolean => typeof nav().vibrate === "function";
export const canWakeLock = (): boolean => !!nav().wakeLock?.request;
/** Element fullscreen exists (Android, iPad, desktop); iPhone Safari has none. */
export const canFullscreen = (): boolean => !!document.fullscreenEnabled;

/** Light haptic tick (Android; ignored elsewhere). */
export function haptic(ms = 8): void { try { nav().vibrate?.(ms); } catch { /* ignore */ } }

/**
 * Keeps the screen on while the game runs. `want(true)` takes the lock (needs a visible page), `want(false)` releases it; the browser drops the lock
 * when the page is hidden, so it is taken again when the page is visible and the game is still running.
 */
export class WakeLock {
  private sentinel: any = null;
  private wanted = false;
  /** the page cannot keep the screen on (no API, or the browser refused): shown in the Touch tab */
  failed = false;
  supported = canWakeLock();
  onChange: (() => void) | null = null;
  constructor() {
    document.addEventListener("visibilitychange", () => { if (!document.hidden && this.wanted) void this.acquire(); });
  }
  get held(): boolean { return !!this.sentinel && !this.sentinel.released; }
  want(on: boolean) {
    this.wanted = on;
    if (on) void this.acquire(); else void this.release();
  }
  private pending = false;
  private async acquire() {
    if (!this.supported || this.held || this.pending || document.hidden) return;
    this.pending = true;
    try {
      this.sentinel = await nav().wakeLock.request("screen");
      this.failed = false;
      this.sentinel.addEventListener?.("release", () => { this.sentinel = null; this.onChange?.(); });
    } catch { this.failed = true; this.sentinel = null; }
    this.pending = false;
    this.onChange?.();
  }
  private async release() {
    const s = this.sentinel; this.sentinel = null;
    try { await s?.release?.(); } catch { /* ignore */ }
    this.onChange?.();
  }
}

/** Fullscreen on or off; true when a change was requested. */
export function toggleFullscreen(): boolean {
  if (!canFullscreen()) return false;
  if (document.fullscreenElement) void document.exitFullscreen(); else void document.documentElement.requestFullscreen({ navigationUI: "hide" } as any).catch(() => {});
  return true;
}

/**
 * What the Touch tab can say about getting rid of the browser bars: `fullscreen` = a button works; `homescreen` = iPhone / iPod in Safari (no element
 * fullscreen): suggest Add to Home Screen, which opens the page without browser chrome; `none` = nothing to offer (already standalone or fullscreen-less desktop).
 */
export function chromeAdvice(): "fullscreen" | "homescreen" | "none" {
  if (isStandalone()) return "none";
  if (canFullscreen()) return "fullscreen";
  return isIOS() ? "homescreen" : "none";
}
