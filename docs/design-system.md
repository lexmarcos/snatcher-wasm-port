# Design system: "Junker HQ terminal"

Code: `web/styles/system.css` (tokens, base, components, screens, motion), `web/index.html` (markup), `web/fonts/` (DotGothic16 subset + `OFL.txt`).
Checks: `web/test/design.ts` (screens and layout checks), `web/test/import.ts` (the importer states), `scripts/design-sheet.py` (contact sheet).
Screenshots go to `work/shots/design/` (gitignored; the contact sheet is `sheet.png`).

## 1. Subject

The machine Gillian uses at Junker HQ. The computers in Snatcher are not green phosphor terminals: they are beveled blue windows on dark navy,
like the game's own message box and command menu. The page is that window system on a late-80s Japanese PC (PC-88 / MSX2) screen.
Audience: players who own the disc. Job of the page: get from "I have my .cue and .bin" to playing with as little reading as possible.

## 2. Tokens

CSS custom properties on `:root`. The page is dark only (`color-scheme: dark`). Contrast ratios are WCAG 2.x, measured with the formula in the spec.

| Token | Hex | Use |
|---|---|---|
| `--field` | `#0E1640` | window body (navy, not black) |
| `--desk` | `#070B24` | page background, drawn as a 2x2 checker dither with `--field` |
| `--bevel-hi` | `#9DB4F0` | light edge, top left |
| `--bevel-mid` | `#4A5E9A` | title strip, quiet edges, rules |
| `--bevel-lo` | `#02041A` | dark edge, bottom right, inset wells |
| `--face` | `#26357A` | button face (added; see section 10) |
| `--ink` | `#E8ECF8` | text |
| `--dim` | `#8A97C4` | secondary text |
| `--call` | `#F8E038` | the yellow of the speaker names: primary action, focus and current item, progress fill. Nothing else |
| `--alert` | `#F05050` | errors only |
| `--ok` | `#58D888` | success lines only |

Contrast on the surfaces text really sits on (all five text colours reach AA 4.5:1 on `--field`, no colour had to be adjusted):

| Text colour | on `--field` | on `--desk` | on `--bevel-lo` (slot, strips) | on `--face` (buttons) | on `--bevel-mid` (title strip) |
|---|---|---|---|---|---|
| `--ink` | 14.73 | 16.43 | 17.19 | 9.50 | 5.29 |
| `--dim` | 6.05 | 6.75 | 7.06 | 3.90 (not used on buttons) | 2.17 (not used) |
| `--call` | 13.01 | 14.51 | 15.18 | 8.39 | 4.68 |
| `--alert` | 4.94 | 5.51 | 5.77 | 3.19 (not used) | not used |
| `--ok` | 9.60 | 10.71 | 11.20 | 6.19 | not used |

Rules that keep it true: `--dim` text never sits on a button face or the title strip; `--alert` appears only on `--field` and `--bevel-lo`;
the disabled button uses `--dim` on `--field` (6.05). Status lines carry a pixel glyph (check, cross, exclamation) besides the colour.

The dither (`--dither`, an SVG data URI with `image-rendering: pixelated`, shown at 2 px per cell) is the one signature texture: it is the
background of `body` and of the start overlay, and nowhere else (the pause backdrop is flat translucent `--desk`).

## 3. Type

One family: **DotGothic16** (Japanese dot-gothic, SIL OFL 1.1), self-hosted. `web/fonts/DotGothic16-subset.woff2` (19 KB, 312 glyphs: Basic Latin,
Latin-1, Latin Extended-A, general punctuation, arrows, box drawing, the squares) and `web/fonts/OFL.txt`. `web/server.ts` serves `/fonts/` and `/styles/`.
The page makes no third-party request (`test/design.ts` fails on any non-local request). The design grid of the font is 16 dots, so sizes are multiples of 16 px:

| Role | Size | Where |
|---|---|---|
| body, headings inside windows, buttons, labels | 16 px, line-height 24 px (1.5) | everything |
| window title | 32 px, line-height 40 px | title strip ("Web port", "Paused") |
| hero wordmark | 64 px (48 px at 480 px wide and below) | the start window |

