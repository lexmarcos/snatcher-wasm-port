# Voice inventory: every voice clip of Snatcher and what is on screen while it plays

Survey made on 2026-10-15 to decide whether pt-BR subtitles for the voice lines are worth doing. Nothing was transcribed or translated, no engine
behaviour was changed. No game text is quoted here: strings are referred to by offset, clips by id. Generated tables come from
`snt voice-inventory` (section 8); the per-clip CSV files are in `work/voice/` (gitignored).

## 1. Summary

* The disc has **1162 voice clips, 83.2 minutes** (`PCMLT_01.BIN` ids 36..1197; ids 1..35 are silent PCM sfx *banks*, docs/re/audio.md section 1.2).
  **1146 clips (82.8 min) are started by a script**, 16 clips (0.5 min) are never started (section 6).
* **No voice clip is ever played together with a message box string.** All 1200 voice starts in the 39 scripts are followed by a `CMD0A` (the sequencer waits for the clip to end) within seven nodes and before any other voice,
  with no `SAY` in between, so a message box string never overlaps a clip. The script text (the part that is translated) therefore never subtitles a voice.
* Classes (per clip, minutes counted once per clip):

| class | clips | minutes |
|---|---:|---:|
| partial | 3 | 0.4 |
| unknown | 43 | 3.0 |
| voice-only | 1085 | 77.3 |
| with-text | 15 | 2.1 |
| never started by any script | 16 | 0.5 |
| **all voice clips of the disc** | **1162** | **83.2** |

  * `voice-only`: **1085 clips, 77.3 min**: no text before, during or after the clip in the same script sequence, nothing in the picture.
  * `with-text`: 15 clips (2.1 min): the SP09 file-reading screen, where the picture itself shows the spoken pages (text drawn by animation objects of `DATA_G0`, not a script string; English only).
  * `partial`: 3 clips (0.4 min): text in the picture that is not a transcript (GAME OVER picture, the two videophone banners, not-in-service and no-video).
  * `unknown`: 43 clips (3.0 min): a `SAY` stands right before the clip or right after the wait. The text length does not follow the clip length (correlation r = 0.22 for text-after, 0.10 for text-before, section 5),
    so it is not a transcript of the clip; whether the pair belongs together cannot be confirmed without listening.
* The screen while a clip plays is the **dialogue strip with an empty message box** (portrait icons next to it) in 86 % of the minutes, a black screen in 6 %, a picture without strip in 3 %, 5 % not determined (section 4).
  The strip is exactly where the message box would be drawn, so the game's own message-box path can show a subtitle there.
* Cost: Whisper on this CPU is an estimate of about 15 minutes (`small`) to 3 hours (`large-v3`) for the whole set; ElevenLabs Scribe would cost about US$0.30 but means uploading game audio (section 7).

## 2. Where a voice can start (evidence)

| path | result |
|---|---|
| script op 0x04 `CMD0D id` -> sequencer command 0x0D (S:0x101DC) -> `pcm_play_voice` S:0x154F8 | the only path that starts an audible clip: 1332 nodes in the 39 files, 132 select a bank (id < 36, silent), 1200 start a clip |
| `pcm_play_voice` callers | the xrefs of the SP disassembly (`re/out/dis/*.asm`) show exactly two: sequencer command 0x0D (S:0x101E4) and the title code at S:0xE148, which asks for id 0x1E = bank 30 (silent) |
| scene modules (DATA_D0/D1/D2/Y16: title, intro, staff roll) | only PCM sfx (`sound06`) and FM calls; `engine/src/modules/*.rs` never emits `Out::Voice`; a 30000-frame run of the boot chain and of the ending produces no voice event |
| CONTINUE (save restore) | replays `0D [$9C1A]`, which only holds bank ids (voices reset it, `game.rs` seq 0x0D) |
| anim ops 0x9D / 0x9F / 0xAD (`subfn_12`, `subfn_06`) | FM sfx / music and PCM sfx, never a PCMLT voice |
| shooting stages | the stages only use sfx; their voices (e.g. SP29, SP30) come from the surrounding scripts (counted) |
| attract mode | none: the title waits for START (30000 idle frames: no voice, no demo) |

The sequencer command 0x0D keeps `$7891 = 1` and loads the portrait scene `S:0x13FEC[id]` (table of 1198 bytes) after the start. That byte is the talking-animation index
of the clip, not a speaker identity (it falls monotonically with the id), so it is not used as a speaker guess.

## 3. Method

1. `snt voice-inventory` (tools/src/cmds/voice.rs) parses SP00..SP38 and rebuilds the execution structure of every node the way the VM walks it (cons cells, link
   continuations, IF/WHEN/CYCLE/RANDOM/SWITCH alternatives, menu items, GOSUB). The sweep parser of `sncore::script` places the second child of a cons cell wrongly
   when the first child has a link (found in SP08 L_20db); the walker repositions it with the VM's own skip rule (S:0x1699A).
   The test `inventory_covers_every_cmd0d_node` checks that all 1332 `CMD0D` nodes are found exactly once.
2. From every `CMD0D id >= 36` the script is followed forward (through GOTO / JUMP / CALL, up to 12 hops) to the first of: a `SAY`, another voice, a menu, the end of the block.
   `CMD0A` marks "waited". Classes: simultaneous `SAY` -> `with-text` (never found); `SAY` after the wait or directly before the clip in the same sequence -> `unknown`; otherwise `voice-only`.
