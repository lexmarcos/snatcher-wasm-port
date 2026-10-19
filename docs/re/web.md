# WEB workstream – browser front end

Code: `web/index.html`, `web/src/{main,sources,audio,input,pacer,saves,touch,touchpad,touchgame,touchmath,platform,crt,crtmath,display}.ts`, `web/server.ts`, wasm API in `engine/src/lib.rs`
(`Game`: new, load_file, boot, new_game, debug_jump, set_save, set_voice_len, set_fm_len, set_sfx_len, set_gun, tick,
framebuffer_ptr, take_events, debug_info). Build `scripts/build-web.sh`, tests `scripts/test-web.sh`.

## Status
- **Design system** ("Junker HQ terminal", `docs/design-system.md`): all HTML screens (start overlay with every importer state, pause panel with tabs, toolbar, monitor frame, debug overlay) use `web/styles/system.css` and the self-hosted DotGothic16 (`web/fonts/`, served as `/fonts/`, `/styles/`). Checked by `web/test/design.ts` (run by `scripts/test-web.sh`). A static host must serve `web/styles/` and `web/fonts/` next to `index.html`, `dist/` and `pkg/`.
- **Disc image import** (`.cue` + `.bin` picked or dropped, checked, extracted in a worker, cached in OPFS, played with no ogg step): done and tested, section 8.
- **Done and tested** (`web/test/ui.ts`, `pace.test.ts`, `sources.test.ts`, `shot.ts` with `newgame` and `scene=SP01:0`, no console errors):
  start overlay (served assets or folder picker), menu (Esc, pauses), volume sliders, fullscreen, mute, pause on hidden tab,
  smoothing and debug toggles, save export/import, save events reach the store and the engine, mono/stereo, FM/PCM stop
  commands, LRU buffer cache, visible notes for missing optional assets, phone-width layout.
- **Display filter** (picture styles: consumer TV, studio monitor, scanlines only; WebGL2, display only): done and tested, section 12.
- **Touch controls** (phones and tablets): done, section 11 (on-screen pad and touch the game, layouts, Touch tab, wake lock, audio unlock, manifest). Checked in emulation only, see 11.9.
- **Not done**: real-ear check of the mono/stereo option
  (verified structurally and numerically, see 4), audio of `kind==2` voice music ducking (audio.md §5 says ~1 dB, ignored),
  the RF5C164 voice `pan` nibbles (order unknown, audio.md "Open questions"; voices play centred).
- **Needs the coordinator / engine owners**: `engine/src/menus.rs` `sfx_1c` emits `Out::Pcm` but its target is the FM effect
  (queue command 0x1C = FM sfx, S:0x1013E; game.rs `0x1C => fm_cmd`); the front end plays `pcm` events as FM effects. If
  `sfx_1c` is changed to `fm_cmd`, the `pcm` case in `handleEvent` becomes dead. `docs/re/modules.md` still says "mono/stereo
  option has no audible effect yet": it now has one in the front end (below).

## 1. Asset sources (`sources.ts`, `imported.ts`)
Three interchangeable `Source`s (the third, **imported**, is the player's path: section 8); the two below (game files, HEADER.BIN/IP.BIN, audio files by path relative to `assets/`):
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
stay in memory and the menu says to export them. The autosave (section 9) is a separate store and is not part of this export.

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
| import, pick -> Play enabled (header check, 99 files, 1197 voices, banks, drums, FM manifest, 486 MB of CD audio copied, SHA-256) | 13.7 s cold, 3.6 s with the image in the OS file cache | 12.5-12.8 s |
| same import natively (`snt import-disc`, warm disk cache) | 2.2 s | |
| FM audio of all 122 sounds, rendered in the worker in the background | 77.6-77.7 s | 77.6-79.4 s |
| same natively (`snt audio-fm`, all 122) | ~90 s | |
| browser RSS (whole process tree): idle page / peak during the import / peak over import + FM + 600 frames of play | 538-544 / 778-844 / 1067-1117 MB | 541 / 773-799 / 978-1146 MB |
| wasm heap of the worker: after the import / after rendering all FM (largest sound, then the worker is terminated) | 17 MB / 257 MB | 17 MB / 257 MB |
| cache in OPFS (files 2 MB, voices 81, effects 2, CD 486, FM 538, manifests) | 1086 MB | 1086 MB |
Decision on FM: a blocking full render would add 78 s to the first visit, a lazy per-sound render would make the first use of every long song wait
2-10 s every session. Rendering everything once in the background with on-demand priority gets the player into the game after the 13 s import, and
the cache makes every later visit instant. The longest single sound (6 minute loop, song 6c) takes 7 s natively and the whole set is not slower in the worker than natively
(78 s against ~90 s), so a request for a sound that is not rendered yet waits for the one in progress (7 s at worst) plus its own render; the
background queue starts with the shortest sounds.
Memory: the image is read in pieces of 600 KB, so the import does not grow with the image size: the worker's wasm heap is 17 MB after the import (257 MB after the
longest FM render, released when the worker ends), the rest of the +240-300 MB over the idle page during the import is V8 / Chrome buffering of the pieces and of the OPFS writes. During play the
renderer also holds the decoded `AudioBuffer`s (a 7 minute CD track = 148 MB as float32; the LRU in `audio.ts` caps them at 320 MB).
Bit-exactness is checked in the browser as well: CD track 04, voice 41 and FM effect 01 (rendered in the browser) equal the reference WAVs sample
for sample, both after the import and after a reload from the cache.

## 9. Autosave (`autosave.ts`, `engine/src/snapshot.rs`, `core/src/snap.rs`)
The page saves the running game by itself at quiet moments, keeps it apart from the four save slots, and offers "Continue where you left off" in the start window.
On by default; the check box "Autosave" is in the pause panel, tab Saves.

### 9.1 Design chosen, and why
What the game can do itself, and why it is not enough: the game's own SAVE writes the 0x1C0-byte block `$9C00..$9DBF` (S:0x14F12 `snapshot_full`, `Vm::snapshot_full`): play clock,
button config, script id, the return pc of the SAVE node, the history stack, scene id, music id, 64 bytes of object flags, the story variables. CONTINUE (`Vm::from_save`) rebuilds a *scene*
from that: it reloads the script and the scene and starts the music; the VM stacks, the menu path, the animation phases, the text window, the CD clock, the palette fades and the message
state are not in the block, and a block can only be made where the script runs its SAVE node. It cannot restore the command menu the player was in.
Options considered: (a) the game's block at every menu: not restorable, see above; (b) record the pad input from the start of the session and replay it at the next visit: the engine is deterministic
(oracle-exact), but the replay grows with the play time (an hour is 216 000 frames), breaks when the engine changes, and the language / subtitle / audio-length settings would have to be replayed too;
(c) **a snapshot of the whole engine state at a safe point, chosen**. The engine state is plain data (`Vm`, `Display` with the VDP and the object runtime, text window, message box, menu, registers, counters),
about 54 KB for a screen, and it is exact: `engine/tests/snapshot.rs` proves that a restored engine runs on frame by frame like the original.
`core/src/snap.rs` is a small codec (no dependencies, no derive): `snap_struct!` lists every field and destructures without `..`, so a field added later and not stored (or not skipped on purpose) is a compile error.
Zero runs are stored compactly (a VRAM image is mostly zeros).

