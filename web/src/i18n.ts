// Front end language: English (built in) and Brazilian Portuguese (i18n/pt-BR/ui.json, fetched on demand).
// The game text itself is translated inside the engine (docs/i18n.md); this file only covers the page around it.
// Every user-visible string of the page has a key here. `T(key, vars)` returns the string of the active language and
// falls back to English when a key is missing, so a half-finished ui.json never breaks the page. `{name}` marks a variable.

export type Lang = "en" | "pt-BR";

export const UI_EN = {
  // toolbar and canvas
  "bar.menu": "Menu (Esc)", "bar.menu.title": "Settings, controls, saves (Esc)",
  "bar.mute": "Mute (M)", "bar.unmute": "Unmute (M)", "bar.mute.title": "Mute audio (M)",
  "bar.full": "Fullscreen (F)", "bar.full.title": "Fullscreen (F)",
  "canvas.label": "Snatcher game screen",
  "msg.loading": "loading…", "msg.ready": "ready", "msg.waiting": "waiting for game files",
  "msg.files": "{n} game files loaded", "msg.nosaves": "no saves to export", "msg.saved": "saved to slot {n}",
  "msg.pad": "gamepad: {name}", "msg.nopad": "no gamepad", "msg.engine": "engine error: {e}",
  // start overlay
  "ov.title": "Snatcher",
  "ov.intro": "Play your own copy of Snatcher (USA, Sega CD) in this browser; nothing is uploaded.",
  "ov.play": "Play", "ov.pick": "Choose extracted folder…", "ov.loading": "Loading {done}/{total}…",
  "ov.saves": "Saves", "ov.import": "Import saves…", "ov.export": "Export saves", "ov.controls": "Controls",
  "ov.using": "Using: {label}", "ov.nofiles": "No game files found in that selection.",
  "ov.nofiles2": "Looked at {n} files. Pick the folder that contains the extracted \"files\" folder (assets/raw) or the whole assets folder.",
  "ov.nolocal": "No game files on this server.",
  "ov.fail": "Could not start: {e}",
  // sources
  "src.server": "local server (./assets)", "src.picked": "picked folder",
  "src.listed": "{n} game files listed in assets/raw/filelist.tsv",
  "src.files": "{n} game files", "src.boot": "header.bin {h}, ip.bin {i}", "src.found": "found", "src.missing": "missing",
  "src.audio": "audio: {n} .ogg files, pcm.json {p}, fm.json {f}",
  "err.noassets": "no local assets found at /assets/raw (run the extraction, see README)", "err.noserver": "the local asset server is not reachable",
  "err.empty": "assets/raw/filelist.tsv is empty",
  // disc image import
  "ov.disc": "Disc image",
  "imp.help": "Pick the <code>.cue</code> file and its <code>.bin</code> file(s) together, or drop them here.",
  "imp.privacy": "Your files are read in this browser tab only and are never uploaded. The imported data stays in this browser until you remove it.",
  "imp.button": "Import disc image…",
  "imp.cancel": "Cancel",
  "imp.cd": "Keep the CD music in this browser (490 MB more). Without it, pick the disc again on each visit to hear the music.",
  "imp.reading": "Reading the cue sheet. Keep this tab open.",
  "imp.phase.header": "Checking the disc…",
  "imp.phase.files": "Reading the game files…",
  "imp.phase.voices": "Decoding the voices…",
  "imp.phase.fm": "Planning the music…",
  "imp.phase.cd": "Copying the CD music…",
  "imp.phase.done": "Finishing…",
  "imp.done": "Imported in {s} s. Press Play. The music is prepared in the background.",
  "imp.found": "Your imported disc is in this browser: {title}, {size}. Press Play.",
  "imp.use": "Use imported data",
  "imp.remove": "Remove imported data",
  "imp.remove.confirm": "Click again to remove",
  "imp.removed": "Removed. The imported data is gone from this browser.",
  "imp.storage": "Browser storage: {used} used of {quota}. Imported data: {cache}. Persistent storage: {persist}.",
  "imp.persist.yes": "granted",
  "imp.persist.no": "not granted (the browser may clear it if the disk gets full)",
  "imp.nospace": "Not enough free browser storage: the import needs about {need} and {free} are free. Untick the CD music option to save 490 MB, or free some disk space.",
  "imp.nosupport": "This browser cannot store the import (no private file system, OPFS). Use a current Chrome, Edge, Firefox or Safari, or pick an extracted folder instead.",
  "imp.err.nocue": "No .cue file among the {n} selected files ({detail}). Select the .cue and its .bin file(s) together.",
  "imp.err.manycues": "More than one .cue file was selected ({detail}). Import one disc at a time.",
  "imp.err.nobin": "The cue sheet names files that were not selected: {detail}. Select them together with the .cue (every .bin file of the disc).",
  "imp.err.badcue": "This .cue file cannot be used: {detail}",
  "imp.err.toobig": "{detail} is too big to be a cue sheet. Pick the .cue file, not the image.",
  "imp.err.cue": "The cue sheet does not match the image files: {detail}",
  "imp.err.notsega": "This is not a Sega CD disc image ({detail}). Pick the .cue of a raw BIN/CUE image (CHD files are not supported yet).",
  "imp.err.notsnatcher": "This is a Sega CD disc, but not Snatcher: {detail}.",
  "imp.err.edition": "This looks like another edition of Snatcher: {detail}. Only Snatcher (USA) is supported.",
  "imp.err.region": "Wrong region code: {detail}. Only Snatcher (USA) is supported.",
  "imp.err.digest": "This is Snatcher, but its data is not the known Snatcher (USA) dump (checksum {detail}).",
  "imp.err.short": "The image is truncated, or a .bin file is incomplete: {detail}",
  "imp.err.format": "The image is not a raw 2352-byte-per-sector BIN: {detail}",
  "imp.err.missing": "A game file is missing on the disc: {detail}",
  "imp.err.data": "The disc data is not what was expected: {detail}",
  "imp.err.io": "Reading or storing the data failed: {detail}",
  "imp.err.cancelled": "Import cancelled. Pick the files again to start over.",
  "src.imported": "imported disc (browser storage)",
  "src.imp.title": "{title}: {files} game files",
  "src.imp.cdkept": "CD music stored in the browser",
  "src.imp.cdsession": "CD music is read from your files, not stored: pick the disc again on your next visit to hear it",
  "src.imp.cdnone": "CD music is not stored: import the disc again to hear it",
  "bg.fm": "preparing music {done}/{total}",
  // settings panel
  "pn.title": "Paused", "pn.resume": "Resume",
  "pn.music": "Music", "pn.voice": "Voice", "pn.fx": "Effects", "pn.smooth": "Smooth scaling",
  "pn.vsub": "Voice subtitles", "pn.vsub.title": "Show the translated text of the voice clips in the message box (Brazilian Portuguese only)",
  "pn.debug": "Debug overlay (F1)", "pn.buttonsfor": "Buttons for", "pn.reset": "Reset to defaults",
  "pn.promptlabel": "Button names in game text", "pn.prompt.auto": "Icons of the last used device", "pn.prompt.letters": "Original letters (A B C)",
  "pn.sound": "Sound mode (set by the game's options screen): FM {fm}, PCM {pcm}.", "pn.mono": "mono", "pn.stereo": "stereo",
  "pn.savesnote": " Imported saves show up the next time the title screen is opened.",
  // notes
  "note.ok": "Everything loaded.", "note.noengine": "The engine (web/pkg) is not built: run scripts/build-web.sh. {e}",
  "note.nofile": "none of the game files could be read",
  "note.failfiles": "{fail} of {total} game files could not be read; the game may stall.",
  "note.noboot": "header.bin / ip.bin not found: the copyright and Konami logo screens are skipped (optional).",
  "note.audio": "{n} audio file(s) not found or not decodable (last: {last}); those sounds are silent.",
  "note.manifests": "Audio manifests missing ({list}): {what}. Run the audio pipeline (README).",
  "note.manifests.all": "voices, FM music and effects are silent", "note.manifests.part": "part of the audio is silent",
  "note.start": "{e}", "note.crash": "The engine stopped with an error: {e}",
  "note.storage": "Browser storage is blocked: saves last only until this page closes. Use Export saves.",
  "note.nosaves": "No saves to export.", "note.exported": "Saves exported.",
  "note.imported": "Imported save slot(s) {slots}.", "note.notimported": "No save imported.",
  "note.nopack": "The pt-BR translation was not found (run scripts/build-web.sh); the game text stays in English.",
  "saves.slots": "Saved slots: {list}.", "saves.none": "No saves yet.",
  "saves.volatile": " Browser storage is unavailable: saves are kept for this session only, export them.",
  // controls help
  "ctl.keyboard": "Keyboard", "ctl.gamepad": "Gamepad", "ctl.lastused": "Last used. Press a gamepad button to show its controls here.",
  "ctl.lastdev": "{name} (the last device used){extra}.", "ctl.generic": ", shown with Xbox-style icons",
  "ctl.move": "Move", "ctl.arrows": "Arrow keys", "ctl.dpad": "D-pad", "ctl.stick": "left stick",
  "ctl.confirm": "Confirm / fire", "ctl.cancel": "Cancel / light", "ctl.draw": "Draw blaster", "ctl.start": "Start",
  "ctl.also": " (also {k})", "ctl.sorspace": "S or Space",
  "ctl.web": "Web menu", "ctl.orEsc": " or Esc", "ctl.esc": "Esc",
  "ctl.gun": "Light gun", "ctl.mouse": "mouse",
  "ctl.gunhelp": " Move to aim, left button = trigger, right button = second button (holster). Pressing any pad key switches back to the pad.",
  "ctl.misc": "Fullscreen / mute / debug",
  "pad.assign": "Assign…", "pad.press": "Press a button… (Esc cancels)", "pad.nonstd": " (non-standard mapping)",
  "pad.connected": "Connected: {list}.", "pad.none": "No gamepad detected. Press a button on it (browsers only show a pad after its first input).",
  "pad.generic": "Unknown controllers use Xbox-style icons. ", "pad.connect": "Connect a gamepad and press a button on it first.",
  "pad.row": "{what} (Genesis {g})",
  "ov.window": "Web port",
  "ov.step1": "Your disc",
  "ov.step2": "Play",
  "ov.invite": "Import your disc in step 1, then press Play.",
  "ov.usinglocal": "Using the game files found on this server. Press Play.",
  "ov.alt": "Other ways to load",
  "ov.alt.folder": "Already have the game files extracted? Choose that folder (the project's assets folder works best). The files are read in this page only.",
  "ov.alt.dev": "Developers: after the extraction, <code>bun web/server.ts</code> serves <code>./assets</code> and this page selects it by itself.",
  "imp.slot.drag": "Release to import",
  "imp.button.busy": "Importing…",
  "imp.button.done": "Imported",
  "bar.label": "Game controls",
  "tab.sound": "Sound",
  "tab.display": "Display",
  "tab.controls": "Controls",
  "tab.gamepad": "Gamepad",
  "tab.saves": "Saves",
  "pn.tabs": "Menu sections",
  "pn.map": "Button map",
} as const;

