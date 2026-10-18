# Progress

## How to run
See `README.md`. Short version:
```
cargo build --release
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw
scripts/convert-music.sh                                   # CD-DA → Opus
./target/release/snt audio-pcm assets/raw/files assets/audio
./target/release/snt audio-fm assets/raw assets/audio && python3 -I re/py/audio_encode.py assets/audio
scripts/build-web.sh && bun web/server.ts                  # http://localhost:5300/ (?newgame, ?scene=SP01:0, ?debug)
cargo run --release -p snengine --example run -- <frames> <every> <outdir> [auto|f:bits,...] [SPnn:pc]   # native, PNG + sheet
re/oracle: .venv/bin/python -I scenario.py scenarios/intro.json   # original code, reference PNGs (docs/re/oracle.md)
```
Ports: 5300 web port, 5310 asset viewer (`bun web/viewer/server.ts 5310`).

## Milestones (state matches the git history)
- **M0 extraction – DONE.** IP/SP carved (sizes match header), 97 ISO files + 3 txt, 20 CD-DA tracks.
- **M1 code analysis – DONE (ongoing labels).** SP @S:0x6000, SUBCODE @S:0xD400, main CPU is a
  VBlank-driven display slave; label files `re/labels/*.txt`; Ghidra headless + capstone listings.
- **M2 LZKN1 – DONE.** Same as Contra Hard Corps variant; Rust decoder byte-identical on 1440/1440 blobs.
- **M3 assets – DONE.** Scenes/palettes/font decoded and rendered (viewer :5310); voices (1162),
  PCM banks/drums/sfx, 122 FM sounds pre-rendered from a Rust translation of the 68k FM driver.
- **M4 script VM – DONE.** 75 opcodes, disassembler consumes 100% of 39 SP files, reference VM.
- **ORACLE – DONE.** BIOS-free HLE emulator (Musashi) runs the original from power-on to Junker HQ.
- **M5 engine – IN PROGRESS.** New game plays the opening (Omni Building caption, ACT 1 card,
  Junker HQ, Mika dialogue, command menu) matching oracle screenshots; scene timing within ~3%.
  Web front end: canvas, input, WebAudio (CD/FM/voice/PCM), saves in localStorage.
  Missing: title/intro modules, menu highlight bar, pointer/keypad menus, shooting, audio check.

## Resume here: autosave (2026-10-18, docs/re/web.md section 9)
The page saves the running game by itself and offers "Continue where you left off" in the start window (primary button beside Play). **Design: a snapshot of the whole engine state** at a safe point (not the game's own 0x1C0-byte save block,
which cannot restore a menu or a message): `core/src/snap.rs` (codec, `snap_struct!` checks every field at compile time), `engine/src/snapshot.rs` (`safe_point`, `snapshot`, `restore`, header with format, layout fingerprint, disc digest, language and pack hash, checksum),
wasm API `autosave_point / autosave_key / snapshot / snapshot_info / restore / snapshot_header`, front end `web/src/autosave.ts` (IndexedDB `snatcher.autosave`, throttle, bad-autosave messages) wired in `main.ts`.
Safe points: the command menu waiting for a key, a message box waiting for the confirm button; never during a voice clip, PCM effect, finite FM jingle, text entry, shooting, picture loading, the title, intro and ending. Saved at a new safe point after a scene change (3 s apart at most),
else every 30 s, and when the page is hidden or closed. Toggle "Autosave" (Saves tab, `snatcher.autosave.on`, default on), "Delete autosave" with a second click, "Clear everything" removes it, "Remove imported data" keeps it, the saves export does not include it.
Restore is exact for the engine state and the picture (tests); the CD-DA and looping FM music start again at the engine clock's position (approximate for FM); voices and shooting are never saved. Cost: COST_SHORT.
Tests: `cargo test --release -p snengine --test snapshot` (round trip at 50+ points, frames and events equal after 600-700 frames), `bun web/test/autosave.ts` (also in `scripts/test-web.sh`), `import.ts` checks the wipe / remove rules.
Open: the shooting stages could be made safe once their state is covered by a test (the snapshot already holds the object runtime; `Shoot` and the gun registers are not stored); the intro / ending timelines are not saved; one autosave slot only (a second "previous" slot would protect against Play overwriting it);
an FM music position from the real driver instead of the frame count; `pagehide` writes are not guaranteed to finish in every browser (the last safe-point save stands).