What is stored (`snapshot.rs`, `engine_state!`): `Display` (VDP VRAM/CRAM/planes/sprites/raster, the object runtime, palette and tile channels, scroll, shadow palette, main-CPU display variables),
`Vm` (script position, all six stacks, story variables = the save block, RNG), the sequencer (queue, step, timer), message box, text window bitmap and renderer, list menu and its cursor / pending move / closing state,
the `Regs` map (every register the engine mirrors, the object snapshot bytes), the frame counter and play clock, the CD-DA start frame, voice / effect / jingle counters, the PCM bank, `Ix` (typed text, gun calibration flag, PCM channel ends),
the `Input` state (held / pressed / repeat counters, button config number), the mapped DATA file number (the file itself is mapped again and pt-BR-patched), the script file number (the file is loaded again), extra mapped regions,
and what the front end is playing (`fm_music`: id and start frame, `fm_loops`: looping FM effects).
Not stored: the disc files and what is built from them (fonts, names, the script and DATA bytes), what the front end sets (button prompt style, voice / FM / PCM lengths, save slots, gun readings, the subtitle option, the language), the frame buffer
(drawn again after a restore), pending events, the title / intro / ending modules.

### 9.2 Safe points: when it saves
`Engine::safe_point()` (reasons: `safe_check()`, shown in the debug overlay):
1 = **the command menu waits for a key** (queue idle, no cursor move, no closing effect); 2 = **a message box waits for the confirm button** (text fully drawn).
Never at a safe point (the last safe point stays): a voice clip or PCM effect playing (a front end cannot start a clip in the middle), a finite FM jingle or effect playing (looping FM music and effects are started again),
the text-entry keypad / keyboard, pointer moves, shooting stages (a restore of the shooting state is not covered by the tests), a picture or screen reset still loading, a subtitle on screen, the title, the intro and the ending timelines, `sub_stall`.
Front end (`Autosaver.poll`, once per frame, one call into the engine): a *rising edge* into a safe point marks a wanted save. It is written at once when the scene or script changed since the last save (at most once every 3 s: `MIN_GAP_MS`),
otherwise when the last save is 30 s old (`EVERY_MS`). A page that is hidden or closed (`visibilitychange`, `pagehide`) saves the quiet moment it is at (at most once a second); if it is not at one, the last safe point stands.
Nothing is saved while the start window or the pause panel is open (the game is not running) or while autosave is off.

### 9.3 Where it lives
IndexedDB, database `snatcher.autosave`, store `slot`, key `latest`: one record `{v, stamp, act, play, kind, pt, mono, frame, data}` (`data` = the snapshot, about 40-60 KB). Not localStorage: it is too big for that and not text.
The on/off choice is `snatcher.autosave.on` ("0" = off, anything else or missing = on) in localStorage. Both may be blocked: with localStorage blocked the page still works (default on, the choice lasts for the visit);
with IndexedDB blocked (`indexedDB` missing, private windows of some browsers) the autosave is kept in memory for the visit and the page says so (a warning note and a line in the Saves tab: "Browser storage is blocked: the autosave lasts only until this page closes").
The four save slots (`snatcher.save.N`) are never read or written by the autosave. **Export / import of saves does not include the autosave**: the export is the game's own portable save block (any build can read it);
an autosave is tied to one engine layout and one disc (it would be refused after the next engine change), is ten times bigger and is a convenience, not a save.
"Clear everything" (start window, `btnWipe`) deletes the database as well as every `snatcher.*` key; "Remove imported data" (`btnClear`) removes only the disc and **keeps the autosave** (`web/test/import.ts` checks both).
Turning autosave off keeps the existing autosave (Continue is still offered); the Saves tab and the start window's Saves row have a button "Delete autosave" with a second click to confirm. Nothing is deleted silently.

### 9.4 Restore and its fidelity
"Continue where you left off" (`btnContinue`): the page switches to the snapshot's language first (the screen and the message text offsets are language specific), starts the engine with the disc files, installs the pack, then `Game.restore(bytes)`
replaces the state and the page restores the sound mode (`audio.mono`, saved with the record: it is audio state, not engine state). The first tick continues from the saved frame.
Exact (checked frame by frame, pictures and engine events, English and pt-BR with subtitles, 9 scene roots of acts 1-3 plus a new game, menu and message points, `engine/tests/snapshot.rs`; and in the browser, same engine frame and same screen hash, `web/test/autosave.ts`):
the picture, the script and menus, text window, the cursor, palette fades in progress, scroll and effects, RNG, the play clock, all counters. A restored message box can be confirmed at once.
Approximated:
- CD-DA music starts again at the engine's CD clock (`Out::MusicAt`, seconds since the track started): the audio and the engine clock agree to the usual drift correction (docs/re/audio.md, `cdclock.ts`); the first moments wait for the file to be decoded, the page starts "late" like on any track change.
- FM music starts at the position the engine's frame count gives (`Out::FmAt`, wrapped into its loop). The engine does not know where the FM driver really is, so the position can differ from the original run by the usual FM timing error (the frame count is the game's clock; the rendered song is the driver's output); it is a musical position near the right one, not a sample-exact one.
- Looping FM effects start from their beginning.
- Held buttons: the pad state is stored, but a key held through the reload is a new press for the browser.
Not restored by design: a voice clip in progress, a jingle, shooting. They are never saved, so Continue goes back to the last quiet moment before them (up to 30 s earlier than the exit, unless the page was hidden at a quiet moment).
Known difference: an autosave is one slot; Play from the title and playing on will replace an older autosave at the next quiet moment (the start window says which autosave Continue would load).

