# WEB workstream – browser front end

Code: `web/index.html`, `web/src/{main,sources,audio,input,pacer,saves}.ts`, `web/server.ts`, wasm API in `engine/src/lib.rs`
(`Game`: new, load_file, boot, new_game, debug_jump, set_save, set_voice_len, set_fm_len, set_sfx_len, set_gun, tick,
framebuffer_ptr, take_events, debug_info). Build `scripts/build-web.sh`, tests `scripts/test-web.sh`.

## Status
- **Done and tested** (`web/test/ui.ts`, `pace.test.ts`, `sources.test.ts`, `shot.ts` with `newgame` and `scene=SP01:0`, no console errors):
  start overlay (served assets or folder picker), menu (Esc, pauses), volume sliders, fullscreen, mute, pause on hidden tab,
  smoothing and debug toggles, save export/import, save events reach the store and the engine, mono/stereo, FM/PCM stop
  commands, LRU buffer cache, visible notes for missing optional assets, phone-width layout.
- **Not done**: touch controls (no on-screen pad; phones can only use a Bluetooth gamepad), real-ear check of the mono/stereo option
  (verified structurally and numerically, see 4), audio of `kind==2` voice music ducking (audio.md §5 says ~1 dB, ignored),
  the RF5C164 voice `pan` nibbles (order unknown, audio.md "Open questions"; voices play centred).
- **Needs the coordinator / engine owners**: `engine/src/menus.rs` `sfx_1c` emits `Out::Pcm` but its target is the FM effect
  (queue command 0x1C = FM sfx, S:0x1013E; game.rs `0x1C => fm_cmd`); the front end plays `pcm` events as FM effects. If
  `sfx_1c` is changed to `fm_cmd`, the `pcm` case in `handleEvent` becomes dead. `docs/re/modules.md` still says "mono/stereo
  option has no audible effect yet": it now has one in the front end (below).

## 1. Asset sources (`sources.ts`)
Two interchangeable `Source`s (game files, HEADER.BIN/IP.BIN, audio files by path relative to `assets/`):
- **served**: `GET /assets/raw/filelist.tsv` lists the disc files (PCMLD_01.BIN is skipped: it is the PCM loader image, the
  audio pipeline replaces it), files come from `/assets/raw/files/NAME`, boot code from `/assets/raw/{header,ip}.bin`,
  audio from `/assets/web/music/trackNN.ogg` and `/assets/audio/web/**`. `web/server.ts` only serves from the local `assets/` dir.
- **picked**: `<input webkitdirectory>`; files are read lazily as `File.arrayBuffer()` (so no object URLs are needed:
  decoding takes the bytes directly). Recognition by name and path (`webkitRelativePath`): game files = files directly inside a
  directory called `files`; `header.bin` / `ip.bin` by name; `.ogg`/`.json` by the longest matching path suffix
  (`audio/web/fm/sfx_01.ogg`, then `web/fm/sfx_01.ogg`, ...), so picking `assets`, `assets/audio/web` or `assets/web/music`
  all work; the unrelated raw `assets/audio/pcm.json|fm.json` are never used for manifests (need at least `web/x.json`).
  A flat selection without directory info falls back to `*.BIN|*.TXT` and name matching. Tested end to end in headless Chrome
  by blocking `/assets/` after load and uploading the `assets` directory (Chrome fills a `webkitdirectory` input from a
  directory path, 5933 files): the game runs and the manifests and a CD track decode from the picked files.
- Optional files (header/ip, manifests, individual oggs) degrade to a visible note in the overlay/menu ("Status"); a
  missing required game file set blocks Play with an error.

## 2. Loop, input, pause
- `pacer.ts`: fixed 1/60 s steps from requestAnimationFrame timestamps. Timestamps within ~8 % (max 1.4 ms) of 30/60/120/180 Hz
  periods are snapped to them, so 60 Hz jitter gives exactly one tick per frame and 120 Hz gives 1,0,1,0; other rates (144 Hz) average
  60 ticks/s; gaps > 100 ms (hidden tab, stall) are dropped, at most 6 catch-up ticks per frame. `pace.test.ts` simulates 30/60/120/144 Hz with
  jitter over 10-20 s of virtual time.