## Resume here: disc image import in the browser (2026-10-17, docs/re/web.md section 8)
A player with the disc image opens the page, picks (or drops) the `.cue` + `.bin` file(s) and plays; no extraction pipeline. Done: multi-FILE cue parser + sector layout
(`core/src/disc.rs`), the importer (`core/src/import.rs`: Sega header + SHA-256 disc check, ISO9660 files, boot code, PCM voices/banks/drums, CD-DA, FM manifest by a chip-less driver run),
wasm bindings (`engine/src/importer.rs`), worker + OPFS cache + imported Source (`web/src/{import.worker,imported,cache}.ts`), overlay UI with progress, drag and drop, storage
usage, persist request, remove button, en + pt-BR strings, FM sounds rendered in the background (priority to what the game asks), CD-DA copied into the cache (option to skip).
Tests: `cargo test --release -p sncore --test import` (byte-for-byte vs `assets/raw`, CD vs the WAVs, voices/banks/drums vs the WAVs, all 122 FM vs the WAVs with `SNT_FULL=1`,
single vs multi-bin, wrong-disc errors with synthetic images), `bun test web/test` (`imported.test.ts`), `bun web/test/import.ts` (headless Chrome, `/assets` off, screenshots in `work/shots/import/`).
Tools: `snt import-disc <cue> <outdir>`, `snt split-cue <cue> <outdir> <name>` (Redump-style multi-bin copy), `snt extract` now accepts multi-bin cues.
Open: CHD input (decode in wasm or `chdman` first), a static production build (the page needs `/pkg`, `/dist`, `/i18n` served; no `/assets`), other regions / editions (add their serial + digest
to `import::KnownDisc` once a dump is available), FM background rendering could use a second worker, a persisted `FileSystemFileHandle` for the CD music (Chromium) instead of copying 490 MB.

## Resume here
oracle2 (2026-10-15, `docs/re/oracle2.md`): a second, **real-BIOS** oracle (Genesis Plus GX + Sega CD BIOS US v1.10, deterministic, ~735 frames/s, same trace format, `ORACLE=2` for the fid/shoot tools)
confirms the HLE oracle: 12/12 reference screens pixel-identical, the message-box palette split dark, Act 1 session 106/106 queues identical HLE vs real BIOS, engine 106/106. It found what the HLE cannot show:
(1) GAME OVER / QUIT INVESTIGATION: the real game shows the GAME OVER picture, then the SP reloads and the game **restarts at the title** - the HLE's title phase stayed black, the engine stayed on GAME OVER forever; **fixed** in the engine
(`Engine::reset_to_title`, test `state4_restarts_at_the_title`); (2) real-BIOS timing: file loads take 12 + sectors/1.25 + 7 frames (HLE/engine 3/0), a voice start 38 frames (HLE 4, engine 2), so the engine is ~8 % faster than the console model.
Fidelity vs the original (oracle): Acts 1-3 + endings queues identical (Act 1 106/106, SP29 165/165, a2_taxi 74/75 + the now fixed state-4 queue); staff roll pixel-identical; CONTINUE round trip equal; all 15 shooting stages frame-exact with pad;
FM driver write-exact, FM output within ~2 dB of the Nuked YM2612 run of the real driver (oracle2 section 5). Build, tests (sncore 38, snengine 21 + player_path 4, prompts 3), `scripts/build-web.sh` and `scripts/test-web.sh` are green.
Open items, in suggested order:
1. Timing vs the real console (docs/re/oracle2.md section 3): optional load/voice-start model (`20 + sectors*0.8` frames per file load, voice start 12/30 frames by seek distance); the core's seek time is a heuristic, decide first whether exact feel matters.
2. Sub-CPU stalls at PCM sound starts (verified on the real BIOS: 2.6 handler-less frames per effect start in stage 0): lost presses while a sound plays are not modelled (docs/re/shooting.md).
3. Gun Y bias (`GUN_CAL_BIAS_Y` = 31) stays a HYPOTHESIS: the game's own calibration gives value = y + 27 on both oracles (identical cell rows at y 21/61); only a real Justifier can tell (docs/re/shooting.md, oracle2 section 2).
   Trigger flash (full white frame at every shot while the blaster is drawn) confirmed on the real BIOS, intentionally not drawn.