3. Duration: samples up to the 0xFF end marker of the clip in `PCMLD_01.BIN` at 16276 Hz (the driver's fixed rate, docs/re/audio.md), same as `snt audio-pcm`.
4. Reachability: roots reachable from the new-game entry SP06:0000 over GOTO/JUMP/CALL/GOSUB/link edges. 1184 of 1200 voice triggers are reachable; the others are the SP01 debug menu (15) and
   one dead block in SP26. Every clip that is used at all is used by the real game (the SP01 clips are repeated elsewhere).
5. Screen probe: `engine/examples/voice_probe.rs` runs the engine headless from the root of each trigger (with the scene that the analysis found, story flags all clear, all set, and pseudo-random for the roots that needed them),
   presses the confirm button every 40 frames and, 60 % into every clip, records: dialogue strip / black / no strip, the portrait icons in the strip (hashed, names in `docs/voice-icons.tsv`, chosen by eye),
   and a screenshot. 992 of 1200 triggers were reached; for 146 more the class of the same scene resource was used (marked `*`), 62 clips have no screen information (mostly menu paths that need a prior state).
6. Screens that carry text were found by looking at the probe screenshots of the 72 roots with text-like picture area; the clips are listed in `docs/voice-graphics-text.tsv`.
   This part is manual and not exhaustive (section 5).
7. Real-hardware check (oracle2, real Sega CD BIOS, `re/oracle2/o2_voicebox.py`): SP06:032e (three voice-only clips of the reception scene) and SP31:0004 (Metal Gear scan screen, six clips) show the same screen as the engine:
   dialogue strip with an empty dark message box and the portrait icon, no text. (The oracle2 run of SP09:0ff4 starts in a different scene because the oracle jump skips the SCENE node of the caller; not used.)

## 4. Where the clips are

Per act: 18.0 min (Act 1), 24.7 min (Act 2), 34.6 min (Act 3) of voice-only clips (Act = SP06-19 / SP20-35 / SP36-38, inferred from the entry points). The ending script SP38 alone has 235 clips, 22.7 minutes.

| file | act | triggers (voice-only / other) | distinct clips | minutes |
|---|---|---:|---:|---:|
| SP00 | sys | 1 (0 / 1) | 1 | 0.1 |
| SP01 | sys | 15 (13 / 2) | 10 | 0.3 |
| SP03 | sys | 2 (0 / 2) | 2 | 0.2 |
| SP06 | 1 | 55 (55 / 0) | 55 | 4.1 |
| SP07 | 1 | 57 (46 / 11) | 54 | 3.9 |
| SP08 | 1 | 21 (19 / 2) | 21 | 1.5 |
| SP09 | 1 | 38 (21 / 17) | 38 | 3.4 |
| SP10 | 1 | 36 (35 / 1) | 36 | 2.2 |
| SP11 | 1 | 26 (23 / 3) | 25 | 1.3 |
| SP12 | 1 | 4 (3 / 1) | 4 | 0.3 |
| SP13 | 1 | 4 (3 / 1) | 3 | 0.3 |
| SP14 | 1 | 34 (32 / 2) | 34 | 2.5 |
| SP15 | 1 | 43 (40 / 3) | 42 | 2.0 |
| SP16 | 1 | 3 (1 / 2) | 2 | 0.1 |
| SP18 | 1 | 9 (3 / 6) | 8 | 0.6 |
| SP20 | 2 | 1 (1 / 0) | 1 | 0.1 |
| SP21 | 2 | 12 (12 / 0) | 12 | 1.0 |
| SP23 | 2 | 54 (50 / 4) | 54 | 3.0 |
| SP24 | 2 | 11 (8 / 3) | 10 | 0.5 |
| SP25 | 2 | 11 (11 / 0) | 11 | 0.7 |
| SP26 | 2 | 27 (26 / 1) | 27 | 1.7 |
| SP27 | 2 | 42 (42 / 0) | 42 | 3.1 |
| SP28 | 2 | 41 (40 / 1) | 41 | 2.3 |
| SP29 | 2 | 15 (10 / 5) | 14 | 0.8 |
| SP30 | 2 | 41 (38 / 3) | 41 | 3.4 |
| SP31 | 2 | 55 (55 / 0) | 55 | 3.8 |
| SP32 | 2 | 48 (45 / 3) | 48 | 3.5 |
| SP33 | 2 | 56 (53 / 3) | 55 | 2.3 |
| SP34 | 2 | 9 (9 / 0) | 9 | 0.4 |
| SP35 | 2 | 6 (1 / 5) | 6 | 0.8 |
| SP36 | 3 | 110 (109 / 1) | 110 | 7.0 |
| SP37 | 3 | 78 (76 / 2) | 75 | 5.1 |
| SP38 | 3 | 235 (235 / 0) | 235 | 22.7 |

Screen while the clip plays (class of the first trigger of each clip; `*` in the scene table = inferred from the scene resource):

| screen while the clip plays | clips | of which inferred from the scene | minutes |
|---|---:|---:|---:|
| black | 97 | 6 | 5.1 |
| no strip | 20 | 9 | 2.4 |
| not probed | 62 | 0 | 3.9 |
| empty strip | 199 | 28 | 15.4 |
| strip + portraits | 768 | 103 | 55.9 |

* `empty strip` / `strip + portraits`: the picture occupies the upper 136 lines; below it a black dialogue strip with 0..3 portrait icons (64x63 px) and an empty message box. This is the layout of normal dialogue before the text starts.
* `black`: the whole screen is black (fade, dark rooms, voice over black). Free for a subtitle anywhere.
* `no strip`: the picture or a solid backdrop reaches the bottom of the screen (SP09 file reading, GAME OVER). A subtitle needs an overlay.

Largest blocks (by length), all voice-only; the full list of 211 roots is in appendix A:

| file | label | act | clips | minutes | screen |
|---|---|---|---:|---:|---|
| SP07 | L_0ebd | 1 | 16 | 2.1 | strip + portraits* |
| SP10 | L_05bd | 1 | 33 | 2.0 | strip + portraits* |
| SP27 | L_0354 | 2 | 20 | 1.7 | strip + portraits* |
| SP06 | L_0e4c | 1 | 14 | 1.3 | empty strip |
| SP37 | L_088d | 3 | 14 | 1.3 | strip + portraits |
| SP38 | L_060a | 3 | 9 | 1.3 | empty strip |
| SP23 | L_08ac | 2 | 23 | 1.3 | strip + portraits |
| SP30 | L_0d19 | 2 | 13 | 1.2 | strip + portraits |
| SP11 | L_00be | 1 | 23 | 1.2 | black |
| SP27 | L_03db | 2 | 20 | 1.2 | strip + portraits |
| SP38 | L_027c | 3 | 14 | 1.2 | strip + portraits |
| SP36 | L_0051 | 3 | 18 | 1.2 | strip + portraits |
| SP31 | L_0014 | 2 | 12 | 1.2 | strip + portraits |

To look at a block: `?scene=SPnn:<pc>` in the dev server (`pc` = the root label without `L_`, the clips are inside), or `web/test/pages.ts`; the engine example `run` takes `SPnn:pc` too.
The label is the top-level script node that contains the voice (a chain of such nodes is one cutscene; roots are linked by GOTO). Scene resources are `DATA_xx#image` (the picture loaded by the `SCENE` node).

## 5. What the picture can show with a clip, and the weak points

* **Subtitle room.** Where the strip exists the message box is empty, as in the real game (checked on the real BIOS). `cmd_message` (engine) switches the portrait objects 5..14 off when a message starts (`MsgEvent::Begin`, S:0x13604),
  so a subtitle sent through the same path replaces the icons exactly as a normal line does. A clip is a few seconds (median 3.5 s, max 35.9 s) and `CMD0A` holds the script until it ends, so the subtitle can stay for the clip length (+6 frames, the engine's voice model).
  For the 20 `no strip` clips (2.4 min) the subtitle would be drawn over the picture.
* **Text before / after (`unknown`).** adjacent text (class unknown): 57 triggers, 40 with the text after the wait, 17 with the text before the voice
  Text length against clip length: r = 0.22 (text after the wait, n = 39) and r = 0.10 (text before, n = 16), median 7.9 and 6.5 characters per second; speech is about 13-16, so these texts are not transcripts of the clips.
  Several of these clips look like sound effects rather than speech (animal-sound menu items in SP24, the GAME OVER voice id 48 used by 10 game-over paths). Needs listening.
* **Not every clip is speech.** Clips under 1.5 s are 159 of the 1146 and some are cries or effects; the 77 clips over 10 s (16.5 min) include long reads (file reading) and probably ambient loops (id 152 is 35.9 s).
  A transcription pass will show it (empty output); I did not listen to anything.
* **Text in pictures** is only found by eye (section 3 item 6). The pictures that carry text and were seen: SP09 file reading (`with-text`), GAME OVER, videophone banners, map captions in SP36 (place names only).
  The SP09 case is also missing from docs/i18n-graphics.md (added there).
* **Speaker.** Not recoverable from the data: the script has no speaker for a voice, and the portrait icons on screen are the listeners as often as the speaker (in the reception scene the speaker, Mika,
  is the big picture and only Gillian's icon is in the strip). The tables show the speakers of nearby `SAY` lines and the portraits (visual labels, HYPOTHESIS, `docs/voice-icons.tsv`). The transcription step will be the real answer.

## 6. Clips that are never started, debug-only clips

unused clip ids: 41 42 52 60 69 70 71 72 73 74 75 76 303 466 962 1137


Those 16 clips (ids above, 0.5 min) are in the table of the disc but no script starts them (a sound test or cut content). The SP01 developer menu (15 triggers, ids repeated elsewhere) is the only debug user.
Clips used several times: 23 (id 48 and 43 ten times each, id 37 seven times).

## 7. Feasibility of subtitles (not measured: nothing was installed)

Environment: `ffmpeg` 6.1.1 at `/usr/bin/ffmpeg`; Python 3.12.3 with pip 24.0, pipx and the venv module (no `uv`); no `faster_whisper`, `whisper`, `torch` or `onnxruntime` installed;
the Hugging Face cache already holds `Systran/faster-whisper-small` (weights only; also HTDemucs); CPU Intel i7-1355U (2 P + 8 E cores = 12 threads, AVX2 + AVX-VNNI), 15 GB RAM, 89 GB free, no NVIDIA GPU.

Audio to transcribe: **1146 clips = 82.8 min** (voice-only 77.3 min); mean 4.3 s, median 3.5 s. Decoded clips are 16 kHz mono (8-bit sign-magnitude source), `snt audio-pcm` already writes them as `assets/audio/voice/vNNNN.wav` (gitignored).

| tool | assumed speed on this CPU | whole set | note |
|---|---|---|---|
| faster-whisper `small`, int8, 12 threads | 5-8 x real time | 10-17 min | weights already cached |
| faster-whisper `medium`, int8 | 1.5-2.5 x | 33-55 min | better for names (Jamie, Napoleon, Benson ...) |
| faster-whisper `large-v3`, int8 | 0.4-0.7 x | 2-3.5 h | best quality, an overnight job |
| ElevenLabs Scribe | cloud | minutes | 1.38 h x US$0.22 = **about US$0.30** (voice-only: US$0.28) |

(speeds are the usual figures for this class of CPU, not measured here.) One call per clip costs a padded 30 s encoder window each: 1146 x about 1-1.5 s (small) / 3-5 s (medium) / 8-12 s (large-v3) is 20-30 min / 1-1.5 h / 2.5-4 h, so concatenate clips with 1 s of silence
into ~30 s files (or use the batched pipeline) and cut by timestamps; keep the id <-> time map. Scribe means **uploading game audio to a third party**; the project rule is not to upload game data, so that is the owner's decision (nothing was uploaded).

Translation: about 8-12 thousand words (estimate, 1146 short lines) against 10,728 strings already translated. A subtitle table keyed by clip id (one entry per clip, not per trigger) fits the existing pack
(`Out::Voice { id }` is already an engine event and the clip lengths are in the audio manifest).

Recommendation: **worth doing.** 97 % of the voiced minutes have no transcript on screen at all, including the whole ending (SP38, 22.7 min) and the Act 3 cutscenes, so a pt-BR player would hear 83 minutes of English with nothing to read. The strip is free, the engine already
knows when a clip starts and how long it is, and the transcription is a one-evening CPU job. The costs are the manual review (non-speech clips, names) and translation. Start with `small` for a draft, run `medium` or `large-v3` overnight for the final text.

## 8. Re-running

```sh
cargo build --release -p sntools -p snengine --example voice_probe
./target/release/snt voice-inventory assets/raw/files --csv work/voice/voice-triggers.csv --clips-csv work/voice/voice-clips.csv --scenes-csv work/voice/voice-scenes.csv --md work/voice/tables.md \
    --probe-results work/voice/probe_all.txt --icons docs/voice-icons.tsv --graphics-text docs/voice-graphics-text.tsv
scripts/voice-inventory.sh          # whole pipeline incl. the engine probe sweeps (about 25 min), QUICK=1 for the static part only
re/oracle/.venv/bin/python -I re/oracle2/o2_voicebox.py SP06:032e 700 work/voice/o2_032e   # real BIOS screen at every voice start (from re/oracle)
```
Files: `tools/src/cmds/voice.rs` (analysis), `engine/examples/voice_probe.rs` (screen probe), `re/oracle2/o2_voicebox.py` (real BIOS), `docs/voice-icons.tsv` (icon hash -> label, visual guess),
`docs/voice-graphics-text.tsv` (clips with text in the picture, by eye). CSV columns: `voice-triggers.csv` one row per `CMD0D` (id, kind, seconds, file, act, pc, root label, scene, class, text relation, text offset,
text speaker, nearby speakers, via-jump flag, next-voice flag, menu flag, reachable, screen, portraits, env, pan); `voice-clips.csv` one row per clip; `voice-scenes.csv` one row per root.

## Appendix A: voice-only blocks by root (generated)

| file | label | act | scene resource(s) | clips | seconds | screen while the clip plays | speaker hint |
|---|---|---|---|---:|---:|---|---|
| SP06 | L_0026 | 1 | DATA_H01#1 | 1 | 4 | empty strip | ? |
| SP06 | L_032e | 1 | DATA_H00#1 | 3 | 13 | strip + portraits | portraits: Gillian |
| SP06 | L_0353 | 1 | DATA_H00#0 DATA_H00#1 | 4 | 14 | strip + portraits 50%, black 25% | portraits: Gillian+woman (short auburn hair) |
| SP06 | L_05d6 | 1 | DATA_H01#0 DATA_H01#2 | 3 | 9 | black | ? |
| SP06 | L_06a0 | 1 | DATA_H02#1 | 9 | 44 | strip + portraits | portraits: Gillian+man (brown beard)+woman (short auburn hair) |
| SP06 | L_0797 | 1 | DATA_H02#0 DATA_H02#1 | 8 | 34 | strip + portraits* | ? |
| SP06 | L_0d52 | 1 | DATA_H07#2 | 5 | 12 | strip + portraits | portraits: Gillian+woman (short auburn hair) |
| SP06 | L_0e4c | 1 | DATA_H07#0 (+2 more) | 14 | 79 | empty strip 50%, strip + portraits 50% | portraits: Gillian+man (red cap)+woman (short auburn hair) |
| SP06 | L_1332 | 1 | DATA_H07#3 | 1 | 8 | empty strip | ? |
| SP06 | L_1340 | 1 | DATA_H07#4 | 3 | 8 | strip + portraits 66%, empty strip 33% | portraits: Metal Gear |
| SP06 | L_1cd7 | 1 | DATA_S0#0 | 3 | 20 | strip + portraits | portraits: Gillian+Metal Gear |
| SP06 | L_1f27 | 1 | DATA_S0#3 | 1 | 2 | strip + portraits | portraits: Metal Gear |
| SP07 | L_00a1 | 1 | DATA_K1#5 | 8 | 29 | strip + portraits 75%, empty strip 25% | text: Metal Gear; portraits: Gillian+Metal Gear |
| SP07 | L_0488 | 1 | DATA_K1#0 DATA_K1#1 | 7 | 18 | strip + portraits | portraits: Gillian+Metal Gear |
| SP07 | L_08aa | 1 | DATA_K1#7 | 3 | 3 | strip + portraits | portraits: Gillian+Metal Gear |
| SP07 | L_0925 | 1 | DATA_K1#8 | 5 | 12 | strip + portraits | portraits: Gillian+Metal Gear |
| SP07 | L_0dde | 1 | DATA_K1#0 (+2 more) | 5 | 21 | strip + portraits 80%, black 20% | portraits: Gillian+Metal Gear |
| SP07 | L_0ebd | 1 | DATA_K1#10 (+3 more) | 16 | 127 | strip + portraits* 61%, strip + portraits 38% | text: Gillian/Metal Gear; portraits: Metal Gear |
| SP08 | L_006e | 1 | DATA_H02#0 | 6 | 21 | strip + portraits | portraits: Gillian+Metal Gear+man (brown beard) |
| SP08 | L_124e | 1 | DATA_H03#0 | 10 | 40 | strip + portraits | text: Metal Gear; portraits: Gillian+Metal Gear+man (red cap) |
| SP08 | L_1ab3 | 1 | DATA_H05#0 | 3 | 9 | strip + portraits | portraits: Gillian+Metal Gear |
| SP09 | L_08b0 | 1 | DATA_G0#4 DATA_G0#6 | 14 | 55 | strip + portraits 66%, black 33% | portraits: Gillian+Metal Gear+woman (blue hair) |
| SP09 | L_09bd | 1 | DATA_G0#4 DATA_G0#6 | 2 | 7 | black | ? |
| SP09 | L_0b3f | 1 | DATA_G0#8 | 3 | 6 | strip + portraits | portraits: Gillian+woman (blue hair) |
| SP09 | L_1201 | 1 | DATA_G0#11 DATA_G0#13 | 2 | 7 | black | ? |
| SP10 | L_0042 | 1 | DATA_A0#2 | 2 | 7 | not probed | ? |
| SP10 | L_05bd | 1 | DATA_A0#0 (+2 more) | 33 | 119 | strip + portraits* 54%, black 45% | ? |
| SP11 | L_00be | 1 | DATA_J0#0 DATA_J0#1 | 23 | 74 | black | ? |
| SP12 | L_000c | 1 | DATA_O0#0 DATA_O0#1 | 1 | 4 | empty strip | ? |
| SP12 | L_00b5 | 1 | DATA_O0#0 DATA_O0#1 | 2 | 11 | black 50%, empty strip 50% | ? |
| SP13 | L_079d | 1 | DATA_O0#2 | 2 | 14 | empty strip | ? |
| SP13 | L_0835 | 1 | DATA_O0#2 | 1 | 5 | empty strip | ? |
| SP14 | L_0b16 | 1 | DATA_I0#3 DATA_I0#4 | 4 | 9 | strip + portraits 75%, empty strip 25% | portraits: Gillian+Metal Gear |
| SP14 | L_0b42 | 1 | DATA_I0#5 | 7 | 22 | strip + portraits 71%, empty strip 28% | portraits: Metal Gear |
| SP14 | L_0e21 | 1 | DATA_I0#7 | 6 | 25 | strip + portraits | portraits: Gillian+Metal Gear |
| SP14 | L_1061 | 1 | DATA_I0#5 | 3 | 21 | empty strip | ? |
| SP14 | L_107b | 1 | DATA_I0#4 | 2 | 18 | strip + portraits | portraits: Metal Gear |
| SP14 | L_10a9 | 1 | DATA_I0#6 | 1 | 4 | strip + portraits | portraits: Metal Gear |
| SP14 | L_10d1 | 1 | DATA_I0#4 | 7 | 33 | strip + portraits 85%, black 14% | portraits: Gillian+man (red face) |
| SP14 | L_1134 | 1 | DATA_I0#1 | 2 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP15 | L_04a0 | 1 | DATA_F0#2 | 3 | 6 | strip + portraits 66%, empty strip 33% | portraits: Gillian+Metal Gear |
| SP15 | L_04cc | 1 | DATA_F0#2 | 6 | 17 | strip + portraits | portraits: Gillian+Metal Gear |
| SP15 | L_068f | 1 | DATA_F0#4 | 7 | 24 | strip + portraits 85%, black 14% | portraits: Gillian+Metal Gear |
| SP15 | L_0da0 | 1 | DATA_F4#0 | 3 | 4 | strip + portraits | text: Metal Gear; portraits: Gillian+Metal Gear |
| SP15 | L_0def | 1 | DATA_F0#5 (+2 more) | 8 | 19 | strip + portraits 87%, empty strip 12% | portraits: Gillian+Metal Gear |
| SP15 | L_10b3 | 1 | DATA_F0#10 | 1 | 5 | empty strip | ? |
| SP15 | L_10c9 | 1 | DATA_F0#10 DATA_F0#11 | 3 | 9 | strip + portraits 66%, empty strip 33% | portraits: Gillian+Metal Gear |
| SP15 | L_10ef | 1 | DATA_F0#11 | 4 | 13 | strip + portraits 75%, black 25% | portraits: Gillian+Metal Gear |
| SP15 | L_115e | 1 | DATA_F0#0 | 5 | 12 | strip + portraits | portraits: Gillian+Metal Gear |
| SP16 | L_0073 | 1 | DATA_F4#2 | 1 | 2 | no strip | portraits: blank icon |
| SP18 | L_0435 | 1 | DATA_M0#1 | 3 | 12 | strip + portraits 66%, empty strip 33% | portraits: Gillian |
| SP20 | L_0004 | 2 | DATA_H15#1 | 1 | 4 | empty strip | ? |
| SP21 | L_10a1 | 2 | DATA_H03#0 | 12 | 58 | strip + portraits 83%, strip + portraits* 16% | portraits: Gillian+man (red cap) |
| SP23 | L_0050 | 2 | DATA_A0#0 (+2 more) | 15 | 40 | black, strip + portraits* 6% | text: Gillian/Metal Gear |
| SP23 | L_0429 | 2 | DATA_A0#3 | 2 | 16 | strip + portraits | portraits: Gillian |
| SP23 | L_0452 | 2 | DATA_A0#3 | 6 | 21 | strip + portraits 83%, empty strip 16% | portraits: Gillian |
| SP23 | L_0801 | 2 | DATA_A0#1 | 4 | 16 | strip + portraits | portraits: Gillian+Metal Gear |
| SP23 | L_08ac | 2 | DATA_A0#0 | 23 | 75 | strip + portraits | portraits: Gillian+Metal Gear+woman (black hair) |
| SP24 | L_0371 | 2 | DATA_B0#0 DATA_B0#2 | 7 | 23 | strip + portraits 75%, empty strip 25% | portraits: Gillian+Metal Gear |
| SP25 | L_03fc | 2 | DATA_G0#10 DATA_G0#9 | 11 | 42 | strip + portraits | portraits: Gillian+Metal Gear |
| SP26 | L_003d | 2 | DATA_Q3#4 DATA_Q3#6 | 6 | 14 | black | ? |
| SP26 | L_0c11 | 2 | DATA_Q3#13 | 12 | 52 | not probed | ? |
| SP26 | L_13bf | 2 | DATA_Q3#14 | 3 | 15 | strip + portraits | portraits: Gillian+Metal Gear+man (blond hair) |
| SP26 | L_13e5 | 2 | DATA_Q3#14 | 4 | 10 | strip + portraits | portraits: Gillian+Metal Gear+man (blond hair) |
| SP26 | L_1492 | 2 | DATA_Q3#14 | 1 | 6 | empty strip | ? |
| SP27 | L_001f | 2 | DATA_O0#0 | 2 | 11 | black 50%, empty strip 50% | ? |
| SP27 | L_0354 | 2 | DATA_O0#5 | 20 | 103 | strip + portraits* 55%, strip + portraits 40% | portraits: Gillian+Metal Gear+beast head |
| SP27 | L_03db | 2 | DATA_O0#5 | 20 | 74 | strip + portraits 70%, strip + portraits* 25% | portraits: Gillian+Metal Gear |
| SP28 | L_0004 | 2 | DATA_P0#3 | 2 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP28 | L_01d0 | 2 | DATA_P0#4 | 4 | 9 | strip + portraits | portraits: Gillian+Metal Gear |
| SP28 | L_037e | 2 | DATA_P0#5 DATA_P0#7 | 5 | 12 | strip + portraits 80%, empty strip 20% | portraits: Gillian+Metal Gear+man (dark curly hair) |
| SP28 | L_0472 | 2 | DATA_P0#6 | 19 | 62 | strip + portraits 52%, strip + portraits* 31% | portraits: Metal Gear |
| SP28 | L_07cf | 2 | DATA_P0#1 DATA_P0#6 | 10 | 44 | strip + portraits | portraits: Gillian+Metal Gear+woman (blue hair) |
| SP29 | L_00d1 | 2 | DATA_S2#1 DATA_S2#4 | 10 | 26 | empty strip 50%, strip + portraits 50% | portraits: Gillian+Metal Gear |
| SP30 | L_0020 | 2 | DATA_Q3#7 DATA_Q3#8 | 8 | 34 | strip + portraits | portraits: Gillian+Metal Gear+man (blond hair) |
| SP30 | L_00a2 | 2 | DATA_Q3#7 DATA_Q3#8 | 8 | 41 | not probed | ? |
| SP30 | L_08c1 | 2 | DATA_Q3#1 (+2 more) | 9 | 35 | strip + portraits 77%, empty strip 22% | portraits: Gillian+Metal Gear+man (blond hair) |
| SP30 | L_0d19 | 2 | DATA_Q5#0 DATA_Q5#2 | 13 | 75 | strip + portraits 69%, strip + portraits* 30% | portraits: Gillian+Metal Gear+man (blond hair) |
| SP31 | L_0014 | 2 | DATA_Q5#3 | 12 | 71 | strip + portraits 58%, strip + portraits* 41% | portraits: Metal Gear |
| SP31 | L_00c4 | 2 | DATA_Q5#3 DATA_Q5#4 | 8 | 30 | strip + portraits | portraits: Gillian+Metal Gear |
| SP31 | L_0127 | 2 | DATA_Q5#4 DATA_Q5#5 | 10 | 38 | strip + portraits | portraits: Gillian+Metal Gear+man (blond hair) |
| SP31 | L_0196 | 2 | DATA_Q5#5 DATA_Q5#6 | 8 | 31 | strip + portraits | portraits: Gillian+Metal Gear+man (blond hair) |
| SP31 | L_0209 | 2 | DATA_Q5#7 | 2 | 3 | empty strip 50%, strip + portraits 50% | portraits: man (grey hair) |
| SP31 | L_0223 | 2 | DATA_Q5#0 DATA_Q5#1 | 8 | 41 | strip + portraits | portraits: Gillian+Metal Gear+man (grey hair) |
| SP31 | L_026f | 2 | DATA_Q5#7 | 1 | 3 | empty strip | ? |
| SP31 | L_0285 | 2 | DATA_Q5#0 | 1 | 4 | black | ? |
| SP31 | L_02a4 | 2 | DATA_Q8#16 | 3 | 5 | black 66%, empty strip 33% | ? |
| SP31 | L_02c2 | 2 | DATA_Q8#13 | 1 | 1 | empty strip | ? |
| SP31 | L_032b | 2 | DATA_Q8#15 | 1 | 1 | empty strip | ? |
| SP32 | L_0008 | 2 | DATA_U0#0 | 14 | 58 | black 57%, black* 42% | ? |
| SP32 | L_06da | 2 | DATA_U0#2 | 11 | 57 | strip + portraits 72%, strip + portraits* 27% | portraits: Gillian+Metal Gear |
| SP32 | L_0775 | 2 | DATA_U0#3 | 1 | 3 | strip + portraits | text: Gillian/Metal Gear; portraits: Gillian+Metal Gear |
| SP32 | L_08aa | 2 | DATA_U0#4 | 4 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP32 | L_08e5 | 2 | DATA_F0#5 | 8 | 44 | strip + portraits | portraits: Gillian+Metal Gear |
| SP32 | L_0e04 | 2 | DATA_T0#1 | 7 | 28 | empty strip | ? |
| SP33 | L_0255 | 2 | DATA_H11#2 | 23 | 57 | empty strip*, empty strip 4% | ? |
| SP33 | L_0c2b | 2 | DATA_H02#0 (+2 more) | 10 | 19 | black | ? |
| SP33 | L_0dbd | 2 | DATA_H11#1 | 8 | 14 | strip + portraits | portraits: Gillian+Metal Gear |
| SP33 | L_0e20 | 2 | DATA_H13#0 | 11 | 34 | strip + portraits, empty strip 9% | portraits: Gillian+Metal Gear |
| SP33 | L_0ed4 | 2 | DATA_H13#0 DATA_Q3#3 | 1 | 1 | empty strip | ? |
| SP34 | L_001a | 2 | DATA_S1#0 DATA_S1#1 | 9 | 21 | not probed | ? |
| SP35 | L_0789 | 2 | DATA_M0#7 | 1 | 4 | empty strip | ? |
| SP36 | L_0004 | 3 | DATA_H15#2 | 1 | 8 | empty strip | ? |
| SP36 | L_003c | 3 | DATA_H15#3 | 1 | 13 | empty strip | ? |
| SP36 | L_0051 | 3 | DATA_Y00#4 (+2 more) | 18 | 71 | strip + portraits 55%, strip + portraits* 38% | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_00d9 | 3 | DATA_Y00#1 | 2 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP36 | L_00fc | 3 | DATA_Y00#5 | 1 | 5 | empty strip | ? |
| SP36 | L_010e | 3 | DATA_Y00#3 | 1 | 6 | strip + portraits | portraits: Metal Gear |
| SP36 | L_011c | 3 | DATA_Y00#5 DATA_Y00#9 | 13 | 45 | strip + portraits 69%, empty strip 30% | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_0186 | 3 | DATA_Y00#8 DATA_Y00#9 | 3 | 11 | strip + portraits 66%, empty strip 33% | portraits: Metal Gear+woman (auburn hair) |
| SP36 | L_01b3 | 3 | DATA_Y00#7 | 3 | 16 | strip + portraits | portraits: Gillian+Metal Gear |
| SP36 | L_01d8 | 3 | DATA_Y00#5 | 1 | 5 | strip + portraits | portraits: Metal Gear |
| SP36 | L_01e6 | 3 | DATA_Y00#6 | 3 | 12 | strip + portraits | portraits: Metal Gear+woman (auburn hair) |
| SP36 | L_0225 | 3 | DATA_Y00#2 | 1 | 3 | empty strip | ? |
| SP36 | L_0233 | 3 | DATA_Y00#5 DATA_Y00#9 | 6 | 25 | strip + portraits | portraits: Gillian+Metal Gear |
| SP36 | L_026f | 3 | DATA_Y00#5 DATA_Y00#9 | 11 | 45 | strip + portraits | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_02bd | 3 | DATA_Y00#0 | 3 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP36 | L_02e6 | 3 | DATA_Y00#5 DATA_Y00#9 | 6 | 20 | strip + portraits 66%, empty strip 33% | text: Metal Gear; portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_071a | 3 | DATA_Y01#10 DATA_Y01#7 | 7 | 25 | strip + portraits 71%, empty strip 28% | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_0754 | 3 | DATA_Y01#5 | 1 | 4 | strip + portraits | portraits: woman (auburn hair) |
| SP36 | L_0762 | 3 | DATA_Y01#7 | 3 | 12 | empty strip 66%, strip + portraits 33% | portraits: Metal Gear |
| SP36 | L_077c | 3 | DATA_Y01#6 | 6 | 31 | strip + portraits | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_07c1 | 3 | DATA_Y01#6 | 16 | 46 | strip + portraits 87%, strip + portraits* 12% | portraits: Gillian+Metal Gear+woman (auburn hair) |
| SP36 | L_086e | 3 | DATA_Y01#10 | 2 | 2 | strip + portraits | portraits: Metal Gear+woman (auburn hair) |
| SP37 | L_0101 | 3 | DATA_S0#2 DATA_S0#3 | 5 | 13 | empty strip 60%, strip + portraits 40% | portraits: Metal Gear |
| SP37 | L_0174 | 3 | DATA_S0#4 | 4 | 13 | strip + portraits | portraits: Gillian+Metal Gear |
| SP37 | L_01d8 | 3 | DATA_S0#4 | 1 | 3 | strip + portraits | portraits: Gillian+Metal Gear |
| SP37 | L_01f8 | 3 | DATA_S0#4 | 1 | 1 | strip + portraits | portraits: Metal Gear |
| SP37 | L_02a5 | 3 | DATA_Y03#0 | 8 | 21 | strip + portraits 87%, black 12% | portraits: Gillian+Metal Gear |
| SP37 | L_088d | 3 | DATA_Y04#4 DATA_Y04#6 | 14 | 78 | strip + portraits 57%, strip + portraits* 42% | portraits: Gillian+Metal Gear |
| SP37 | L_0a9e | 3 | DATA_Y04#4 DATA_Y04#5 | 6 | 27 | strip + portraits | portraits: Gillian+Metal Gear |
| SP37 | L_0b08 | 3 | DATA_Y04#1 | 1 | 4 | empty strip | ? |
| SP37 | L_0b16 | 3 | DATA_Y04#3 | 9 | 55 | strip + portraits 77%, strip + portraits* 22% | portraits: Gillian+Metal Gear |
| SP37 | L_0b54 | 3 | DATA_Y04#0 | 1 | 5 | empty strip | ? |
| SP37 | L_0b66 | 3 | DATA_Y04#2 | 1 | 5 | empty strip | ? |
| SP37 | L_0b7c | 3 | DATA_Y04#3 | 2 | 6 | strip + portraits | portraits: Gillian+Metal Gear |
| SP37 | L_0b98 | 3 | DATA_Y04#3 | 2 | 8 | strip + portraits | portraits: Gillian+Metal Gear |
| SP37 | L_0bb0 | 3 | DATA_Y04#0 | 1 | 2 | empty strip | ? |
| SP37 | L_0bc6 | 3 | DATA_Y04#2 | 3 | 7 | strip + portraits 66%, empty strip 33% | portraits: Gillian |
| SP37 | L_0be4 | 3 | DATA_Y04#0 DATA_Y04#2 | 3 | 9 | empty strip | ? |
| SP37 | L_0c11 | 3 | DATA_Y04#2 DATA_Y05#0 | 4 | 11 | strip + portraits 50%, black 25% | portraits: Gillian |
| SP37 | L_0c6c | 3 | DATA_Y05#1 DATA_Y05#2 | 5 | 11 | strip + portraits 80%, empty strip 20% | portraits: Gillian |
| SP37 | L_0cd1 | 3 | DATA_Y05#2 | 2 | 12 | empty strip 50%, strip + portraits 50% | portraits: Gillian |
| SP37 | L_0f0b | 3 | DATA_Y05#3 DATA_Y05#4 | 2 | 6 | empty strip | ? |
| SP37 | L_0f55 | 3 | DATA_Y05#5 | 1 | 8 | no strip | portraits: (picture - not an icon) |
| SP38 | L_003d | 3 | DATA_Y06#2 DATA_Y06#5 | 14 | 52 | strip + portraits 57%, empty strip 28% | portraits: Gillian+woman (auburn hair) |
| SP38 | L_00ab | 3 | DATA_Y06#1 | 6 | 27 | strip + portraits | portraits: Gillian+woman (auburn hair) |
| SP38 | L_00e5 | 3 | DATA_Y06#2 | 3 | 20 | empty strip | ? |
| SP38 | L_010b | 3 | DATA_Y06#4 | 7 | 59 | strip + portraits 85%, strip + portraits* 14% | portraits: woman (blond hair) |
| SP38 | L_0150 | 3 | DATA_Y06#0 | 5 | 35 | strip + portraits | portraits: woman (auburn hair) |
| SP38 | L_017e | 3 | DATA_Y06#6 | 8 | 37 | strip + portraits | portraits: woman (blond hair) |
| SP38 | L_01d2 | 3 | DATA_Y06#7 | 2 | 14 | strip + portraits | portraits: woman (blond hair) |
| SP38 | L_01ed | 3 | DATA_Y06#2 | 1 | 4 | empty strip | ? |
| SP38 | L_01fb | 3 | DATA_Y06#6 | 3 | 14 | strip + portraits 66%, empty strip 33% | portraits: woman (blond hair) |
| SP38 | L_0219 | 3 | DATA_Y06#3 | 4 | 23 | strip + portraits | portraits: Gillian+woman (blond hair) |
| SP38 | L_0258 | 3 | DATA_Y06#2 | 3 | 8 | empty strip | ? |
| SP38 | L_027c | 3 | DATA_Y07#1 (+4 more) | 14 | 72 | strip + portraits 70%, empty strip 20% | portraits: Gillian+old man (grey hair)+woman (blond hair) |
| SP38 | L_0319 | 3 | DATA_Y07#0 | 6 | 55 | strip + portraits 83%, strip + portraits* 16% | portraits: old man (grey hair)+woman (blond hair) |
| SP38 | L_0358 | 3 | DATA_Y08#6 | 2 | 10 | empty strip | ? |
| SP38 | L_0374 | 3 | DATA_Y08#5 | 3 | 14 | strip + portraits | portraits: old man (grey hair) |
| SP38 | L_039d | 3 | DATA_Y08#1 | 4 | 19 | strip + portraits | portraits: old man (grey hair)+woman (blond hair) |
| SP38 | L_03cc | 3 | DATA_Y08#0 DATA_Y08#2 | 5 | 39 | strip + portraits | portraits: old man (grey hair) |
| SP38 | L_0409 | 3 | DATA_Y08#3 | 4 | 29 | empty strip 50%, strip + portraits 50% | portraits: woman (blond hair) |
| SP38 | L_0429 | 3 | DATA_Y08#4 DATA_Y09#3 | 5 | 59 | strip + portraits 80%, strip + portraits* 20% | portraits: old man (grey hair) |
| SP38 | L_0466 | 3 | DATA_Y09#4 | 3 | 14 | strip + portraits | portraits: Gillian+old man (grey hair) |
| SP38 | L_0493 | 3 | DATA_Y09#3 | 2 | 14 | strip + portraits | portraits: old man (grey hair)+woman (blond hair) |
| SP38 | L_04ae | 3 | DATA_Y09#2 | 3 | 24 | strip + portraits | portraits: Gillian+old man (grey hair)+woman (blond hair) |
| SP38 | L_04d3 | 3 | DATA_Y09#1 DATA_Y09#3 | 6 | 39 | strip + portraits 83%, empty strip 16% | portraits: Gillian+old man (grey hair)+woman (blond hair) |
| SP38 | L_0512 | 3 | DATA_Y09#0 | 7 | 53 | strip + portraits 85%, strip + portraits* 14% | portraits: Gillian+old man (grey hair) |
| SP38 | L_0557 | 3 | DATA_Y10#5 | 3 | 18 | empty strip | ? |
| SP38 | L_0579 | 3 | DATA_Y10#0 DATA_Y10#4 | 6 | 40 | strip + portraits | portraits: Gillian+old man (grey hair)+woman (blond hair) |
| SP38 | L_05b4 | 3 | DATA_Y10#2 | 3 | 14 | strip + portraits | portraits: old man (grey hair)+woman (auburn hair) |
| SP38 | L_05ce | 3 | DATA_Y10#5 | 2 | 12 | empty strip | ? |
| SP38 | L_05ea | 3 | DATA_Y10#3 | 4 | 28 | strip + portraits 75%, empty strip 25% | portraits: old man (grey hair) |
| SP38 | L_060a | 3 | DATA_Y10#1 DATA_Y10#5 | 9 | 77 | empty strip 33%, strip + portraits 33% | portraits: Gillian+old man (grey hair) |
| SP38 | L_0663 | 3 | DATA_Y10#3 | 1 | 4 | empty strip | ? |
| SP38 | L_0671 | 3 | DATA_Y10#2 DATA_Y11#4 | 4 | 25 | strip + portraits | portraits: Gillian+old man (grey hair) |
| SP38 | L_0695 | 3 | DATA_Y11#1 | 1 | 3 | strip + portraits | portraits: old man (grey hair) |
| SP38 | L_06d2 | 3 | DATA_Y11#0 | 1 | 1 | strip + portraits | portraits: Gillian |
| SP38 | L_06e4 | 3 | DATA_Y11#2 | 6 | 39 | strip + portraits | portraits: Gillian+old man (grey hair) |
| SP38 | L_071c | 3 | DATA_Y11#4 | 3 | 33 | empty strip | ? |
| SP38 | L_0742 | 3 | DATA_Y11#0 | 3 | 19 | strip + portraits | portraits: Gillian+old man (grey hair) |
| SP38 | L_076c | 3 | DATA_Y11#3 | 1 | 3 | empty strip | ? |
| SP38 | L_077a | 3 | DATA_Y11#4 | 3 | 23 | empty strip | ? |
| SP38 | L_0798 | 3 | DATA_Y12#3 | 3 | 20 | empty strip | ? |
| SP38 | L_07b2 | 3 | DATA_Y12#2 | 2 | 16 | empty strip | ? |
| SP38 | L_07d2 | 3 | DATA_Y12#0 DATA_Y12#1 | 4 | 13 | strip + portraits | portraits: Gillian+Metal Gear |
| SP38 | L_07fa | 3 | DATA_Y12#4 | 1 | 9 | empty strip | ? |
| SP38 | L_0808 | 3 | DATA_Y12#5 DATA_Y12#6 | 3 | 14 | strip + portraits 66%, empty strip 33% | portraits: Metal Gear+woman (auburn hair) |
| SP38 | L_0834 | 3 | DATA_Y12#1 | 2 | 12 | empty strip | ? |
| SP38 | L_0848 | 3 | DATA_Y12#4 DATA_Y12#6 | 3 | 14 | strip + portraits 66%, empty strip 33% | portraits: Gillian+Metal Gear |
| SP38 | L_0874 | 3 | DATA_Y12#6 | 2 | 14 | strip + portraits | portraits: Gillian+Metal Gear+old man (grey hair) |
| SP38 | L_08ab | 3 | DATA_Y13#0 | 1 | 1 | empty strip | ? |
| SP38 | L_08f3 | 3 | DATA_Y13#7 | 5 | 10 | strip + portraits 80%, empty strip 20% | portraits: old man (grey hair) |
| SP38 | L_094e | 3 | DATA_Y13#2 DATA_Y13#6 | 4 | 19 | empty strip 50%, strip + portraits 50% | portraits: cyborg face+old man (grey hair) |
| SP38 | L_0988 | 3 | DATA_Y13#7 | 2 | 5 | empty strip 50%, strip + portraits 50% | portraits: cyborg face |
| SP38 | L_099c | 3 | DATA_Y14#6 | 1 | 6 | empty strip | ? |
| SP38 | L_09aa | 3 | DATA_Y14#4 | 6 | 30 | empty strip | ? |
| SP38 | L_09d6 | 3 | DATA_Y14#10 | 1 | 5 | empty strip | ? |
| SP38 | L_09e4 | 3 | DATA_Y14#3 DATA_Y14#5 | 2 | 5 | empty strip | ? |
| SP38 | L_09fc | 3 | DATA_Y14#9 | 2 | 9 | empty strip 50%, strip + portraits 50% | portraits: cyborg face |
| SP38 | L_0a10 | 3 | DATA_Y14#6 DATA_Y14#8 | 5 | 16 | strip + portraits 80%, empty strip 20% | portraits: Gillian+Metal Gear |
| SP38 | L_0a50 | 3 | DATA_Y14#9 | 2 | 8 | black 50%, empty strip 50% | ? |
| SP38 | L_0a6f | 3 | DATA_Y14#4 | 3 | 15 | empty strip | ? |
| SP38 | L_0a8d | 3 | DATA_Y14#3 | 1 | 4 | empty strip | ? |
| SP38 | L_0a9b | 3 | DATA_Y14#0 | 1 | 5 | strip + portraits | portraits: Metal Gear |
| SP38 | L_0ab8 | 3 | DATA_Y14#4 | 1 | 4 | empty strip | ? |
| SP38 | L_0ac6 | 3 | DATA_Y14#3 | 1 | 3 | empty strip | ? |
| SP38 | L_0ad8 | 3 | DATA_Y14#6 | 1 | 4 | empty strip | ? |
| SP38 | L_0ae6 | 3 | DATA_Y14#3 | 1 | 1 | empty strip | ? |
| SP38 | L_0e26 | 3 | DATA_Y15#8 | 5 | 25 | empty strip 80%, empty strip* 20% | ? |

## Appendix B: triggers that are not plainly voice-only (generated)

| id | seconds | class | file | pc | relation | text offset | speaker of that text |
|---|---:|---|---|---|---|---|---|
| 48 | 6.6 | partial | SP00 | 00b0 | text-in-picture |  |  |
| 367 | 5.9 | unknown | SP01 | 0459 | text-before | 05a3 | Metal Gear |
| 369 | 1.5 | unknown | SP01 | 046b | text-after-wait | 05be | Metal Gear |
| 45 | 7.2 | unknown | SP03 | 07be | text-after-wait | 2093 | Gillian |
| 46 | 5.4 | unknown | SP03 | 08c5 | text-after-wait | 247a | Metal Gear |
| 37 | 3.4 | unknown | SP07 | 044a | text-after-wait | 0c40 | Metal Gear |
| 140 | 2.9 | unknown | SP07 | 08db | text-after-wait | 1675 | Metal Gear |
| 169 | 2.7 | unknown | SP07 | 0a00 | text-after-wait | 189a | Metal Gear |
| 161 | 1.3 | unknown | SP07 | 0ee0 | text-after-wait | 22d4 | Metal Gear |
| 162 | 1.0 | unknown | SP07 | 0f0a | text-before | 22d4 | Metal Gear |
| 163 | 2.8 | unknown | SP07 | 0f10 | text-after-wait | 2405 | Metal Gear |
| 144 | 1.3 | unknown | SP07 | 100b | text-after-wait | 2598 | Metal Gear |
| 164 | 1.9 | unknown | SP07 | 155b | text-after-wait | 2f62 | Metal Gear |
| 165 | 2.6 | unknown | SP07 | 1564 | text-after-wait | 2f62 | Metal Gear |
| 166 | 3.6 | unknown | SP07 | 156a | text-after-wait | 2f62 | Metal Gear |
| 48 | 6.6 | partial | SP07 | 1a02 | text-in-picture |  |  |
| 181 | 5.0 | unknown | SP08 | 1266 | text-before | 4774 | Metal Gear |
| 195 | 14.6 | unknown | SP08 | 1cc8 | text-after-wait | 6d31 | Gillian |
| 49 | 2.1 | unknown | SP09 | 0f46 | text-after-wait | 2829 | Metal Gear |
| 221 | 2.1 | with-text | SP09 | 1001 | text-in-picture |  |  |
| 222 | 7.5 | with-text | SP09 | 1007 | text-in-picture |  |  |
| 223 | 11.1 | with-text | SP09 | 1018 | text-in-picture |  |  |
| 224 | 12.9 | with-text | SP09 | 1029 | text-in-picture |  |  |
| 225 | 4.4 | with-text | SP09 | 103a | text-in-picture |  |  |
| 226 | 13.3 | with-text | SP09 | 104b | text-in-picture |  |  |
| 227 | 6.0 | with-text | SP09 | 105c | text-in-picture |  |  |
| 228 | 10.6 | with-text | SP09 | 1062 | text-in-picture |  |  |
| 229 | 15.5 | with-text | SP09 | 1082 | text-in-picture |  |  |
| 230 | 4.7 | with-text | SP09 | 1093 | text-in-picture |  |  |
| 231 | 9.4 | with-text | SP09 | 10a4 | text-in-picture |  |  |
| 232 | 3.9 | with-text | SP09 | 10b5 | text-in-picture |  |  |
| 233 | 14.2 | with-text | SP09 | 10c6 | text-in-picture |  |  |
| 234 | 4.2 | with-text | SP09 | 10d7 | text-in-picture |  |  |
| 235 | 4.8 | with-text | SP09 | 10e1 | text-in-picture |  |  |
| 238 | 5.5 | unknown | SP09 | 1837 | text-before | 40e8 | Gillian |
| 241 | 6.0 | unknown | SP10 | 011a | text-after-wait | 0554 | Gillian |
| 37 | 3.4 | unknown | SP11 | 0185 | text-before | 0614 | Metal Gear |
| 54 | 2.2 | unknown | SP11 | 0908 | text-before | 268c | Employee |
| 54 | 2.2 | unknown | SP11 | 0932 | text-after-wait | 26ef | Employee |
| 37 | 3.4 | unknown | SP12 | 0196 | text-before | 0503 | Doorman |
| 54 | 2.2 | unknown | SP13 | 007e | text-after-wait | 0419 | Manager |
| 36 | 2.8 | unknown | SP14 | 00c3 | text-before | 03be | Gillian |
| 48 | 6.6 | partial | SP14 | 15b6 | text-in-picture |  |  |
| 36 | 2.8 | unknown | SP15 | 00b3 | text-before | 03d3 | Gillian |
| 367 | 5.9 | unknown | SP15 | 0db5 | text-before | 2d30 | Metal Gear |
| 48 | 6.6 | partial | SP15 | 15ed | text-in-picture |  |  |
| 48 | 6.6 | partial | SP16 | 0383 | text-in-picture |  |  |
| 48 | 6.6 | partial | SP16 | 03e1 | text-in-picture |  |  |
| 1193 | 11.5 | partial | SP18 | 041b | text-in-picture |  |  |
| 1188 | 0.9 | unknown | SP18 | 04b7 | text-after-wait | 09eb | Metal Gear |
| 1189 | 4.5 | unknown | SP18 | 08ef | text-after-wait | 1147 | Gillian |
| 1189 | 4.5 | unknown | SP18 | 095d | text-after-wait | 1303 | Gillian |
| 1190 | 3.5 | unknown | SP18 | 0997 | text-after-wait | 1303 | Gillian |
| 1191 | 6.7 | partial | SP18 | 157a | text-in-picture |  |  |
| 414 | 4.1 | unknown | SP23 | 0259 | text-after-wait | 07eb | Metal Gear |
| 415 | 2.2 | unknown | SP23 | 03c5 | text-before | 0c9c | Metal Gear |
| 419 | 2.5 | unknown | SP23 | 07bf | text-after-wait | 1d51 | Gillian |
| 420 | 2.5 | unknown | SP23 | 07c5 | text-after-wait | 1d51 | Gillian |
| 57 | 1.5 | unknown | SP24 | 05dc | text-after-wait | 1c89 | Metal Gear |
| 44 | 1.4 | unknown | SP24 | 05f6 | text-after-wait | 1c89 | Metal Gear |
| 56 | 1.2 | unknown | SP24 | 0611 | text-after-wait | 1c89 | Metal Gear |
| 36 | 2.8 | unknown | SP26 | 02e8 | text-after-wait | 0c17 | Metal Gear |
| 456 | 4.0 | unknown | SP28 | 0045 | text-after-wait | 0293 | Metal Gear |
| 59 | 2.2 | unknown | SP29 | 0265 | text-after-wait | 061f | Metal Gear |
| 61 | 3.9 | unknown | SP29 | 02b8 | text-after-wait | 06b0 | Metal Gear |
| 61 | 3.9 | unknown | SP29 | 0313 | text-after-wait | 076d | Metal Gear |
| 58 | 5.9 | unknown | SP29 | 034e | text-after-wait | 07d2 | Metal Gear |
| 62 | 7.0 | unknown | SP29 | 038d | text-after-wait | 085f | Gillian |
| 68 | 4.6 | unknown | SP30 | 0158 | text-before | 0293 | Gillian |
| 594 | 5.1 | unknown | SP30 | 0a4e | text-after-wait | 1af0 | Random |
| 48 | 6.6 | partial | SP30 | 100d | text-in-picture |  |  |
| 38 | 3.7 | unknown | SP32 | 06ce | text-before | 1874 | Gillian |
| 688 | 1.6 | unknown | SP32 | 088d | text-before | 1dac | Metal Gear |
| 48 | 6.6 | partial | SP32 | 0ed7 | text-in-picture |  |  |
| 709 | 5.1 | unknown | SP33 | 0244 | text-before | 0ad1 | Metal Gear |
| 48 | 6.6 | partial | SP33 | 1236 | text-in-picture |  |  |
| 48 | 6.6 | partial | SP33 | 12c0 | text-in-picture |  |  |
| 1192 | 12.3 | unknown | SP35 | 06ef | text-after-wait | 0c44 | Gillian |
| 1195 | 10.7 | unknown | SP35 | 07c8 | text-after-wait | 0fdc | Gillian |
| 1196 | 5.7 | unknown | SP35 | 087a | text-after-wait | 12a3 | Gillian |
| 1197 | 6.0 | unknown | SP35 | 0880 | text-after-wait | 12a3 | Gillian |
| 1193 | 11.5 | partial | SP35 | 11c3 | text-in-picture |  |  |
| 844 | 2.7 | unknown | SP36 | 031e | text-after-wait | 00a7 | Metal Gear |
| 40 | 6.1 | unknown | SP37 | 04f4 | text-before | 0cfa | Metal Gear |
| 40 | 6.1 | unknown | SP37 | 06d9 | text-before | 1448 | Metal Gear |
