# Snatcher (USA, Sega CD) – native web port (private preservation project)

A reimplementation of the Snatcher engine in Rust, compiled to WebAssembly, that plays the
original game data from **your own disc image**. It is not an emulator: the script VM,
sequencer and scene composer are rewritten from the reverse-engineered 68000 code.

No game data is included in this repository. `assets/`, `work/`, `*.chd/*.bin/*.cue` are ignored.

## Play (the simple way): import your disc image in the browser
You need the page served (see "Build" below) and your own disc image of **Snatcher (USA, Sega CD)** as `.cue` + `.bin` (one `.bin` for the
whole disc, or one `.bin` per track as in Redump dumps). Then:
1. Open the page and press **Import disc image (.cue + .bin)...**, select the `.cue` **and** all its `.bin` files together (or drop them on the page).
2. Wait for the progress bar (about 10 s); press **Play**. The music is prepared in the background (a few minutes, shown in the toolbar).
3. Next visits play at once: the import is stored in this browser (about 1.1 GB with the CD music; the page shows the space used and has a
   **Remove imported data** button).
Your files are read and processed in the browser only and are never uploaded. The importer checks that the disc is Snatcher (USA) and tells
you if it is another disc, another region or edition, an incomplete image or a missing `.bin`. CHD images are not supported yet (convert them first:
`chdman extractcd -i "Snatcher (USA).chd" -o snatcher.cue -ob snatcher.bin`). No extraction, ffmpeg or Python is needed on this path.
Details and measurements: `docs/re/web.md` section 8.

## Requirements (to build the page and for development)
Rust (with `wasm32-unknown-unknown` target and `wasm-bindgen-cli 0.2.129`), `bun`. For the local extraction pipeline below also `chdman`, `ffmpeg`;
for reverse engineering only: Python 3 with `capstone`, Ghidra 12.1.4.

## Build
```sh
scripts/build-web.sh        # wasm engine + web front end (web/pkg, web/dist)
bun web/server.ts           # http://localhost:5300/ (PORT=... to change); only serves the page, the wasm and ./assets if it exists
```

## Developer pipeline: extract the disc to files (optional; the importer above replaces it for players)
```sh
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
cargo build --release
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw   # assets/raw/{files,header.bin,ip.bin,filelist.tsv}; multi-bin cues work too
scripts/convert-music.sh                                          # CD-DA -> assets/web/music/trackNN.ogg
./target/release/snt audio-pcm assets/raw/files assets/audio      # voices, banks, drum kit, pcm.json
./target/release/snt audio-fm  assets/raw assets/audio            # FM songs + effects, fm.json (~1.5 min)
python3 -I re/py/audio_encode.py assets/audio                     # -> assets/audio/web/**.ogg + json
```
`./target/release/snt import-disc work/snatcher.cue outdir` runs the importer natively into a directory of streams (what the browser stores);
`snt split-cue work/snatcher.cue work/split "Snatcher (USA)"` makes a Redump-style multi-bin copy for tests.

The page opens a start overlay. If the server finds `assets/raw` it offers **Play**; otherwise (or in addition) you can import a disc image
(above) or **Choose extracted folder...** to load the files from a folder you pick in the browser (best: the whole `assets` folder;
it needs `raw/files`, optionally `raw/header.bin`, `raw/ip.bin`, `audio/web`, `web/music`). Nothing is uploaded,
and missing optional files only produce a note (sound off, boot logo screens skipped). Details: `docs/re/web.md`.

URL parameters: `?newgame` (skip title/intro), `?scene=SP01:0` (debug scene jump), `?autostart` (skip the overlay),
`?debug` (engine state overlay, also F1). Tests: `scripts/test-web.sh [outdir]` (unit tests + headless Chrome UI
checks + `web/test/shot.ts` screenshots; uses port 5301, or `PORT`); `cargo test --release -p sncore --test import` and `bun web/test/import.ts`
(importer: byte-for-byte against the reference extraction, headless Chrome with `/assets` off; skip cleanly without the local disc).

Controls: arrows = D-pad, Z/X/C (or A/S/D) = A/B/C, Space = B, Enter = Start; Xbox, PlayStation and Switch gamepads via the Gamepad API
(per-family defaults, remappable in the menu; the game text shows the icon of the button of the last used device, see `docs/re/web.md` section 6).
Light gun (shooting sections): mouse to aim, left button = trigger, right button = second button; any pad key switches
back to the pad. Esc = menu (pauses; volume sliders, smooth scaling, save export/import, controls), F = fullscreen,
M = mute, F1 = debug. The game pauses while the tab is hidden. Saves live in `localStorage` (`snatcher.save.N`);
export/import them as JSON from the menu.

## Documentation
- `docs/PROGRESS.md` – milestones, current state, next steps.
- `docs/RE_NOTES.md` and `docs/re/*.md` – reverse-engineering notes with evidence.
- `docs/re/oracle.md`, `docs/re/oracle2.md` – the two dev-only test oracles (BIOS-free HLE; real Sega CD BIOS on Genesis Plus GX, needs your own BIOS image).
- `docs/voice-inventory.md` – every voice clip (1162 clips, 83 min): where it plays, whether text is shown, duration; subtitle feasibility. `scripts/voice-inventory.sh` rebuilds it.
- `docs/re/web.md` – web front end (sources, audio graph, saves, tests, status).
- `re/README.md` – disassembly/Ghidra workflow.