4. Front end: touch controls, voice pan / music ducking - docs/re/web.md, docs/re/audio.md.
5. YM2612: the real chip model has a DAC noise floor / ladder distortion the engine lacks (high bands 10-30 dB higher in noisy sounds); levels agree, nothing retuned (oracle2 section 5).
6. Larger engine item left: after the GAME OVER restart the title chain timing (`SP_RELOAD_FRAMES` = 90) is from one real-BIOS measurement; the copyright/logo/title phase of the real restart lasts 840 frames like a cold boot.

## Resume here: pt-BR pictures (2026-10-16, docs/i18n-graphics.md)
Done and committed (python `scripts/gfx_i18n/make_patch.py` -> `i18n/pt-BR/gfx.tsv` -> `snt i18n-build`): title menu, act cards, location caption, prologue date / 50 years / RSS / disclaimer / dedication / JAPAO,
videophone banners, CONTINUE slot screen labels + header, GAME OVER (codex-image). Engine: `SubMem::decomp` patches decoded blobs, raw DATA patches in `cmd_load`, only in pt-BR.
Options screen done (2026-10-16, all 21 states, `scripts/gfx_i18n/optshots.py`). Open: NO DATA, Dec. 2047 line, newspaper headlines (reasons in docs/i18n-graphics.md "Kept in English"); play-test the pictures in the browser.

## Resume here: voice subtitles (2026-10-16, docs/voice-subtitles.md, survey in docs/voice-inventory.md)
Status per act: **transcribed** all 1146 clips (Whisper large-v3 on the LAN server, two passes, work/voice/transcripts, gitignored; key in work/whisper.env, never committed); **translated**: Act 1 (291 clips), Act 2 (395 clips, section 11) and **Act 3 (409 clips, 2026-10-17, section 12)** in i18n/pt-BR/voice.json (1095 subtitles); **reviewed**: all three acts (review subagent, 4/5 each); sweep: Act 3 reached all 409 clips (roots in engine/tests/vsub_roots.txt, vsub test green; the sweep now skips CD-DA cues). Open by ear: clips 293, 300, 332, 357, 665, 667, 568, 636/637 (section 11); 961, 1136, 1012, 1145, 1054, 1060, 806, 1041 (section 12); CD-DA "Thank you, Metal" (track 9) was confirmed correct by the user.
Engine: `engine/src/vsub.rs` draws the subtitle as an overlay on the finished frame (no game state change; English byte-identical; tests `engine/tests/vsub.rs`) **placed around the dialogue portraits** (2026-10-16): box with no portrait, lane next to one, band above two or more, compact band without a strip; text wrapped and paged at run time (`core/src/vwrap.rs`), `voice.json` stored unwrapped, `voice-check` validates every layout, `scripts/voice-speakers.py` uses the talking portrait to complete speakers (docs/voice-subtitles.md section 4); option "Legendas de voz" in the web menu (default on in pt-BR).
Bug fixed 2026-10-16 (docs/voice-subtitles.md section 9): a clip followed at once by an inset picture command (0x15, the Junker ID card in SP06, clip 96) lost its subtitle because every picture command ended it; now picture commands end a subtitle only after its clip.
Sweep: `engine/examples/vsub_sweep.rs` (+ `engine/tests/vsub.rs`, roots in `engine/tests/vsub_roots.txt`) plays entry points and checks timing / text / end / cover of every subtitle. **Act 1 re-swept 2026-10-17 with the fixed tool (docs/voice-subtitles.md section 13): all 291 clips reached over flags zero / ones / alt / randN, no violation; speakers of 92 (Mika) and 105-107 corrected from the talking portrait; the vsub test now asserts that every Act 1 clip is reached.** Open: listen to clip 300 (SP12, no subtitle) and 293, 332, 357.
Tools: `snt voice-export`, `scripts/voice-transcribe.py`, `scripts/voice-script.py`, `snt voice-import`, `snt voice-check` (+ part of `i18n-check ... all`), `web/test/vsub.ts`, `voice_probe` with `VSUB=1 I18N_PACK=...`, `strip_probe` (portrait geometry / talking portrait survey), `scripts/voice-speakers.py`.
Next: (1) play-test the subtitles in pt-BR (timing of the hold, long clips);
(2) optional English subtitle mode from the local transcripts; (3) the 15 SP09 file-reading clips show the English page in the picture and the pt-BR subtitle below it (compact band), check it reads well.