No smaller sizes. Hierarchy comes from colour, bevel strips and spacing. Sentence case everywhere; no all-caps labels, no eyebrows.
Lines are at most 70ch (the font is half-width: 1ch = 8 px at 16 px). `-webkit-font-smoothing: none`, `font-smooth: never`, `font-kerning: none`.
`test/design.ts` walks every visible text node of the overlay (en and pt-BR), the toolbar, the debug overlay and the panel and fails if any size is not a multiple of 16 px.

**Coverage.** pt-BR diacritics `ã õ ç é ê á í ó ú à` and their capitals, `â ô ü ñ`, `…`, `–`, `—`, `×`, `·` are all in the font. **Missing glyphs:**
`▶ ► ▸` (U+25B6, U+25BA, U+25B8) and the check marks `✓ ✔`. They are not faked with a fallback font: the cursor, the check mark, the cross,
the exclamation mark and the disclosure arrows are pixel shapes drawn as CSS masks (`--m-cursor`, `--m-check`, `--m-cross`, `--m-bang`, `--m-down`, 8x8 grids shown at 16 px).
The fallback list (`MS Gothic`, `Osaka-Mono`, `sans-serif`) only matters for characters outside the subset, which the page does not use.
Exception to the size rule: the SVG icons of the controls table (`web/src/icons.ts`) depict hardware buttons (key caps, A/B/X/Y); their tiny letters are part of the drawing.

## 4. Shape and structure

Radius 0 everywhere. No blur anywhere: depth comes only from 2 px bevels made of hard inset shadows (`--tl` light edge, `--br` dark edge, over a 2 px `--bevel-lo` border).
Pressed = the two edges swap. Spacing steps are 8, 16, 24, 32 px.

## 5. Components