- Pause: menu open, start overlay, or `document.hidden` stops ticking and suspends the AudioContext; the pacer is reset on resume.
- `Input.enabled` is false while a modal is open (keys go to the page; Enter/Space on a focused button never drive the game).
- Canvas scale: largest integer multiple in *device* pixels that fits (fractional only below 1x), so 125 % DPI screens stay crisp.

## 3. Saves
`snatcher.save.N` (N = 1..4) hold base64 of the 0x1C0-byte block (`SAVE_SIZE`, core/src/script/vm.rs). On a `save` event the
store is updated and `set_save` is called with the first 0x1C0 bytes (the engine also updates its own copy in game.rs cmd 0x1E, so
the call is redundant but harmless). Export = JSON `{format:"snatcher-web-saves", version:1, size:448, slots:{"1":"<b64>"}}`;
import validates format, slot range and length (>= 0x1C0, extra bytes dropped), reports rejected slots. The title screen reads the
slots when it opens (`read_slots`, title.rs), so an import made before pressing Play is seen at once; one made while playing is
seen the next time the title screen is entered (HYPOTHESIS: not tested in the running game). When localStorage is blocked, saves
stay in memory and the menu says to export them.

## 4. Audio (`audio.ts`)
Graph: CD -> `cdBus`; FM music -> `fmMusicBus`, FM sfx -> `fmSfxBus`, both -> `fmMix`; voices -> `voiceBus`, PCM sfx -> `pcmSfxBus`,
both -> `pcmMix`; all -> `master`. Sliders: Music = CD + FM music, Voice = voices, Effects = FM + PCM sfx; gain = slider^2.
- **Sound mode**: FM id 0xF3/0xF4 (M:0xFF51A8: F3 forces centre pan, F4 stereo) and PCM 0xFC/0xFD (S:0x162E0: mono/stereo) set
  `fmMix`/`pcmMix` to `channelCount` 1 or 2 (explicit, "speakers"): mono = 0.5 (L+R) on both outputs. Verified with an
  OfflineAudioContext (L=1,R=0 gives 0.5/0.5 mono and 1/0 stereo, `ui.ts`). The engine emits these at power-on (F4+FD) and on the
  options screen (title.rs `sound_mode`). HYPOTHESIS: the original option does not touch CD-DA (it does not pass the sound drivers), so
  CD tracks are never downmixed.
- **Stop commands**: FM F1 stops all FM sound (music + effects), F2 stops the music, 0xFF fades it over 85 frames; PCM 0xFF stops the
  voice and all PCM effects; `music_stop` events stop the CD track. A stop or replacement issued while a file is still loading cancels the
  older request (generation counters; tested with a cleared cache).
- **Cache**: decoded buffers in an LRU capped at 320 MB decoded size / 400 entries (a 4-minute CD track is ~70 MB); playing sources keep
  their own reference. Missing files are remembered (no refetch loop) and reported once through `onMissing`.
- Relative mixer levels FM/PCM/CD stay HYPOTHESES (all unity), as in audio.md.

## 8. Disc image import (`imported.ts`, `import.worker.ts`, `cache.ts`, `core/src/import.rs`, `engine/src/importer.rs`)
The simple path for a player: open the page, press **Import disc image (.cue + .bin)** (or drop the files on the page), press Play. No local
extraction pipeline, no ffmpeg, no server data. Everything runs in the browser; nothing is uploaded (the page says so in the overlay).

**Input.** One multi-file picker (`<input type=file multiple accept=".cue,.bin,.img,.iso">`) or drag and drop on the overlay (files or a folder).
`resolvePick` finds the one `.cue`, asks Rust (`cue_files`) for the `FILE` names and matches them with the picked files by name (case
insensitive, folders ignored). A renamed single image is accepted when it is the only image picked. Layouts: single bin (chdman) and
Redump multi-bin (one `FILE` per track, `INDEX 00 00:00:00` / `INDEX 01 00:02:00` inside each file). `PREGAP` (not stored in a file) and
`INDEX 00` handling is in `disc::Cue::layout` (unit tests in `core/src/disc.rs`): a track runs from its INDEX 01 to the next track's
INDEX 00 of the same file (or the end of its file), so the pregap of the next track never belongs to this one, in both layouts.
Not supported: CHD, `MODE2`, `WAVE` audio files (clear messages).