## Resume here: CD-DA speech subtitles (2026-10-17, docs/voice-subtitles.md section 10)
Done: engine overlay on the CD clock, pack section, `snt cdda-import` / `cdda-check`, web audio aligned to the engine clock, **transcription with ElevenLabs Scribe v2** (`scripts/cdda-transcribe.py --backend eleven`; transcripts only in gitignored `work/cdda/`), `i18n/pt-BR/cdda.json` (560 cues, 17 tracks; **the prologue track 3 does have narration**, 35-126 s; track 4 has dialogue + dossier from 157 s), review subagent applied.
Open: play-test timing in the browser; speaker names for the dialogue tracks (not shown now); the Whisper backend is untested against a real server.

## Resume here: translation pt-BR (task: translate every batch, review with a subagent, apply, commit)
Docs: `docs/i18n.md` (system, tools), `i18n/pt-BR/GLOSSARY.md` (glossary, style, adaptations), status table at the end of this file (regenerate: `python3 -I scripts/i18n-progress.py`).
Workflow per batch: `snt i18n-ctx` (dump, work/ only) -> write a patch `SPnn@off | ~text` (`~` = auto line breaks, `|` = forced break) -> `snt i18n-import` + `snt i18n-check` (0 failures) ->
`snt i18n-preview` screenshots -> review subagent (sonnet, prompt in work/i18n/review_prompt.txt, not committed) -> apply fixes -> commit `i18n: SPxx translated + reviewed (score n/5)` -> add the file to `i18n/pt-BR/reviews.tsv`.
Shared menu labels: `i18n/pt-BR/menu.json` (`snt i18n-learn-menu`, `snt i18n-apply-menu`).
STATUS (2026-10-15): every script file SP00-SP38 is translated and reviewed (all 4/5; SP01 debug menu intentionally English, SP31 has no text). Next: (1) text drawn in graphics, see docs/i18n-graphics.md; (2) pt-BR voice dubbing (ElevenLabs project); (3) play-test in pt-BR and collect wording fixes (small patch + `snt i18n-check`); (4) done: accented capitals redrawn on the original baseline (docs/i18n.md "Accented capitals").
Page breaks (2026-10-15, branch `pt-line-spacing`): `snt i18n-breaks` (docs/i18n.md "Page breaks in the middle of a sentence") ranks every `{page}`; `i18n-reflow` now picks breaks by severity (sentence end, then clause; `--redo <old-lang-dir>` re-does the added ones); no break of severity 3-5 is left in pt-BR; engine check: `web/test/pages.ts` (`?scene=SPnn:<pc>`, `i18n-ctx` prints `pc=`). Next: play-test the Jordan database pages.
Warning: never re-import an old whole patch file after fixes were applied (it overwrites the fixes); edit the patch source or import a small fix patch.

