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

## 5. Tests
`scripts/test-web.sh [outdir]`: `bun test web/test/` (pacer, picked-folder recognition), then headless Chrome (chrome-headless-shell from
`~/.cache/puppeteer`): `ui.ts` (overlay, menu pause, hidden-tab pause, volumes, mono/stereo, stop commands, LRU, save import/export and
save event, picked-folder run, degraded assets, phone width 360x640, screenshots to the out dir), `shot.ts newgame` and `shot.ts scene=SP01:0`
(exit code 1 on any console error, page error or HTTP >= 400). Missing-file 404s of optional audio are expected console noise in real use.