### 9.5 Versioning and bad autosaves
Header (`snapshot.rs`): magic `SNAS`, format number (`FORMAT`), **layout fingerprint** (FNV of the type and field names of the stored state, computed by writing a default engine; any added, renamed or removed field changes it, so a deploy that does not touch the state keeps
players' autosaves), **disc digest** (names, sizes and a sample of every loaded disc file; a re-dump of the same disc gives the same value), language flag and **translation pack hash**, kind, script, scene, play time, frame, payload length, and an FNV checksum of everything.
The page checks format, fingerprint and checksum when it lists the autosave (`snapshot_header`), and disc, language and pack when it restores. A bad one is removed and the start window says why, in plain words:
another engine version ("came from another version of the game engine"), another disc, another language or translation, damaged. Never a crash: `Engine::restore` decodes everything first and changes the engine only when all of it is valid and the needed files exist
(`snapshot_refuses_and_reports` also checks that a failed restore leaves the engine unchanged). The game then stays on the start window (Play is available).

### 9.6 Cost (measured, headless Chrome, this laptop, `bun web/test/autosave.ts`)
Size: **33-63 KB per snapshot** (54.6 KB at the first message of a new game; the range is over about 60 safe points in `engine/tests/snapshot.rs`; zero runs are already folded out, the VRAM image and the text window dominate).
Time of one save in the page: **engine side (`snapshot()` + `snapshot_info()` through wasm) 4.5 ms for the first save of a visit (cold code), 1.2-1.4 ms for the next ones; whole save including the IndexedDB write 7.3 ms for the first, 3.8 ms for the next ones**
(one 60 Hz frame is 16.7 ms; a save happens at most once every 3 s and only at a quiet moment, so no frame is missed in practice: a quiet moment is the game waiting for a key). The page keeps the numbers (`autosaver.stats`: saves, last / worst time, size) and the debug overlay (F1) prints them
("autosave: on, safe point, saved 2x, last 53767 B in 3.8 ms (engine 1.20 ms)"). Reading the header of a stored autosave at page load (`snapshot_header`) is a checksum over 55 KB. Natively (release build, `SNAPTIME=1 cargo test --release -p snengine --test snapshot -- --nocapture`): `snapshot()` 0.25-0.5 ms, `restore()` about 1 ms (decoding, mapping the DATA file again, drawing the frame).

### 9.7 Tests
`cargo test --release -p snengine --test snapshot`: round trip at the first menu, the first message wait and points after scene changes in a new game (en and pt-BR with subtitles), 9 scene roots of acts 1-3 (FM and CD music, voices, 6 to 12 points each): after `restore` the next 600 to 700
frames equal the original run in picture, engine events and (every 50 frames) full state bytes; the audio a restore starts again equals what the live engine was playing; refusals (not a safe point, other language, other disc file, other layout, other format, damage, truncation, a failed restore changes nothing).
`core/src/snap.rs` unit tests (round trip, corrupt input, fingerprint). `web/test/autosave.ts` (headless Chrome, own server on 5340): first save in a new game, the record in IndexedDB and not in localStorage, save slots untouched, Continue (en, pt-BR) with the same engine frame and screen hash and the music back,
hiding the page saves, toggle off = no new autosave although safe points go by, the setting persists, delete with a second click, "Clear everything", other engine / other disc / damaged autosave, localStorage blocked, IndexedDB blocked. Screenshots in `work/shots/autosave/`.

## 10. Static build and GitHub Pages (`scripts/build-static.sh`, `.github/workflows/pages.yml`, `web/src/urls.ts`, `web/test/static.ts`)
The page is published at https://markzuel.com/snatcher-wasm-port/ (project site of `lexmarcos/snatcher-wasm-port`; the owner's user site serves the custom domain, this repo sets none). The site holds the engine, the page and the pt-BR pack only: no game data, no `assets/`, no source maps. The disc is imported by the player in the browser (section 8) and never sent anywhere.

**Sub-path.** Every URL is relative to the page: `index.html` uses `dist/main.js`, `styles/system.css`, `fonts/...`; code builds the others with `siteUrl("pkg/snengine.js")` (`urls.ts`: `document.baseURI` in the page, the worker's own URL one folder up in the worker). The dev server still serves the same paths at the root. `--external /pkg/snengine.js` is gone from the bun build: the engine is loaded by a computed `import()`, which bun leaves alone. The wasm file is passed to `init({module_or_path})` explicitly, so it gets the version query too.

**Headers.** Pages sets none of the dev server's. Nothing needs cross-origin isolation or SharedArrayBuffer (checked: no use in `src/`, and `static.ts` asserts `crossOriginIsolated` is false). `.wasm` is served as `application/wasm` (streaming compile works); the pack is read with `arrayBuffer()`, so its content type does not matter.

**No probe on Pages.** The dev server serves `./assets` and the page asks for `assets/raw/filelist.tsv`. On a static host that is a 404, and Chrome prints every 404 in the console. `build-static.sh` therefore adds `<meta name="sn-served-assets" content="none">` to the published `index.html`; `probeServed()` returns at once when it is there (no request, no "no local assets" line under "Other ways to load"). The dev page has no such meta and probes as before. The page has an inline favicon, so there is no `favicon.ico` request either.

**Cache.** Pages caches for 10 minutes, so a deploy could pair an old wasm with a new JS. `build-static.sh` hashes the whole site (html, js, wasm, pack, strings, css; the build is deterministic) and writes the first 12 hex digits into the `@@BUILD@@` token of the bundles and into `main.js?v=` / `system.css?v=` in `index.html`. `siteUrl()` appends `?v=<hash>` to the glue, wasm, worker, pack and strings URLs. In dev the token stays and no query is added. Fonts keep plain URLs (they never change). `.nojekyll` is in the folder.

**Build.** `scripts/build-static.sh` runs `scripts/build-web.sh` (`SKIP_BUILD=1` reuses the last build) and fills `build/site/` (gitignored, about 1.8 MB: wasm 0.87, pack 0.72, js 0.11, rest small). It stops if a token is left, `index.html` has an absolute path, or the folder holds a `.map`, `.bin`, `.cue`, `.chd`, `.iso`, `.ogg` or `.wav`.

**Deploy.** `pages.yml` runs on push to `main` and by hand: Rust stable + wasm32 target, `wasm-bindgen` 0.2.129 (prebuilt release, same version as `Cargo.lock`), Bun, `scripts/build-static.sh` (`cargo build -j 4`), then `upload-pages-artifact` and `deploy-pages`. Repository setting: Pages source "GitHub Actions".

**Check.** `bun web/test/static.ts` serves `build/site` under `/snatcher-wasm-port/` with a plain server (no special header, 404 for the rest) on port 5350 and checks: 0 console errors, 0 failed requests, no `/assets` request, step 1 only on a first visit, pt-BR switch. `FULL=1` also imports the local disc and plays (about 35 s for the import, the pack fetched, frames drawn). `SITE_URL=<url>` checks a live site instead. Screenshots go to `work/shots/pages/` (gitignored: they show the game).

## 11. Touch input for phones and tablets (`touch.ts`, `touchpad.ts`, `touchgame.ts`, `touchmath.ts`, `platform.ts`, `engine/src/touch.rs`)
Two ways to play on a touch device, chosen in the pause panel (Touch tab) and kept in `localStorage` (`snatcher.touch.mode` = `pad` | `game` | `off`).
Default: `pad` when the main pointer is a finger (`(pointer: coarse)` and `maxTouchPoints > 0`: phones, tablets, iPadOS), `off` on a computer (also a laptop with a
touch screen: the player can switch the pad on). `off` leaves the old behaviour (the browser's made-up mouse events still drive the light gun). Nothing changes on a desktop.
Both modes feed the **existing input path**: `Input.touch` (a `TouchSource`) is ORed into the pad bits of every game tick in `Input.poll()`, exactly like `keys` and the
gamepads, and replaces the mouse as the light gun while a finger aims (`Input.poll` -> `Game.set_gun`). No new engine input path. A finger makes `Device` = `touch`
(`Input.touchDevice()`): the game text then keeps the original letters A B C and the word START (`promptFor` -> style 0), which are the letters drawn on the pad.

### 11.1 Mode 1, on-screen pad (`touchpad.ts`)
D-pad (left), A B C (right, C = confirm carries the yellow), Start and Menu above them. Square beveled blocks of the design system (docs/design-system.md "Touch controls"),
at least 48 CSS px, 56 px by default (Touch tab: size 90-140 %, opacity 30-100 %).
* Multi-touch: every finger is tracked (`pointerdown/move/up/cancel`, pointer capture on the pad root); the bits are the union of all fingers.
* D-pad: a finger that starts on it steers it wherever it goes (like a thumb stick): offset from the centre -> 8 directions of 45 degrees with a dead zone
  (`dpadBits`, the same sector test as the gamepad stick). Diagonals set two bits (the game's own pad aiming in the shooting stages uses them).
* Buttons: hit-tested on every move with 6 px of slop, so sliding from C to B presses B and releases C, sliding off releases, a held button stays held.
* Nothing can stay pressed: `pointerup`, `pointercancel`, `lostpointercapture`, `touchcancel`, `pagehide`, a hidden page, a lost window focus and any modal
  (`Input.enabled = false`) release everything. A press shorter than one game tick is latched for one tick.
* Menu is a normal button (opens the pause panel). Start is a held pad bit.
* **Light-gun sections in this mode**: the pad plays them the way a pad plays (the engine's own pad aiming: the D-pad picks one of the 3 x 3 target cells, A draws or
  holsters the blaster, C fires; frame-exact against the original, docs/re/shooting.md). No extra code: chosen because it is the original pad behaviour and needs no
  aim by pixel. Tapping the picture in this mode does nothing (a finger on the picture is not a mouse: `pointerdown` is cancelled so the browser makes no mouse events).

### 11.2 Mode 2, touch the game (`touchgame.ts`)
No pad is drawn. The picture is the controller. A small bar keeps **Menu** and shows **Back**, **Start/OK** and **Holster** only while they do something.
The engine tells the page what the game waits for and where the tappable things are (11.4); the page turns taps into presses (2 ticks down, 2 ticks up, one at a time,
waiting for `ready`, because a press made while the game redraws a menu is lost). Pointer events, `touch-action: none` on the game surface, `-webkit-touch-callout: none`,
`user-select: none`, `overscroll-behavior: none` (no pull to refresh), `touch-action: manipulation` on the rest of the page (no 300 ms delay, no double-tap zoom), `gesturestart`
cancelled (iOS pinch), `contextmenu` cancelled.

**Every state found and how it is played** (the list comes from `Engine::game_touch` / `Title::touch` / `Timeline::touch`; each kind has a test):
| State (engine `kind`) | Where it is | Touch solution |
|---|---|---|
| boot screens (copyright, Konami logo), fades, scene loads, scenes that play (`none`) | before the title, between scenes | nothing is needed; a touch holds the confirm button, which is harmless there (and is the safety net for a wait I did not find) |
| message box, printing or waiting for the button (`message`) | every dialogue, narration, system text | the finger holds the confirm button while it is down: a tap advances, holding prints the text at once (the game's own held-C fast text). Anywhere on the page, the thumb need not aim |
| command menu, list of lines (`menu`) | Junker HQ, every investigation menu, save slots, the debug menu | tap a line: the cursor walks there (Up/Down presses, waits for `ready`) and the game confirms it. Page arrows of an 8-item menu are lines too: tapping one flips the page. The tap is hit-tested with 5 px of slop between the 9 px lines |
| menu depth > 0 (`menu`, `back`) | sub-menus | Back button or a two-finger tap = the cancel button. Not shown at depth 0 (the game ignores cancel there) |
| dark room free view (`menu`, `pointer`; SP26 only, `$79FE == 1`) | "TURN ON LIGHT" in SP26 | touch and drag: B held plus the direction of the drag from the touch point (a stick), the light moves while the finger is down, lifting lets go and the game picks the region under the light. The game text says "press the B button with the direction key": in this mode that is the drag |
| phone keypad (`entry`, style 2) and alphabet keyboard (style 3) | videophone dialling (SP35), Jordan terminal I.D. file (SP04/SP05) | tap a key: the cursor walks there over the keyboard's own grid (shortest route, `Game.touch_step`) and the key is typed. The keyboard's OK and backspace keys are keys too. Back = backspace (the game's cancel in an entry); the OK button = Start = Enter (not in the dialling keypad, which ends by itself) |
| shooting stage (`shoot`), also its practice range | the 15 stages | tap = aim at that game pixel and fire (the mouse light-gun path: `Input.touch.gun()` -> `set_gun`); the first tap also draws the blaster (draw, then fire); the Holster button holsters (the gun's second button). The shot lands on the tapped pixel (test: within 2 px) |
| title menu (`title`: NEW GAME / CONTINUE) | after the logo | tap an entry: the cursor goes there and Start is pressed |
| slot screen (`slots`) | CONTINUE | tap a slot that has data (empty ones are not tappable): Down/Up to it, then Start. Back (B) returns to the title menu |
| options screen (`options`) | after NEW GAME / CONTINUE | tap a row: the cursor goes there and the value changes (DEVICE, CONTROLS = the 6 button configurations, SOUND MODE); GUN ADJUST (only with the gun device, never reachable with fingers) and QUIT start the game. `ui.confirm/cancel/extra` give the physical A B C bits that make the game's logical confirm / cancel / extra under the chosen configuration (`Engine::touch_roles`), so the touch roles stay right after the player changes it |
| gun calibration (`calibrate`) | only reachable with a gun device | tap = trigger at that point, Start button skips |
| intro (`skip`) and any sequencer wait for Start (`skip`) | the opening sequence | a Start button appears; **no tap on the picture skips** (a stray touch must not skip a scene). The late-game sequence takes no input |
| GAME OVER, QUIT INVESTIGATION | the game restarts at the title by itself | nothing to do |
`Back` is also a **two-finger tap** (two fingers down within 450 ms, lifted again without moving).
Nothing needs the pad in this mode, so no pad is ever shown in it. If the player ever meets a wait this table does not cover, the Touch tab switches to the on-screen pad.

### 11.3 Wasm API added (engine, `engine/src/touch.rs`; read-only, not part of a snapshot)
* `Game.touch_ui() -> JSON` `{kind, ready, back, start, label, cur, page, confirm, cancel, extra, pointer, drawn, playing, rects:[{x,y,w,h,id,k}]}`. Rectangles are in game pixels
  (the 256x224 frame buffer). Menu lines: x 8..248, y = `0x8F` (+16 with the portrait layout `$7893`) + 9 per line; keypad keys 16x16, keyboard keys 16x16 with wider BACKSPACE
  (48), SPACE (64), OK (48), centred on the cursor sprite's cell; title items, the 4 slot panels and the 5 settings rows are measured constants of the baked pictures
  (`Title::touch`). The Rust tests check them against the screen (text pixels inside each line, glyph centroids on the keypad keys).
* `Game.touch_step(cell) -> bits` first step of the shortest route of the keypad/keyboard cursor to a cell (breadth first search over the grid codes; `Entry::dest` is the side-effect free
  form of the original `sub_0119E0` walk, which `mv` now uses too).
* test helpers: `Game.menu_items()` (texts of the open menu as JSON), `Game.entry_text()` (what is typed), `Game.peek_reg(addr)` (a sub-CPU word register: `$7908` shots, `$798A/B` gun position,
  `$78F8` blaster drawn, `$96D0` view, `$9716` remembered menu cursor).
Tests: `cargo test --release -p snengine --test touch` (6 tests against the local disc, skipped without it; overlay pictures go to `work/shots/mobile/engine/`).

### 11.4 Coordinates (`touchmath.ts`)
`clientToGame(clientX, clientY, canvasBox)` maps a client point to game pixels with `object-fit: contain` semantics (bars when the box is wider or taller than 256:224; a point on a bar is
`inside: false` and clamped to the picture's edge pixel). The canvas is sized in whole device pixels by `fit()` (crisp pixels also with `image-rendering: auto`, "smooth"), so the
mapping is linear. `Input.track` (mouse) uses the same function.

### 11.5 Layout (`planLayout`, `touch.ts` `fit()`, CSS section 6 of `system.css`)
`<html data-touch="pad|game" data-arr="below|sides|overlay">`. The picture gets a whole device-pixel scale, except on a phone held upright (`full`, `<html data-full>`): there it spans the whole width at a fractional scale, edge to edge with no side padding or bezel (in whole device pixels, so it never pokes out by a rounding; with Plain pixels its columns differ by one device pixel at most).
* Portrait (phones, tablets): picture on top, controls at the bottom (D-pad left, A B C right, Start above them); Menu centred just under the picture (`placeMenu`, `<html data-menu="under">`; back in the left column when the gap is too small). While playing the page around the picture is black (`<html data-play>`).
* Landscape: `sides` = controls in the free space left and right of the picture (the largest scale, not below 3/4 of the largest that fits, whose side columns hold the controls; the buttons shrink
  to the room but never below 48 px); `overlay` = the narrow phones (667x375): the controls are drawn over the picture's edges with 48 px buttons and the picture keeps its larger scale.
* Safe areas: `env(safe-area-inset-*)` (read from the hidden `#sai` probe for the maths, mirrored in `--sai-*`); controls and picture stay inside (test with the protocol's safe-area override).
* The toolbar, the monitor bezel and the canvas bevel lines are not shown in touch layouts (any orientation). `100%` of a fixed `body` instead of `100vh` (iOS toolbars). Orientation: both work; a phone in portrait whose picture
  would be larger sideways sees a dismissible hint (never a block, `snatcher.touch.rotate`). Fullscreen: Fullscreen API (Android, iPad); iPhone Safari has none, so the Touch tab
  says "Share, Add to Home Screen" (`manifest.webmanifest`: `display: fullscreen`, `display_override`, icons made by `scripts/make-icons.py` from the page's own pixel S, no game art). **No service worker**.

### 11.6 Platform pieces (`platform.ts`)
* Screen wake lock while the game runs (`navigator.wakeLock`), released on pause / menu / hidden page and taken again; default on for touch devices, off on computers (Touch tab).
* Haptics `navigator.vibrate(8)` on pad presses and touch-mode presses; off by default, Touch tab (shown only where supported).
* Audio: `primeAudio()` creates and resumes the `AudioContext` inside the click of Play / Continue (before the awaits that load the game) and plays a silent buffer; `AudioOut` takes it over.
  Later taps (`touchend`, `click`, `pointerup`) resume a context the browser suspended or interrupted, unless the page itself paused the audio. `visibilitychange` still suspends.
* Import on phones: the file picker has no `accept` filter on touch devices (iOS Files knows no type for `.cue`/`.bin`; the page validates the pick itself, section 8) and a one-line help is shown.
  The storage line is extended: before any file is picked, `imp.lowquota.*` warns when `navigator.storage.estimate()` leaves less than the import needs (about 1.2 GB with the CD music, 735 MB
  without). The import format is unchanged.

### 11.7 Settings
Touch tab: mode (On-screen pad / Touch the game / Off), button size, pad opacity, vibration, keep the screen on, Fullscreen, iPhone advice. Keys: `snatcher.touch.{mode,size,opacity,haptics,awake,rotate}`
(all `snatcher.*`: "Clear everything" removes them). Strings: `i18n.ts` (`t.*`, `ctl.t.*`) and `i18n/pt-BR/ui.json`. The Sound tab gained a Mute check box (the toolbar is hidden on phones).

### 11.8 Tests
`bun test web/test` (`touchmath.test.ts`: 38 tests: coordinate transform with letterboxing, D-pad angles, hit-testing of menu rectangles, layouts of 10 device shapes, settings),
`bun web/test/touch.ts` (headless Chrome, device emulation iPhone / Android / iPad portrait and landscape and a 667x375 phone, real multi-touch through `Input.dispatchTouchEvent`, own server on 5360;
screenshots of every layout in both modes in `work/shots/mobile/`, gitignored, they show the game). It covers layouts (no horizontal scroll, controls and picture inside the safe area and apart,
buttons >= 48 px), the pad (multi-touch, diagonals, dead zone, sliding, held, cancel, hidden page, blur, menu, prompts, wake lock, haptics), touch the game (paging arrow, command line, Back by two fingers and by the
button, a shooting stage by taps with the shot on the tapped pixel, message tap, keyboard key and Back = backspace, dark-room drag, title -> options (value cycling) -> intro skip), the settings (default, persistence,
desktop unchanged, pad switched on by a desktop player), the 360x640 start window and panel tabs, audio unlock, haptics. `scripts/test-web.sh` runs it.

### 11.9 What was only checked in emulation (try it on a real phone / tablet)
iOS Safari audio unlock (the context is primed in the tap; emulation cannot prove iOS accepts it), the screen wake lock (stubbed in the test), `navigator.vibrate` (stubbed), the Files picker on iOS and the Android
picker with several files, Fullscreen on iPad / Android and Add to Home Screen on iPhone (and how `display: fullscreen` behaves), real notch / home-bar insets (protocol override only), thumb comfort of the pad (size,
the A B C arrangement: A B on top, C below B), the pinch / long-press / pull-to-refresh behaviour of each browser, `storage.estimate()` quotas on phones, performance on a low-end phone.

## 12. Display filter: picture styles (`crt.ts`, `crtmath.ts`, `display.ts`, `touchmath.ts`; tests `crtmath.test.ts`, `crt.ts`; harness `crt-frames.ts`, `crt-compare.ts`, `crt-perf.ts`)

Display only. The engine's 256x224 frame is not touched: `main.ts` still copies every frame into the 2D canvas `#screen` (tests, autosave hashes, the plain picture and the fallback all read that), and when a CRT style is on a second canvas `#crt`
(inside `#stage`, `position: absolute`, `pointer-events: none`) is laid exactly over it. Pointer events keep going to `#screen`. Written from the physics of a tube; no code from crt-royale, crt-guest, crt-geom, crt-lottes or any other
shader (nothing to credit, nothing borrowed; the maths is: a box seen through a gaussian, a gaussian beam, the NTSC YIQ matrix, the sRGB curve, the tanh form of the normal CDF).

### 12.1 What the page does
- **Styles** (Display tab, `snatcher.crt`): Plain pixels (the old behaviour: whole-number scale, square pixels, the Smooth scaling check box), CRT: consumer TV, CRT: studio monitor (PVM), Scanlines only, and Custom (any slider moved). Pixel shape: TV 8:7 or square. Adjust fold: Scanlines, Mask, Glow, Sharpness, Composite blend, Curvature, Mask type, Reset. `?crt=off|tv|pvm|scan` picks a style for one visit without saving (and allows software GL, see 12.7).
- **Defaults**: a computer gets the consumer TV, a phone or tablet (finger as main pointer) gets Scanlines only (`defaultPreset`). Reasons: the TV style is what the art was made for (the title screen's noise is dither that composite video blends away) and a computer picture is 4.5 to 6 device pixels per game line, where the slot mask and the glow resolve; a phone picture is 3.4 to 3.7 device pixels per line (412 CSS px at DPR 2.6), where a mask cannot resolve (it fades out by itself anyway) and the glow and the composite blur cost battery for little, so the slim style (lines, a little glow, no mask, no blend) keeps the picture crisp and the GPU cold. Nothing else is on by default: curvature is 0 in every preset.
- **Pixel shape**: the page used square pixels (256x224 at 1x, 1.143:1). A Mega Drive 256-pixel line fills the same width of a 4:3 screen as a 320-pixel line, so a pixel is about 8:7 wide (the figure used for the SNES 256x224 mode too; 25:22 from the dot clocks, 1.136). The CRT styles default to 8:7 (the picture is 292.6:224, as a TV showed it, and the round shapes in the portraits and the shooting range look right); Square is one click away and is what Plain pixels always uses. Everything that maps a pointer to a game pixel knows the shape (12.4).
- **Layout** (`fit()` in `main.ts`, `planLayout` for the touch layouts): with a style on, the picture takes the biggest size that fits (fractional scales are fine, the lines come from the source line position) in WHOLE device pixels on both axes (`fitPicture`: css size = device pixels / DPR, so the canvas backing store maps 1:1 to the screen and the mask lines up). A whole scale is used when it is within 10 % of the biggest (`crtScale`), because whole scales give identical lines. Under 2 device pixels per game line the filter has nothing to show: the page uses the plain layout and the Display tab says so (`crt.note.small`). A change of `devicePixelRatio` (window moved to another monitor) lays out again (matchMedia, and a 500 ms check for browsers and emulators that do not report it).

### 12.2 Pipeline (WebGL2; all intermediate textures are SRGB8_ALPHA8: 8 bits, the hardware decodes on read and encodes on write, so every blend is in linear light with no float-texture extension, which phones do not all have)
0. `upload`: `texSubImage2D` of the 256x224 frame straight from the wasm memory view (`main.ts frameView()` caches the view; no allocation per frame).
1. `composite` (256x224, only when the blend is on): the one step done in the signal (gamma) domain, because that is where a TV's decoder works: YIQ, luma low-passed with `[a, 1-2a, a]` (a = 0.22 x amount: a 1-pixel pattern drops to 1 - 4a), chroma with a 7-tap triangle (its bandwidth is a quarter of luma's). This is what blends the Mega Drive's dither and 1-pixel stripe transparencies. Then back to linear.
2. `glow` (only redone when a frame arrives): frame -> 128x112 -> 5-tap blur H and V -> 64x56 -> blur H and V. The screen pass mixes 65 % / 35 % of the two levels, added in linear light after the mask (halation is light scattered in the glass, it is not masked) and divided by `1 + glow` so it moves brightness instead of adding it.
3. `hpass` (game-line resolution, `devW x 224`, flat pictures only): every game line reconstructed at every device column. Each game pixel is a box, seen through a gaussian (the beam and the electronics' bandwidth): weight_i = cdf((x - i)/s) - cdf((x - i - 1)/s), five taps, weights sum to 1. No ringing, no nearest-neighbour stair steps, edges as sharp as `sharp` says (s from 0.52 at sharp 0 to 0.14 at sharp 1 game pixel, never under half a device pixel). With curvature the horizontal position depends on y, so the same reconstruction runs per fragment inside the screen pass (the "inline" variant of the same shader, compiled from the same source with `HPRE` undefined).
4. `screen` (canvas backing resolution): warp, then for the 4 nearest game lines `c_j` from the hpass texture; a beam per line: a gaussian in y of variance `sigma^2 + dy^2/12` (the output pixel integrates the beam over its own height `dy`, a box seen through a gaussian), energy `c_j` per line so the average brightness is kept, `sigma` growing from `sigLo` (black) to `sigHi` (full bright) with `b = sqrt(0.5 luma + 0.5 max channel)`: dark pixels are thin beams with clear gaps, bright pixels are wide beams that fill them. Then the mask, then `+ glow`, a soft shoulder above 0.85 (no hard clip, no hue shift), sRGB encode. Black lines cost nothing (early `continue`).
- Scanline widths (`derive`, `crtmath.ts`): `sigLo = max(0.85/scale, 0.60 - 0.45 scan)`, `sigHi = max(0.85/scale, 0.60 - 0.30 scan)` game lines; the floor keeps a beam from being thinner than 0.85 device pixel, which is what keeps the lines even at fractional scales (12.5). `scanFade` takes the scan strength to 0 between 2.5 and 1.5 device pixels per line.
- Mask (`maskLayout`): RGB triad of a whole number of device pixels (`max(3, round(scale x 0.5))`, x 0.34 for the aperture grille), the stripe coverage of each colour is the exact overlap of the device pixel with the stripe (no aliasing for any pitch), and `fade = smoothstep(3.5, 4.5, scale)` switches it off on phone sized pictures. The mask strength at a pixel falls with that channel's own brightness (a bright phosphor blooms over its neighbours: no clipping, and white keeps its level). Kinds: aperture grille (stripes), slot (stripes with bridges on the game line boundaries, staggered by triad), shadow (dots in rows, every other row half a triad over).
- Colour: nothing in the path changes hue. Neutral white point, no tint, no chromatic aberration, no noise, no flicker, no vignette. (A phosphor gamut / white point setting was considered and left out: it would only add a way to get colours wrong; the plain palette is the Mega Drive's.)
- Brightness: `gain = (1 + 0.04 mask + 0.06 scan + 0.08 composite) / (1 + glow)`, fitted with the harness so the mean linear luminance of the picture stays within 10 % of the plain picture on every scene (12.5 table).

### 12.3 Geometry and curvature
`crtmath.warp`: `u = n x (1 + k (ny^2, nx^2))` on the picture box in -1..1, k = 0.1 x curvature (at most 10 %). The middle of each frame edge stays on the middle of the box edge and the corners are pulled in, so the whole frame stays inside the box: nothing is cut (Snatcher has text at the edges), the black corners are just empty. No rounded-corner mask, no vignette. `unwarp` (fixed-point) is the inverse. Curvature is off in every preset.

### 12.4 Pointer and touch mapping (all of it goes through `touchmath.clientToGame` -> `crtmath.viewToGame` with the page's `view = {par, curve}`)
The light gun with a mouse (`Input.track`), a finger as the light gun and every "touch the game" tap (`touchgame.ts point()`, then `hitRect` against the engine's rectangles in game pixels) call `clientToGame`, whose default view is the page's current one (flat when the filter is off, so the old behaviour and its tests are bit for bit unchanged): letterbox, divide by the pixel aspect, apply `warp`. The pad layouts use the same picture box. Checked three ways: unit tests (every game pixel is hit at its own centre in 5 views and 4 box shapes; corners with curvature), `test/crt.ts` A3 (a 3x3 marker drawn by the SHADER is found at the device position `gameToView` predicts, within 1.6 device pixels over 42 points: 2 shapes x curvature 0 / 0.6 / 1, corners and edges; the measured worst case is under 1 device pixel), and `test/crt.ts` B2/B3 (a real mouse and real touches in the running game: 54/54 exact game pixels for 8:7 and square x curvature 0 / 0.5 / 1; a tap on a menu line selects that line on a 360x640 phone at DPR 2 with 8:7 and with curvature 0.5).

### 12.5 How it was tuned (`test/crt-frames.ts`, `test/crt-compare.ts`; pictures in `work/shots/crt/`, gitignored: they show the game)
Eight captured frames (title with its dithered noise, Konami logo, city skyline, Junker HQ dialogue, command menu, night apartment, shooting practice), each through Off and every style at 1280x720, 1920x1080, 2560x1440, a phone portrait (1071x2379, DPR 2.6) and landscape, written with 1:1 crops (3x nearest for viewing) around a face and around text, `compare-<scene>.png` (the same strip of the picture as Off | TV | PVM at 1080p) and `compare-composite-<scene>.png` (blend 0 | preset | 1 on the dithered scenes). Rounds, what each fixed:
1. First version: whole picture 14 to 24 % brighter (glow added energy, gain too high), mask a heavy lattice (strength .3 to .35), TV smeared (sharp .3), scanlines nearly invisible at 4 px per line (gap/peak 0.7 to 0.9 on a flat grey). Fixed by: energy-conserving glow with a matching divide, the mask fading with channel brightness, `sharp` .72 and blend .45 for the TV, narrower beams (floor in device pixels instead of lines).
2. Evenness at fractional scales: with beams 0.55 device pixel wide the peak of neighbouring lines differed by up to 45 % at 2.7x; a floor of 0.85 device pixel and the integrated (variance + dy^2/12) beam brought it to 0.88 to 0.98.
3. Speed (12.6): per-channel beams and 24 `tanh` per pixel replaced by one beam per line and 4 `exp`, the horizontal work moved to the game-line pass.
Evenness measured through the real pipeline on flat greys (`test/crt.ts` A4; line energy = the light of one game line summed over its rows, peak = the brightest row): TV 2.7x: energy min/max 0.973, peak 0.924; 3.35x: 0.983 / 0.943; 4.82x: 0.991 / 0.962; 6.7x: 0.990 / 0.981. PVM 2.7x: 0.966 / 0.890; 3.35x: 0.976 / 0.927; 4.82x: 0.987 / 0.963; 6.7x: 0.989 / 0.981. Whole scales are exactly even. Line contrast (darkest row / brightest row of a flat grey, 4.82x): TV 0.54 (dark) to 0.77 (bright), PVM 0.18 to 0.53.
Mean linear luminance of the picture area at 1920x1080, ratio to Off, final presets: city 0.92 / 1.04 / 1.03 (TV / PVM / scanlines only), dialogue 1.01 / 1.01 / 1.02, Konami logo (white) 1.01 / 0.97 / 0.99, command menu 1.02 / 1.01 / 1.02, night apartment 0.95 / 1.04 / 1.03, shooting range 1.01 / 1.03 / 1.03, title (gold dither) 1.04 / 1.02 / 1.02. The darkest dithered scenes lose the most on the TV style (-5 to -8 %) because composite blending averages the dither in the signal domain, which is darker than the average in linear light; that is how a TV does it, and it stays inside the 10 % budget. In game (`test/crt.ts` B1, a skyline at 760x600): 0.92 / 1.03 / 1.02.

### 12.6 Performance (`test/crt-perf.ts`, headless Chrome 151 with ANGLE/Vulkan on this laptop's Iris Xe; the tests themselves run on software GL, which is for pictures only)
Sustained cost of upload + all passes + a pixel read-back to wait for the GPU (`bun web/test/crt-perf.ts 600`, 3 runs, median; Iris Xe through ANGLE/Vulkan; the other agents' load on the machine moves numbers by 10 to 20 %):

| Surface | Scanlines only | Studio monitor | Consumer TV (composite + glow + mask) |
|---|---|---|---|
| 1920x1080 (scale 4.82) | 0.67 ms | 0.74 ms | 0.90 ms |
| 2560x1440 (scale 6.0) | 0.93 ms | 1.03 ms | 1.28 ms |
| 3840x2160 (scale 9.0) | 1.8 ms | 2.1 ms | 2.2 ms |
| phone portrait, 1071x2379 (DPR 2.6, scale 3.66) | 0.45 ms | 0.49 ms | 0.80 ms |

GPU time from `EXT_disjoint_timer_query_webgl2`, one frame per animation frame (the GPU clocks down between frames, so these read 1.1 to 1.4 ms at 1080p and 1.9 to 3.0 ms at 1440p; the sustained numbers above are what a running game sees). CPU side per frame: the GL calls take 0.06 to 0.1 ms inside the real page (`filterMs` of `test/crt-loop.ts`; 0.2 to 0.4 ms in the batch loop, which also counts the texture upload), the 2D canvas copy that was already there costs 0.014 ms.
Frame loop (`bun web/test/crt-loop.ts`, the game running, headed Chrome on the real display path): 59.5 to 59.8 animation frames per second at 1920x1080 and 2560x1440 for Plain pixels and all three styles, p95 frame gap 16.7 to 16.8 ms, JS time per frame about 4 to 5 ms with or without the filter (the engine's ticks dominate; the filter adds well under 0.1 ms). Headless Chrome with software or Vulkan-without-a-surface compositing is not representative: it drops to about 43 frames per second at 1440p with any WebGL canvas, and to about 12 per second on SwiftShader (hence 12.7).
How it got there: the first version cost 1.8 to 2.3 ms at 1080p and 2.8 to 3.4 ms at 1440p (per-channel beams, 34 `tanh`/`exp` per pixel, the horizontal reconstruction per fragment). The horizontal pass at game-line resolution, one beam width per line, `exp` instead of two `tanh`, and skipping black lines halved it. The mask is dropped automatically on phone sized pictures (no ALU, `uMask = 0`); a mid-range phone GPU was not measured (12.9). Not done: an automatic quality drop when the frame time grows (the cheapest step would be to switch the style to Scanlines only).

### 12.7 Fallbacks and what the page never does
- The GL side is built when the first frame of a game is about to be drawn (`Display.present`), not at page load: the start overlay, the importer and the static-site check never touch WebGL. The layout is made for the filter until then; if WebGL turns out not to work, the layout is redone for the plain picture before anything is painted.
- No WebGL2, shader or framebuffer errors: `CrtFilter.create` returns null, the plain picture is used, the Display tab shows `crt.note.nogl`. `webglcontextlost`: the 2D canvas is shown at once (layout redone for the plain picture), `webglcontextrestored` rebuilds every GL object and draws the current frame again (`test/crt.ts` A6).
- A software rasterizer (SwiftShader, llvmpipe: `failIfMajorPerformanceCaveat`, and the renderer name as a second check) counts as no WebGL: a filter at 1080p there would make the game crawl (measured in headless Chrome: the game fell to about 12 frames per second). `?crt=tv` (or any named style) asks for it explicitly and is allowed, which is how the tests and the harness run.
- The WebGL1 path was not written (iOS 15+ and every current browser have WebGL2; the plain 2D picture is the fallback).
- Autosave, the debug overlay, the touch pad layout, fullscreen, resize, pause and hidden page, the static build are unchanged: the filter only reads the frame and draws into `#crt`. `build-static.sh` copies nothing new (the shaders are strings in `dist/main.js`).

### 12.8 Tests
`bun test web/test` (`crtmath.test.ts`, 18 tests: warp/unwarp, pointer mapping in 5 views and 4 box shapes, scale rule, whole-device-pixel sizes at 6 DPRs, mask pitch and fade, shader inputs, settings parse / persist / defaults, the touch layout with the filter and its fallback), `bun web/test/crt.ts` (headless Chrome, own server on 5374; part A needs no game: shader build, brightness of every style on a flat frame, shader warp against the pointer mapping, line evenness at 2.7 / 3.35 / 4.82 / 6.7x, mask periodic in whole device pixels and absent at phone size, settings and Clear everything, phone default, WebGL missing, lost and restored context, 7 window / DPR combinations; part B needs assets/raw: frame buffer identical with the filter on, brightness of three styles in game, real mouse and real touches with the filter on, no WebGL while playing; `CRT_PART=A|B` runs one part), all in `scripts/test-web.sh`.

### 12.9 What needs a look on real hardware
A real phone (iOS Safari and Android Chrome): the look of Scanlines only at 3.4 to 3.7 device pixels per line, battery / heat over a long session (a pass count of 8 per frame, 1 ms of GPU time at 1080p on an Iris Xe; mid-range phone GPUs were not measured), the first-run default; a high-DPR desktop monitor (mask pitch 3 to 4 device pixels: does the grille look right next to the panel's own subpixels), a 4K display, and a display with a BGR or PenTile subpixel layout (the mask is RGB stripes in device pixels and does not know the panel's order).

## 5. Tests
(The importer tests are in section 8: `cargo test --release -p sncore --test import` and `bun web/test/import.ts`.)
`scripts/test-web.sh [outdir]`: `bun test web/test/` (pacer, picked-folder recognition), then headless Chrome (chrome-headless-shell from
`~/.cache/puppeteer`): `ui.ts` (overlay, menu pause, hidden-tab pause, volumes, mono/stereo, stop commands, LRU, save import/export and
save event, picked-folder run, degraded assets, phone width 360x640, screenshots to the out dir), `shot.ts newgame` and `shot.ts scene=SP01:0`
`autosave.ts` (section 9), `static.ts` (section 10; run by hand, it needs `build/site`), (exit code 1 on any console error, page error or HTTP >= 400). Missing-file 404s of optional audio are expected console noise in real use.

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