## Controllers (Xbox / PlayStation / Switch, button icons) – status 2026-10-15
Done and tested (details, tables, checklist: `docs/re/web.md` section 6): family detection (30 real id strings), standard + raw (Firefox-style) layouts,
radial dead zone, last-used-device tracking with hot-plug, per-family default maps and a remap screen (`snatcher.padmaps` in localStorage), View/Share/-
opens the web menu, 32 icon glyphs (codes 0x86-0xA5, `core/src/prompt.rs`) printed for `{EC}/{EE}/{F0}` and the word START (SP09), `set_prompt` wasm API,
help panel with SVG icons. Engine default stays `Style::Genesis` (original letters): fidelity/regression output unchanged.
Tests: `bun test web/test` (gamepads), `bun web/test/pads.ts` (stubbed pads in headless Chrome, screenshots in `work/shots`), `cargo test -p snengine --test prompts`,
`core/src/prompt.rs` unit tests; all in `scripts/test-web.sh` / cargo.
Next steps: (1) verify with real pads (checklist in web.md 6.7), especially the Firefox/Linux raw tables for Switch Pro (HYPOTHESIS);
(2) rumble needs an engine event for shot/player hit (web.md 6.6); (3) icons for the title "PUSH START" and the options pad-configuration screen
(baked into graphics); (4) optional: gamepad navigation of the web menu.

## Blockers
- A real Sega CD BIOS is now used by oracle2 (user-supplied, never committed); YM2612/mix levels are checked against Nuked YM2612 but not against real hardware.
- REA cannot load raw 68000 images; Ghidra headless is used.

## Translation pt-BR (docs/i18n.md) - status
<!-- i18n-table-begin -->
| file | strings | translated | not translatable (typed entries / kept) | reviewed | score |
|---|---|---|---|---|---|
| SP00 | 8 | 8 |  | yes | 4 |
| SP01 | 126 | 17 |  | yes | n/a (English kept) |
| SP02 | 259 | 251 |  | yes | 4 |
| SP03 | 316 | 316 |  | yes | 4 |
| SP04 | 289 | 289 |  | yes | 4 |
| SP05 | 275 | 118 | 163 | yes | 4 |
| SP06 | 592 | 590 |  | yes | 4 |
| SP07 | 389 | 389 |  | yes | 4 |
| SP08 | 703 | 702 |  | yes | 4 |
| SP09 | 522 | 487 | 46 | yes | 4 |
| SP10 | 430 | 430 |  | yes | 4 |
| SP11 | 333 | 333 |  | yes | 4 |
| SP12 | 280 | 280 |  | yes | 4 |
| SP13 | 534 | 534 |  | yes | 4 |
| SP14 | 411 | 411 |  | yes | 4 |
| SP15 | 382 | 382 |  | yes | 4 |
| SP16 | 34 | 26 | 9 | yes | 4 |
| SP17 | 28 | 19 | 18 | yes | 4 |
| SP18 | 472 | 432 | 42 | yes | 4 |
| SP19 | 122 | 122 |  | yes | 4 |
| SP20 | 150 | 150 |  | yes | 4 |
| SP21 | 645 | 645 |  | yes | 4 |
| SP22 | 165 | 165 |  | yes | 4 |
| SP23 | 270 | 270 |  | yes | 4 |
| SP24 | 190 | 190 |  | yes | 4 |
| SP25 | 212 | 212 |  | yes | 4 |
| SP26 | 426 | 426 |  | yes | 4 |
| SP27 | 220 | 211 | 9 | yes | 4 |
| SP28 | 321 | 310 | 11 | yes | 4 |
| SP29 | 68 | 68 |  | yes | 4 |
| SP30 | 278 | 278 |  | yes | 4 |
| SP31 | 0 | 0 |  | no |  |
| SP32 | 275 | 275 |  | yes | 4 |
| SP33 | 221 | 217 | 9 | yes | 4 |
| SP34 | 53 | 53 |  | yes | 4 |
| SP35 | 404 | 365 | 42 | yes | 4 |
| SP36 | 68 | 68 |  | yes | 4 |
| SP37 | 199 | 199 |  | yes | 4 |
| SP38 | 58 | 58 |  | yes | 4 |
| **total** | **10728** | **10296** | | | |
<!-- i18n-table-end -->