- **Window** (`.win`): beveled frame, title strip (`--bevel-mid`, 32 px title in `--ink`, language toggle at the right), `--field` body (`.win-body`).
- **Strip** (`.strip`): thin inset strip (inverted bevel, `--bevel-lo`) with a 16 px heading. Numbered markers (`.num`) are used only for the real sequence 1 Your disc, 2 Play.
- **Button** (`.btn`): beveled block on `--face`, label centred, 24 px side gutters. Hover and keyboard focus show the **cursor** (a pixel ▶ in the left gutter, like the
  game's command menu) and turn the label `--call`. Focus also draws a 2 px `--call` outline (offset 2 px), so focus never relies on colour alone.
  Pressed inverts the bevel. Primary (`.btn.primary`): `--call` label and a 2 px `--call` outline inside the edge, cursor always shown. Disabled: `--field` face, `--dim` label, flat.
  `.big` is the 48 px Play / Resume / Continue button. Toolbar buttons are 32 px; at 480 px and below every button is at least 44 px.
- **Cursor**: `::before` mask, `visibility` toggled, so nothing shifts. Also used on tabs and disclosure rows.
- **Meter** (`.meter` around a real `<progress max=1000>`): a row of 24 px blocks with 4 px gaps, `--call` for filled, `--face` for empty. The width is a whole number of blocks
  (`round(down, 100%, 28px)`) and `main.ts` `setProgress()` only sets values that end on a block edge, so the fill never shows a partial block. The `<progress>` keeps its role and values; the phase text is its label.
- **Slider**: native `<input type=range>` with a segmented track (16 px period, light = filled, set through `--v` by `main.ts`) and a beveled thumb that turns `--call` on hover and focus.
- **Checkbox**: native `<input type=checkbox>` as a square inset box with a pixel check mark (mask), 24 px, in a 32 px (44 px on phones) label row.
- **Select**: native `<select>` as an inset well with a pixel arrow.
- **Drive slot** (`.slot`, id `impBox`): inset dark slot with a dashed `--bevel-mid` outline that turns `--call` (and the text changes to "Release to import") while dragging.
- **Tabs** (`role=tablist`): Sound, Display, Controls, Touch, Gamepad, Saves. Roving tabindex; Left/Right wrap, Home/End jump, selection follows focus. The Saves tab holds Export saves / Import saves, the slot note, then the **Autosave** check box (`#autoBox`, on by default, title text explains the quiet moments and that the four slots are not touched), a status line (`#pnAuto`: the last autosave, or "Autosave is off: nothing new is saved." and "The autosave from ... stays until you delete it.", or the blocked-storage line) and the button "Delete autosave" (`#pnAutoDelete`). The selected tab is pressed in, with a `--call` label and the cursor.
- **Disclosure rows** (`<details>`): Saves, Controls, Other ways to load; quiet rows between two `--bevel-mid` rules, collapsed by default.
- **Status lines** (`.line`, `.notes li`): `.ok` (check), `.err` (cross), `.warn` (exclamation, `--ink`), `.dim`.
- **Toolbar** (`#bar`, `role=toolbar`): small beveled buttons on a slim strip under the canvas, with the status text.
- **Monitor frame** (`#monitor`): a raised `--bevel-mid` bezel with a sunken screen around the canvas. `main.ts` `fit()` measures the frame and the toolbar, so the canvas keeps an
  integer scale and `image-rendering: pixelated` (picture style "Plain pixels"; with a CRT style the picture takes the biggest whole-device-pixel size that fits, 8:7 wide pixels by default, see below). In fullscreen the frame is dropped. The page itself (bezel, panels, buttons) has no scanlines and no curvature: the CRT look belongs to the game picture only (`#crt`, laid exactly over `#screen` inside `#stage`).
- **Picture style** (Display tab, `web/src/display.ts`): a **select** "Picture style" (Plain pixels, CRT: consumer TV, CRT: studio monitor, Scanlines only; a fifth option "Custom" is shown only while the sliders differ from every preset), a dim help sentence under it (one per style), a warn line (`#crtNote`) only when the browser cannot run the filter or the window is too small for it (then the plain picture is shown), the **Smooth scaling** check box (only for the plain picture, hidden while a CRT style is drawing), a **Pixel shape** select (TV 8:7 / Square), and an **Adjust** fold (`details.inner`, the same quiet row as the other folds, top rule only) with six `.sl` sliders (Scanlines, Mask, Glow, Sharpness, Composite blend, Curvature; 0 to 100 %, steps of 5, same segmented track and value output as the volume sliders), a **Mask type** select (Aperture grille, Slot mask, Shadow mask) and **Reset to default style** (a `.btn`). All 16 px text, no new colours, no new shapes. Moving a slider switches the select to Custom without changing anything else. The choice is kept in `snatcher.crt` (`{"preset":"tv"}` or, for Custom, `{"preset":"custom","params":{...}}`) and removed by Clear everything.
- **Debug overlay** (`#debug`): a small window in the same style, 16 px.
- **Touch pad** (`#touch`, section 6 of `system.css`; phones and tablets, never on a desktop unless the player turns it on in the Touch tab): the on-screen Sega CD pad.
  D-pad on the left (four 1 x 1 arms of a 3 x 3 square around a dark hub, each arm carries the pixel cursor glyph turned to its direction), A B C on the right (A and B on top, C below B; 32 px letters),
  Menu above the D-pad (in portrait: centred just under the picture, when the gap above the controls has room) and Start above the buttons. All blocks are `--face` squares with the 2 px bevel, radius 0, no blur; a pressed block is pressed in (the two edges swap, the same as `:active` on
  a page button) and its glyph or letter turns `--call`. The one yellow outline (inside the edge, like `.btn.primary`) is on **C**, the confirm / fire button: yellow stays "the primary action".
  Size `--u` = 56 px (at least 48 px, set from the viewport: it shrinks to fit beside the picture or under it, never below 48; the Touch tab scales 90-140 %), gap 8 px, 12 px from the screen edge plus the safe area.
  The pad blocks use `--pad-op` (default 85 %, Touch tab); Menu and Start stay opaque. While the game is on screen the page around it is black (#000). Arrangements: `below` (portrait), `sides` (landscape, in the free columns beside the picture), `overlay` (narrow landscape, over the picture's edges).
  Hover is switched off on touch devices (`@media (hover: none)`), so a tapped button does not stay yellow.
- **Touch buttons of "touch the game"** (`.tbtn`, 48 px high, 96 px wide): Menu always; Back, Start / OK and Holster only while they do something (hidden without moving the others).
- **Rotate hint** (`.hint`): a beveled strip under the notch, one sentence and an OK button, never modal.
- **Choice** (`.seg`, `web/src/seg.ts`; phones and tablets only): a wrapping row of beveled buttons that replaces a `<select>` on a touch layout (Picture style, Pixel shape, Touch controls). One tap chooses. The chosen one is pressed in with a `--call` label and the cursor, like a selected tab (`aria-pressed`); 48 px high (two lines of label fit in 56), at least 136 px wide, a lone last button takes the whole row. The `<select>` stays in the page and is hidden by CSS on touch.
- **Menu sheet** (`html[data-sheet]`, `#panel` on a touch layout): the pause panel as a window next to the picture instead of over it (portrait: under the picture, landscape: right of it), same `.win`, title strip and bevels, no dimmed backdrop. CSS section 7 of `system.css`, rules in `docs/re/web.md` 11.10. Tabs on touch: Display, Sound, Touch, Saves (Gamepad only with a pad), one row, 4 px padding (the cursor is not drawn on these tabs: the pressed-in yellow tab says "here"). Its content scrolls inside the window and **the scroll bar stays visible** (16 px, `--bevel-lo` track, beveled `--face` thumb): anything that scrolls must show it. Right and bottom edges are `--bevel-mid` so the window reads on the black page. **Back to the game** (`.btn.primary.big`) is the last row, at the bottom of the sheet (on a short sheet, a phone held sideways or a sheet under 440 px, it replaces the title in the title strip). Sliders on touch: 48 px high, thumb 28 x 44 px. Each picture-style slider has a one-line `--dim` explanation under it (`.sl-hint`, also on a computer).
- **Touch targets**: every button on a touch layout is at least 48 px; the page's own buttons keep the 44 px rule at 360 px (checked by `test/design.ts`, `test/touch.ts`).

## 6. Screens

- **Start overlay**: one centred window (max 640 px, left-aligned). Title strip "Web port" + EN | PT-BR toggle; wordmark; one sentence of intro;
  1 Your disc (drive slot with the instruction, Import disc image, CD music option, block meter and phase, status line, the honest privacy note);
  2 Play (primary button, one status line); folded rows; the browser-storage line. Importer states: idle, dragging, importing (meter), imported ("Imported" button, Remove imported data), cached ("Your imported disc is in this browser"), error (`.err` line), removing (second click, then "Removed").
  **First visit** (no imported disc, no served or picked copy): only step 1 is shown, without its "1" marker (`#ovWin[data-stage="disc"]`); step 2, the folded rows and the storage line
  (`#ovAfter`) appear once a game source exists and hide again after "Remove imported data". Never a warning.
  **Disc already imported** (`data-stage="imported"`): step 1 (`#ovStep1`, the import) is hidden, and so are the step markers and the "Play" strip; the window shows Play with
  one status line (the imported disc, or the import result right after an import), the folded rows and the storage line. An imported disc wins over the dev server's `./assets`.
  The last folded row, **Remove the files and clear the site**, holds "Remove imported data" (saves and settings stay) and "Clear everything" (the imported disc plus every
  `snatcher.*` key of the origin: saves, gamepad buttons, settings; `.btn.danger`, alert outline, second click to confirm), which reloads the page as a first visit. The old "Game files / Choose extracted folder" block and the developer text live in
  **Other ways to load** (so a first-time visitor never sees them); when the dev server has `./assets`, that source is selected by itself and step 2 says so in one plain line.
- **Continue where you left off** (`#btnContinue`, hidden until an autosave exists and a game source is chosen): a second `.btn.primary.big` in the Play row, in front of Play. With an autosave it is the primary action:
  Play loses `.primary` (it keeps `.big`, a plain beveled button), and Continue takes the focus Play had. Under the row a dim status line (`#autoStatus`, `.line.dim`) says what Continue loads: "Autosave from Oct 18, 2026, 1:47 PM: act 1, 25 s played."
  (pt-BR "Salvamento automático de 18 de out. de 2026, 13:47: ato 1, 25 s de jogo."; the act is left out when it is none of the three). An autosave that cannot be used (another engine version, another disc or language, damaged) is removed and the same place shows
  a `.line.warn` sentence that says why and ends with "Press Play to start again." The Saves fold repeats the line (`#ovAuto`) and holds "Delete autosave" (`#ovAutoDelete`, second click to confirm, label changes to "Click again to delete").
- **Start overlay on a phone or tablet** (`html[data-touch]`): same window, but no drop slot (the instruction sits in full colour above Import disc image), 16 px between the parts of a step, the window kept inside the notch and home-bar insets (`.modal` pads with `--sai-*`), and in portrait it sits at the bottom of the screen so Import and Play are under the thumb. Wordmark 32 px on a screen under 500 px high.
- **Pause panel on a phone or tablet**: the menu sheet (section 5): the game picture stays visible and redraws live while a picture style or a slider changes. Display first, then Sound, Touch (with "How the buttons work"), Saves; no debug overlay or key hints.
- **Pause panel**: the same window titled "Paused": Resume first and primary, the status notes right under it, then the tabs. Esc, the Menu button and the gamepad Select button still open and close it, and it still pauses the game.
- **Toolbar, monitor, debug overlay**: sections 5.

Mobile at 360 px: the window is full width with a 16 px gutter, the title strip and wordmark shrink (48 px), steps and rows stack, the controls table becomes stacked rows, touch targets are at least 44 px, nothing scrolls horizontally (all three are checked by `test/design.ts` in en and pt-BR).

## 7. Motion

Exactly one orchestrated moment: when the start window first appears it opens like an 80s PC draws a window, a stepped reveal from the middle, `animation: win-open 320ms steps(8, end)`
(`clip-path`). Nothing else animates on its own; answers to user actions are instant bevel inversions. `prefers-reduced-motion: reduce` turns the reveal off.

## 8. Copy voice

Plain, player-facing, active voice, sentence case, English and pt-BR in `web/src/i18n.ts` and `i18n/pt-BR/ui.json`.
Intro: one sentence (plays your own copy of Snatcher (USA, Sega CD) in the browser; nothing is uploaded). Step 1 says to pick the .cue and its .bin file(s) or drop them.
Status lines say what happens and what to do next ("Imported in 41.2 s. Press Play. ..."). Errors say what went wrong and how to fix it, without apologising ("No .cue file among the 1 selected files (game.bin). Select the .cue and its .bin file(s) together.").
Autosave copy: the setting is called **Autosave** (pt-BR "Salvamento automático"); the start-window button is **Continue where you left off** ("Continuar de onde parou"); the status line names the date, the act and the play time; a removed autosave says what was wrong and what to do ("Press Play to start again."); delete asks twice and says "Autosave deleted."; the toolbar line says "autosaved" after each save.
Button labels are consistent through the flow: **Import disc image** -> Importing -> **Imported**; **Remove imported data** -> (second click) -> status "Removed". The honest note stays: the imported data stays in this browser until removed; the storage line stays.
Arrows are not used on buttons.

## 9. Rejected defaults

Green phosphor terminal on black; a near-black page with one acid accent; rounded SaaS cards with grey shadows; monospace data labels; all-caps tracked eyebrows; "->" on buttons; CRT scanlines on the page (the game picture has its own optional CRT styles, docs/re/web.md section 12; the windows, bezel and buttons never do).
(DotGothic16 is a pixel font whose Latin letters happen to be half-width: that is the font's design, used at its grid, not a monospace data style.)

## 10. Deviations from the plan, and the final pass

- **`--face` token added** (`#26357A`). The plan names no button face colour; `--field` buttons would not read as raised on a `--field` window, and `--bevel-mid` is the title strip.
- **Font is a subset**, not the whole woff2: the full TTF is 2 MB because of the Japanese set; the page needs 312 glyphs (19 KB). Made with `pyftsubset` from the OFL release; the license file is next to it.
- **Cursor and check marks are drawn, not typeset**: the font has no ▶ and no check mark (section 3).
- **Dither cell is 2 px** (the 2x2 tile is shown at 4x4 CSS px): at 1 px the dark checker is invisible on ordinary screens.
- **Language toggle is two buttons** (`#langOvEn/#langOvPt`, `#langPnEn/#langPnPt`, `aria-pressed`) instead of the `<select>`s; `main.ts` and `test/lang.ts` follow.
- **Pause backdrop is flat translucent `--desk`**, not the dither, to keep the dither in one place and the paused game visible behind the window.
- **Progress values are quantised** to whole blocks (the `<progress>` stays real); the percentage in the text is exact.
- **Primary buttons always show the cursor**, not only on hover, as the "▶ Play" of the layout sketch.
- **Final pass (one decorative thing removed)**: the inset bevel around the drive slot. It repeated the dashed border (two frames for one idea); the dashed outline alone says "slot".
- Not done: the toolbar `#msg`/`#bgStatus` texts still come from `main.ts` as before; the hardware icons of the controls table keep their small SVG lettering (section 3).