**Lazy reads.** The Rust importer reads the image through `DiscIo::read(file, offset, buf)`. In the browser that is `FileReaderSync` on
`File.slice` inside a Web Worker (sectors in pieces of 256), so the 625 MB image is never in memory, in JS or in wasm: the wasm heap
holds only the small game files (about 2 MB) and one 600 KB piece at a time. Output goes to OPFS through synchronous access handles, also
in pieces.

**Disc check** (`import::check_header`, then a digest). Sector 0 must start with `SEGADISCSYSTEM`; the title must be SNATCHER; serial
`GM T-95035` and region field `J` (what the Snatcher (USA) dump has, offset 0x1F0). Distinct messages: not a Sega CD image, another Sega
CD game, another edition of Snatcher (serial shown), region code changed, truncated / wrong-size / non-raw image, `.bin` missing. Then a
SHA-256 over the header, IP and every ISO9660 file (`files_digest`: sorted `name\tsize\tsha256` lines) must equal
`SNATCHER_USA.digest` (checked right after the data track has been read, before the long steps). The digest was cross-checked with an
independent Python script over `assets/raw`. Only the hash is in the repository.

**What is produced** (table in the module doc of `core/src/import.rs`): `files.pack` (the 99 game files, byte-identical to `snt extract`),
`header.bin`, `ip.bin`, `voices.pcm` (1197 clips minus banks, signed 8-bit mono), `sfx.pcm` (banks and drum kit), `cdda.pcm` (the raw
2352-byte-per-sector audio of the 20 tracks), `pcm.json` / `fm.json` (same fields as the pipeline's, plus `at: [stream, offset, bytes]`),
`meta.json` (last). The front end never touches ogg: `ImportedSource.direct()` builds `AudioBuffer`s from these bytes
(`AudioOut.direct` hook): s8 voices/banks (value / 128), 16-bit stereo for FM (48 kHz) and CD (44.1 kHz) converted piece by piece into
the channel arrays, so a 7 minute track needs the AudioBuffer (148 MB as float32) plus 1 MB, not two copies.

**FM audio.** Rendered with the Rust port of the 68000 FM driver + YM2612 (`FmRenderer`), in the worker. Measured natively: all 122 sounds
~90 s, one 6 minute loop (song 6c) 7 s; in the browser see section 8.1. Decision: the manifest (`fm.json`: exact length, loop start/end,
loop kind of every sound) is produced during the import in about 1 s by running the 68000 driver **without** the chip
(`render::render_dry`; neither the driver nor the loop detector reads the chip; tested equal to the full render for all 122 sounds and
equal to the old `fm.json`). The engine therefore has every length at once and the game starts when the import ends. The audio of each
sound is then rendered in the background (shortest first), one file per sound in the cache, with a status in the bar ("preparing music
n/122"); a sound the game asks for before its turn is moved to the front (`fm-want`) and `audio.fmCmd` waits for it like for a slow
download. When all are cached the worker is terminated (its wasm heap, up to ~150 MB for the longest loop, is released). A later visit
renders only what is missing. The renderer keeps only a short window of chip samples (resampling to 48 kHz while rendering: peak for the
6 minute loop 144 MB instead of 232 MB, output identical, tested against the old WAVs).

**Cache** (`cache.ts`): OPFS directory `snatcher-import-v1`. `meta.json` is written last, so a failed or cancelled import leaves nothing
(the directory is removed); a started FM file is valid only when its size equals `frames * 4`. `navigator.storage.persist()` is requested at
import (the browser may refuse; the page shows whether it was granted) and the overlay shows `navigator.storage.estimate()` usage and the
size of the imported data. **Remove imported data** (second click confirms) deletes the directory. Free space is checked before the import
(needs about 175 MB + 560 MB FM + 490 MB CD).
**CD-DA: copy or keep a handle?** Default: copy the raw sectors into the cache (490 MB), because a `File` picked with `<input type=file>`
cannot be kept across visits and a persisted `FileSystemFileHandle` (Chromium only, needs `showOpenFilePicker` and a permission prompt each visit)
would not work in Firefox / Safari. The checkbox "Keep the CD music in browser storage" turns the copy off: the import is then 490 MB smaller, the
music plays this session from the picked files (`s16Buffer` on the `File`), and on the next visit the tracks are reported missing until the disc is
imported again. Sizes of a full import: files 2 MB + voices 81 MB + effects 2 MB + CD 486 MB + FM about 538 MB = about 1.1 GB.

### 8.1 Measurements (headless Chrome 154, this laptop; `bun web/test/import.ts`, one fresh profile per run)
| | single bin (file input) | Redump multi-bin, 21 files (drag and drop) |
|---|---|---|
| import, pick -> Play enabled (header check, 99 files, 1197 voices, banks, drums, FM manifest, 486 MB of CD audio copied, SHA-256) | 13.7 s | 12.5 s |
| same import natively (`snt import-disc`, warm disk cache) | 2.2 s | |
| FM audio of all 122 sounds, rendered in the worker in the background | 77.7 s | 77.6 s |
| same natively (`snt audio-fm`, all 122) | ~90 s | |
| browser RSS (whole process tree): idle page / peak during the import / peak over import + FM + 600 frames of play | 538 / 844 / 1067 MB | 541 / 799 / 1146 MB |
| cache in OPFS (files 2 MB, voices 81, effects 2, CD 486, FM 538, manifests) | 1086 MB | 1086 MB |
Decision on FM: a blocking full render would add 78 s to the first visit, a lazy per-sound render would make the first use of every long song wait
2-10 s every session. Rendering everything once in the background with on-demand priority gets the player into the game after the 13 s import, and
the cache makes every later visit instant. The longest single sound (6 minute loop, song 6c) takes 7 s natively and the whole set is not slower in the worker than natively
(78 s against ~90 s), so a request for a sound that is not rendered yet waits for the one in progress (7 s at worst) plus its own render; the
background queue starts with the shortest sounds.
Memory: the image is read in pieces of 600 KB, so the import does not grow with the image size: the worker's wasm heap stays small (printed by the test:
import, FM), the rest of the +300 MB over the idle page during the import is V8 / Chrome buffering of the pieces and of the OPFS writes. During play the
renderer also holds the decoded `AudioBuffer`s (a 7 minute CD track = 148 MB as float32; the LRU in `audio.ts` caps them at 320 MB).
Bit-exactness is checked in the browser as well: CD track 04, voice 41 and FM effect 01 (rendered in the browser) equal the reference WAVs sample
for sample, both after the import and after a reload from the cache.

## 5. Tests
(The importer tests are in section 8: `cargo test --release -p sncore --test import` and `bun web/test/import.ts`.)
`scripts/test-web.sh [outdir]`: `bun test web/test/` (pacer, picked-folder recognition), then headless Chrome (chrome-headless-shell from
`~/.cache/puppeteer`): `ui.ts` (overlay, menu pause, hidden-tab pause, volumes, mono/stereo, stop commands, LRU, save import/export and
save event, picked-folder run, degraded assets, phone width 360x640, screenshots to the out dir), `shot.ts newgame` and `shot.ts scene=SP01:0`
(exit code 1 on any console error, page error or HTTP >= 400). Missing-file 404s of optional audio are expected console noise in real use.

## 6. Controllers (Xbox, PlayStation, Nintendo Switch) and button icons
Code: `web/src/gamepads.ts` (families, layouts, maps, prompt resolution; pure, unit-tested), `web/src/input.ts` (`scan()`, last device, capture),
`web/src/icons.ts` (SVG icons for the help/remap panel), `core/src/prompt.rs` (1 bpp icon glyphs, `Prompt`), `engine/src/lib.rs`
(`set_prompt`, `set_prompt_style`), tests `web/test/gamepads.test.ts`, `web/test/pads.ts`, `core/src/prompt.rs`, `engine/tests/prompts.rs`.

### 6.1 What A, B, C and START do (evidence in the code and docs)
The Genesis pad bits are S A C B R L D U (bit7..0). The game never uses them directly (except START in the title/intro/keypad): the sub CPU maps the
three buttons to *logical* bits through the pad configuration `$78BA` (table S:0xE034, `actions::Input::remap`): logical bit 5 = **confirm**,
bit 4 = **cancel**, bit 6 = **extra**. Default configuration 0 = physical A -> extra (bit 6), B -> cancel (bit 4), C -> confirm (bit 5).
| context | confirm (bit5) | cancel (bit4) | extra (bit6) | START (raw bit 7) | evidence |
|---|---|---|---|---|---|
| message box | advance page; held = fast text | - | - | - | `Engine::cmd_message` (`L_CONFIRM`), docs/re/actions.md 2.3 |
| list menus | choose the line | back one level (only below the root) | - | - | `TextMenu::input` (core/src/actions.rs) |
| alphabet keyboard / phone keypad | type the key under the cursor | backspace | - | enter / OK (keyboard only) | `menus.rs` entry (loc_011710, loc_011792) |
| pointer mode (dark room) | - | **held** + D-pad moves the view ("light"), release picks the region | - | - | `pointer_frame`; text SP26 @3732 ("Press the {EE} button together with the direction key") |
| shooting, pad | **fire** (`$7991` bit 5) | light | **draw / holster blaster** (bit 6) | - | docs/re/interact.md, shooting.md; texts SP06/SP07 ("press the {EC} button" to draw, "{F0}" to fire) |
| title, intro, options, staff roll | - | - | - | start / skip / leave a screen | `title.rs`, `timeline.rs`, `boot.rs` use the raw bit (not remapped) |
| light gun (mouse) | - | - | - | - | left button = trigger, right = second button (docs/re/shooting.md) |

### 6.2 The two layers (never mix them)
1. **Game layer** (`$78BA`, engine): function -> Genesis letter. Text codes EC = "draw blaster", EE = "light", F0 = "fire" print the letter A/B/C
   of that function: 0:(A,B,C) 1:(A,C,B) 2:(B,A,C) 3:(C,A,B) 4:(C,B,A) 5:(B,C,A) (docs/re/script.md 7, verified against SUBCODE.BIN by
   `web/test/gamepads.test.ts`). Only the title screen's option row changes `$78BA`.
2. **Web layer** (`gamepads.ts`): Genesis letter -> physical control of the device, per family, remappable. The input code never looks at `$78BA`.
The icon shown for a function = the physical control that currently produces the Genesis letter the game prints. The front end sends the
second layer to the engine (`set_prompt(style, a, b, c, start)` = style and the control id behind Genesis A/B/C/START); the engine applies the first
layer when it prints the placeholder (`Prompt::button(letter)`). `resolveIcon(config, fn, device, maps)` in `gamepads.ts` does both layers in TS
(test: all 6 configurations x 3 functions x 4 devices; also exposed as `window.snatcher.resolveIcon`).

### 6.3 Detection and layouts
`detectFamily(id, mapping)`: vendor id first (Chrome/Safari "(... Vendor: 045e Product: 028e)", Firefox "045e-028e-name" with or without leading zeros),
then name words: Microsoft `045e` = xbox, Sony `054c` = playstation (DualShock 4 `05c4`/`09cc`, DualSense `0ce6`, Edge `0df2`), Nintendo `057e`
= switch (Pro `2009`, Joy-Con `2006`/`2007`), else generic (Xbox-style icons). Unit tests use 30 real-world id strings.
Layouts: with `mapping === "standard"` (Chrome/Edge always for known pads, Firefox for known pads) the W3C positional layout is used for every family
(index 0 = bottom face button ... 9 = start, 8 = select, 12-15 D-pad). Positions get the family's names: south/east/west/north = Xbox A/B/X/Y,
PlayStation Cross/Circle/Square/Triangle, Switch **B/A/Y/X** (the Switch letters are swapped against Xbox on the same position).
Non-standard pads (`RAW` in `gamepads.ts`, e.g. Firefox/Linux for a pad it does not know) use per-family joystick-API orders; the D-pad is read from the
hat axes 6/7 and analog triggers from axes when the layout has no trigger buttons (rest value -1 or 0, "pressed" above 0.5).
| browser / OS / pad | status |
|---|---|
| standard layout logic, D-pad, dead zone, remap, all UI | **tested** with a stubbed `navigator.getGamepads` in headless Chrome 154 (chrome-headless-shell)/Linux (`web/test/pads.ts`) |
| real Xbox / DualShock / DualSense / Pro Controller in Chrome or Edge | **HYPOTHESIS** (no hardware): standard mapping assumed from Chrome's behaviour (positional) |
| Firefox/Linux raw order for Xbox (xpad: A B X Y LB RB Back Start Guide L3 R3, hat axes 6/7, triggers axes 2/5) | HYPOTHESIS from the kernel driver and the SDL controller database |
| Firefox/Linux raw order for DualShock 4 / DualSense (cross circle triangle square L1 R1 L2 R2 Share Options PS L3 R3) | HYPOTHESIS (SDL database line `a:b0,b:b1,x:b3,y:b2,start:b9`) |
| Firefox/Linux raw order for Switch Pro (hid-nintendo: B A X Y Capture L R ZL ZR - + Home LS RS) | HYPOTHESIS (weakest: the position of the capture button shifts the indices) |
| Windows/macOS non-standard layouts, Joy-Con pairs, Bluetooth quirks | not covered; the remap screen assigns whatever index the pad reports, but its icon name may then be wrong |
Stick: radial dead zone 0.35, 8 directions (22.5 degree sectors); D-pad (buttons or hat) is added. Several pads work at once (their bits are OR-ed).

### 6.4 Default map per family (Genesis button -> physical control) and why
Genesis C is confirm/fire, B cancel/light, A draw blaster under the default `$78BA = 0`; the web layer follows the platform's convention for confirm and
cancel and puts the remaining game button on the left face button.
| Genesis | Xbox | PlayStation | Switch | generic |
|---|---|---|---|---|
| C (confirm, fire) | A (bottom) | Cross (bottom) | A (right) | bottom |
| B (cancel, light) | B (right) | Circle (right) | B (bottom) | right |
| A (draw blaster) | X (left) | Square (left) | Y (left) | left |
| START | Menu | Options | + | start |
| web menu (same as Esc) | View | Share / Create | - | select |
The top face button (Y / Triangle / X) is not assigned (free for remapping). Home is never read. Keyboard (not remappable): Z/A = A, X/S/Space = B,
C/D = C, Enter = START, arrows = D-pad. If the player changes `$78BA` on the title screen, the *functions* move between Genesis letters and
therefore between buttons; the icons in the text follow automatically, the table above assumes configuration 0.
Remap screen (menu panel "Gamepad"): per family, "Assign..." waits for the next newly pressed face button, bumper, trigger or Menu/Options/+;
a control used by another Genesis button is swapped; "Reset to defaults"; stored in `localStorage` key `snatcher.padmaps` (JSON per family, validated
on load; every access is try/catch, the game works without storage). The "Button names in game text" option switches to the original letters.

### 6.5 Icons
Last used device decides the icon set and switches live: keyboard (keydown of a game key), a specific pad (any button, D-pad or stick outside the dead
zone); unplugging that pad goes back to the keyboard. A mouse click (light gun) never changes the set: a mouse has no A/B/C, so the previous set stays.
At start the keyboard set is active in the browser; the engine itself defaults to `Style::Genesis`, which native runs, the oracle comparisons and all fidelity tests keep (the engine default), so their output is unchanged
(`engine/tests/prompts.rs`: byte-identical for all six configurations).
Glyphs: `core/src/prompt.rs`, a separate table appended after the original 102 glyphs (codes 0x86-0xA5, advance 8-9 px, 0xA5 = zero-width blank):
3 families x 9 controls (south, east, west, north, bumpers, triggers, Menu/Options/+) = 0x86-0xA0, keyboard Z X C Enter = 0xA1-0xA4 (generated from
ASCII art in the source; `cargo test -p sncore --lib prompt::dump -- --ignored --nocapture` prints them). Xbox = letter cut out of a disc,
Switch = letter in a ring, PlayStation = bold cross, ring, square, triangle, keyboard = letter cut out of a keycap. Codes 0x86-0xEB do not occur in any
script text (checked over all SP files), so the originals are untouched. The glyph takes the colour of the text around it.
Where the game names a button (strings dump `snt script-strings assets/raw/files/SPnn.BIN`, searched for button/START/press/push/trigger/pad):
| place | text | icon? |
|---|---|---|
| SP06 @4579 @55cc @5903 (also {F0} = fire) @594b @5b3c @5b73 @5b9e | "... press the {EC} button" (practice, Mika/Harry) | **yes**, placeholder EC / F0 |
| SP07 @03fe @128d | "Push the {EC} BUTTON to draw your blaster" | **yes** (EC) |
| SP26 @3732 | "Press the {EE} button together with the direction key" (dark room) | **yes** (EE) |
| SP09 @0f6a @11c6 @12b0 @133d | "press the {c3}START{c1} button" (code entry) | **yes**: the word START (`FB 'START' F9`) becomes the START icon + 4 zero-width fillers (same length: offsets stay valid, `Prompt::subst_start`) |
| SP06 @4532 @5584 @57c2 @5a8d @5ace @5b03, SP07 @03ca @1257 | "GUN'S START BUTTON" / "JUSTIFIER'S START BUTTON" (gun owners) | no: names the light gun's own button (the mouse); text stays |
| SP06 @703c, SP19 @1a31 @1b3d @1c34, SP20 @2159 | "rapid fire button" of the control pad | no: a switch on the original pad, not a game button |
| SP29 @007d/@0794/@080b/@08a1, SP30 @0809/@0ad3 | "PRESS BUTTON", "this button" | no: buttons of in-game machines |
| title screen "PUSH START" and options/pad-configuration rows (A,B,C) | no such string in SUBCODE.BIN, IP.BIN or any SP file (`grep -a -i "push start\|press start\|button"` only hits SP*.BIN); the options window ("OPTIONS", "DEVICE", the A/B/C configuration rows) is drawn from tile graphics and sprites (`title.rs`, objects 32-36; seen in the player_path frames) | **later**: would need new tile art or text drawn over it |
| light-gun calibration screen, other module screens | graphics | later |
Help panel (`web/src/main.ts` `renderControls`): table of the active device with inline SVG icons (own shapes, no icon packs).

### 6.6 Rumble (not implemented)
The engine emits no semantic event for "gun fired" or "player hit": `shoot.rs` only emits `Out::PcmSfx{id, bank}` with per-stage ids (`T_SHOT_SFX`,
`T_ATTACK_SFX`, `player_hit` at shoot.rs:663). Per the task rules no engine logic was added. Later item: add `Out::Rumble{kind}` at the shot (shoot.rs ~361)
and `player_hit`, forward it in `lib.rs`, and call `gamepad.vibrationActuator?.playEffect("dual-rumble", {duration:80, strongMagnitude:.4, weakMagnitude:.8})`
with feature detection and a menu toggle.

`scripts/test-web.sh` now takes about 15 minutes (`web/test/pads.ts` opens ~20 pages and waits for the typewriter each time).

### 6.7 Manual checklist (per controller)
1. Plug in / pair the pad, open http://localhost:5300/, press any button on it: the "Controls" list (menu panel or start overlay) must switch to the pad, the status
   line shows "gamepad: <family>", the panel "Gamepad" lists it (and "non-standard mapping" if the browser has none).
2. Play: confirm button advances text, cancel button backs out of menus, D-pad and left stick move the menu cursor; View/Share/- opens the web menu, again closes it.
3. Icons: `?scene=SP07:159` (or play to the shooting practice) - "Push the [icon] BUTTON to draw your blaster" must show the icon of the button that
   draws the blaster (default: left face button: Xbox X, PlayStation Square, Switch Y); `?scene=SP09:...` code entry shows the Menu/Options/+ icon.
4. Remap: menu -> Gamepad -> Assign... for "Draw blaster", press another button; the text icon changes at the next message; reload keeps the map;
   Reset to defaults restores it.
5. Reconnect: unplug (icons return to the keyboard set), plug again and press a button (icons return).
6. Report: browser + OS, `Gamepad.id` shown in the panel, and which buttons were swapped (Firefox raw layouts are HYPOTHESES).