export type UiKey = keyof typeof UI_EN;

let dict: Partial<Record<UiKey, string>> = {};
let lang: Lang = "en";

export const getLang = (): Lang => lang;

export function setUiDict(l: Lang, d: Record<string, string> | null) {
  lang = l;
  dict = l === "en" || !d ? {} : (d as Partial<Record<UiKey, string>>);
  try { document.documentElement.lang = l === "en" ? "en" : "pt-BR"; } catch { /* no document in unit tests */ }
}

export function T(key: UiKey, vars?: Record<string, string | number>): string {
  let s: string = dict[key] ?? UI_EN[key];
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? String(vars[k]) : m));
  return s;
}

/** Parse "en", "pt-BR", "pt_br", "pt" ... */
export function parseLang(v: string | null | undefined): Lang | null {
  if (!v) return null;
  const l = v.toLowerCase().replace("_", "-");
  if (l === "pt-br" || l === "pt") return "pt-BR";
  if (l === "en" || l.startsWith("en-")) return "en";
  return null;
}

/** Fill the static page: `data-i18n` (text), `data-i18n-html` (markup written by us), `data-i18n-title`, `data-i18n-label` (aria-label). */
export function applyStatic(root: ParentNode = document) {
  for (const e of Array.from(root.querySelectorAll<HTMLElement>("[data-i18n]"))) e.textContent = T(e.dataset.i18n as UiKey);
  for (const e of Array.from(root.querySelectorAll<HTMLElement>("[data-i18n-html]"))) e.innerHTML = T(e.dataset.i18nHtml as UiKey);
  for (const e of Array.from(root.querySelectorAll<HTMLElement>("[data-i18n-title]"))) e.title = T(e.dataset.i18nTitle as UiKey);
  for (const e of Array.from(root.querySelectorAll<HTMLElement>("[data-i18n-label]"))) e.setAttribute("aria-label", T(e.dataset.i18nLabel as UiKey));
}
