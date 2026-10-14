# Audio – RE notes

Conventions as in docs/RE_NOTES.md. S: = sub CPU address, M: = main CPU address.
Listings: `re/out/dis/audio_sub.asm`, `re/out/dis/audio_main.asm`, regenerated with
`python3 re/dis68k.py re/images/sub_prg.bin 6000 're/labels/sub_*.txt' -o re/out/dis/audio_sub.asm`
(and `main_ip.bin ff0000 're/labels/main_*.txt'`). Labels: `re/labels/{sub,main}_30_audio.txt`.

## 0. Overview – who makes which sound

| Kind | Chip | Driver | Data | Trigger (script op → sequencer cmd → code) |
|---|---|---|---|---|
| Voices (speech, 1162 clips, 83 min) | RF5C164 PCM, streamed from CD | S: PCM driver 0x15100–0x166xx | PCMLT_01 (table) + PCMLD_01 (data) | op 0x04 `id` → cmd 0x0D (S:0x101DC) → `pcm_play_voice` S:0x154F8 |
| PCM sound effects (73) | RF5C164 | same | PCMLT_01+0x4000 (table), PCMDRMDT.BIN (drum kit), kind-1 banks in PCMLD_01 | op 0x05 `id` → cmd 0x0C (S:0x101BC) → `pcm_play_sfx` S:0x1530E; scene code `jsr $D428` |
| FM music (60 songs, ids 0x3F–0x7A) | YM2612 written by the **main 68000** + PCM drums on the sub CPU | Konami 68k driver M:0xFF4000–0xFF6141 | FMWR_1.BIN (Word RAM image) | op 0x0A `id` → cmd 0x07 (S:0x1011E) → `fm_queue_id` S:0x15EB4 (d1=1) → COMSTAT2 → M:0xFF4F96 → `fm_start` M:0xFF5306 |
| FM sound effects (62, ids 0x01–0x3E) | YM2612 ch 5/6 (steals music ch) | same | FMWR_1.BIN | op 0x0F `id` → cmd 0x1C (S:0x1013E) → S:0x15EB4 (d1=0); scene code `jsr $D44C` |
| CD-DA (tracks 2–21) | CD drive | BIOS `_CDBIOS` | disc tracks | op 0x0D `track` → cmd 0x0E (S:0x10238) → `cd_play_track` S:0xE30A; scene code `jmp $D476` |

The Z80 is never run: the IP keeps the Z80 bus requested (M:0xFF0590) and uses Z80 RAM only as
storage for the FM instrument table (copied by `fm_boot_load` M:0xFF56AE); the 68k writes the
YM2612 at 0xA04000/0xA04002 itself (e.g. M:0xFF5F98, M:0xFF5AC6). PSG is only silenced (M:0xFF573C).

## 1. RF5C164 PCM (sub CPU)

### 1.1 Sample format (verified)
8-bit sign-magnitude, bit 7 set = positive. Evidence: smoothness test on 200 KB of PCMLD_01
(mean |Δ|: sign-magnitude 5.8, two's complement 27.8, offset binary 27.4) and decoded voices have
speech-like statistics (zero-crossing ≈ 1–3 kHz, RMS 3–7k, DC ≈ −115). Byte 0xFF = end marker:
the driver copies bytes to wave RAM until 0xFF (S:0x15C32, S:0x15D02 `cmp.b d0,d7`, d0=#$FF),
records that position as the stop point and keys the channel off when reached (S:0x15974–0x159A0
→ S:0x15E24). Decoder: `core::audio::{sm8_to_i16, decode_sm8}`.

Rate: FD register; FD 0x0800 = 1 sample per output tick of 12.5 MHz/384 = 32552 Hz
(**HYPOTHESIS** from RF5C164 documentation; consistent with the authoring rates stored in segment
headers: FD 0x0800 ↔ header 32000, FD 0x0400 ↔ 16000). `core::audio::fd_to_hz`.

### 1.2 PCMLT_01.BIN (loaded at S:0x50000) – voice table
8 bytes per id, id = index; entry 0 is a header (`ae00 0000 0000 0000`).

| byte | meaning | evidence |
|---|---|---|
| 0 | priority → S:$7F807 | S:0x15518 |
| 1 | kind: 1 = load whole entry to S:0x65000 as a PCM sfx *bank*; 2 = streamed voice that ducks music/sfx; 0x82 = streamed voice, no ducking | S:0x1552C, 0x155A4, 0x155C4; ducking S:0x155AA→S:0x15282 |
| 2 | ENV (volume) → $7F808 | S:0x155F6 |
| 3 | PAN → $7F809 (nibbles swapped when written, S:0x154A2) | |
| 4–5 | start sector inside PCMLD_01; driver adds 0x17 = LBA 23 of PCMLD_01 | S:0x15606 `addi.l #$17,d0` |
| 6–7 | length in sectors; kind 1 must be < 0x35 ("pcm.size_over_error!" S:0x15554) | S:0x1554A |

Verified by test `audio::tests::real_tables_if_present`: entries 1..=1197 are contiguous and cover
PCMLD_01 exactly (41381 sectors). Ids 1–35 are kind 1 (banks), 36–1197 kind 2/0x82 (voices).

Streaming (kinds 2/0x82): ROMREADN (`_CDBIOS` 0x20) of 0x20 sectors to S:0x65000 then 0x10 sectors
at a time alternating S:0x65000/S:0x6D000 (S:0x15898/0x158A0); copied to wave RAM by S:0x15CB8.
FD is fixed to 0x0400 (S:0x156B0) → **16276 Hz**; channels 0–3 get identical ENV/PAN/FD/start
(S:0x156E6) → mono. Every voice ends with 0xFF in its last sector (`re/py/audio_ffcheck.py`).

Ducking (kind 2): $7F813 bit0 makes S:0x15282 lower $7F817 by 2 every 2 frames down to 0x9A; when
the voice ends (S:0x15A4E) bit1 raises it again by 2 every 3 frames. $7F817 scales PCM sfx/drum
channels (S:0x15484) and, through the curve S:0x15FBA, is sent to the main CPU as the FM music
volume (COMSTAT1 high byte, S:0x15F74) → music ducks to curve[0x9A]=0xE4/0xFF (≈ −1 dB per the
driver's linear scaling `v*(vol>>1)>>7`, M:0xFF45D8).

### 1.3 PCM sfx table (PCMLT_01 + 0x4000, loaded at S:0x54000)
Byte 0 of entry 0 = count 0x4A (ids ≥ count rejected, S:0x15338). 16 bytes per id:
`[prio][chan 0/1][ENV][PAN][FD u16][addr u32]`; sample data at addr+0x10 (S:0x15402 skips the
segment header). addr 0x55000.. → PCMDRMDT.BIN (loaded at S:0x55000); addr 0x65000.. → the
kind-1 bank currently loaded at S:0x65000. `re/py/audio_sfxmap.py`: every address hits a segment
start; ids 0x01–0x0C = drum kit, 0x0D–0x49 = bank segments (many at bank offset 0 → depend on
the loaded bank). Ids ≥ 0xF8 are commands (S:0x162E0, table S:0x162FC): FC = PCM mono, FD = stereo,
FF = stop all PCM.

### 1.4 Segment chains (PCMDRMDT.BIN, kind-1 banks)
`[u32 n][FFFFFFFF][u16 authoring rate][6×00]` + n bytes (samples incl. 4×FF), next header right
after; n = FFFFFFFF ends the chain. `core::audio::parse_segments` (tested). PCMDRMDT: 12 segments.

### 1.5 PCM drums of the FM music
FM music slots 6/7 are drum tracks (M:0xFF4D22): note n (event 0x60+n) and a 5-bit velocity are
packed into COMCMD5..6 (M:0xFF4DDE, counter in COMCMD5+3). S:0x160BA decodes them and S:0x161F4
plays sfx id n (drum kit 1..12, except 6/8/9/10) on logical channel 2 (slot 6) or 3 (slot 7);
ENV = table ENV × vel<<3 >> 8 (vel 0 keeps the table ENV, S:0x16260). Drum samples are preloaded
into wave RAM from 0x8000 by S:0x16128.

## 2. CD-DA

`cd_play_track` S:0xE30A = `_CDBIOS` 0x12 (MSCPLAY1 – play one track, **HYPOTHESIS** from the BIOS
manual function list) with a0 → track word. No repeat-play call exists (only 0x12), so tracks
play once. Cmd 0x0E (S:0x10238) waits until CDBSTAT (0x81) reports 0x0100 (playing) and clears the
CD-time counter S:$787A; S:0xD5FE copies CDBSTAT absolute/relative time to S:$7876/$787A each
frame; cmd 0x1B (S:0x103D2) waits until $787A reaches its operand (cutscene sync).
Loading a file first calls `_CDBIOS` 3 (S:0xE31A; MSCPAUSEON **HYPOTHESIS**) → data loads
interrupt music.

| track | started by |
|---|---|
| 2 | DATA_D2 code S:0x2886C (`move.w #2,d0; jmp $D476`), DATA_Y16 S:0x2925C |
| 3 | DATA_D1 S:0x29352 |
| 4, 5 | DATA_D0 S:0x2A966, S:0x2A938 |
| 6 | SP08 | 
| 7 | SP16 |
| 8, 9 | SP20 |
| 10, 11 | SP29 |
| 12, 13, 14 | SP33 |
| 15 | SP36 |
| 16, 17, 18 | SP38 |
| 19 | DATA_Y16 S:0x291B0 |
| 20 | SP06 |
| 21 | SP31 |
(script uses: `re/py/audio_scriptuse.py`; DATA_* code addresses assume load at S:0x28000.)

## 3. FM – the Konami 68k driver (main CPU)

### 3.1 Request path
S:0x15EB4 queues ids in a 128-byte ring (S:$7F880); `fm_send_comm` S:0x15F18 sends one per frame
in COMSTAT2 (S:0xFF8024), the volumes (curve S:0x15FBA applied to $7F817 / $7F819) in COMSTAT1
and a change counter in S:0xFF8025. id 0xFF = fade out ($7F815: $7F817 −3 per frame → the
driver mutes when the volume byte < 0x15, M:0xFF40C0). Main: M:0xFF4F96 (vint) reads them,
queues to M:$FF7F00; `fm_update` M:0xFF5106 dequeues one per frame; ids ≥ 0xF1 are commands
(M:0xFF51A8: F1 stop all M:0xFF52AC, F2 stop music M:0xFF51E4, F3 force centre pan (mono),
F4 stereo); others → `fm_start` M:0xFF5306.

### 3.2 Data
Sound table M:0xFF6142, 8 bytes/id: `[type][tracks-1][first slot][prio][u32 index]`;
type low nibble ≠ 0 = music (index = song 0..0x3B), else sfx (index into M:0xFF9000 table).
FMWR_1.BIN = Word RAM image at M:0x200000: 0x0000–0x1DFF instruments (256 longs + 25-byte
patches `[FB/ALG][6 bytes × 4 ops in op1,op2,op3,op4 order: DT/MUL, TL, KS/AR, AM/DR, SR, SL/RR]`)
→ Z80 RAM; 0x1E00–0x2845 sfx bank → M:0xFF9000; 0x2846 song offset table (60 longs) → songs
copied to M:0xFF8000 0x54 longs per frame (`fm_song_copy` M:0xFF55FE; the sequencer is skipped
while copying). Song/sfx header: one long offset per track (relative to the header).

### 3.3 Sequencer
10 track slots of 0x80 bytes at M:0xFF7A00: 0–5 music FM ch 1–6, 6–7 music PCM drums,
8–9 sfx on FM ch 5–6 (they block the music track's chip writes while active: guard at e.g.
M:0xFF5E5E). Per frame (`fm_tick` M:0xFF408A from the vint): tempo accumulator `$05 += $04`,
carry = one tick; events: 0x00 skip, <0x60(+bit7 flag) note [len][gate|vel], 0x60–0xDF drum
notes (slots 6/7), 0xE0–0xFF commands (table M:0xFF400A): E0 rest, E1 tie, E2 instrument,
E3 pan, E4 vibrato, E5 random-pitch noise (table M:0xFF3E00), E6/E7 and E8/E9 loops with
per-iteration volume/pitch offsets, EA tempo, EB auto-pan, EC transpose, ED LFO, EE volume,
EF volume slide, F0/F1 portamento, F2 detune, F4/F5 delayed AMS/PMS, F6/F7 alternate endings,
F9 vibrato ramp, FB decay, FC portamento speed, FD jump, FE call, FF return/end.
Pitch word = note<<8 | fraction → fnum via M:0xFF5FCA/M:0xFF60CA (note 0 = C0).
Full re-implementation: `core/src/audio/kdriver.rs` (every function names its M: address).
Known original bug: M:0xFF44B0 adds stale d6 instead of $4C during the F9 vibrato ramp; never
reached by any song or sfx (counter `f9_bug_hits` = 0 for all 122 sounds).

## 4. Decision: FM is pre-rendered

The FM music needs a sequencer + YM2612 + PCM drum mix. Decision (recorded): **pre-render** every
song and FM sfx offline with our own Rust re-implementation of the driver (`kdriver.rs`, a
function-by-function translation of the 68000 code) driving our own compact YM2612 model
(`ym2612.rs`, written from public chip documentation; integer log-sin/exp tables, ADSR with the
OPN rate tables, detune table, 8 algorithms, feedback, LFO AM/PM, ch3 mode; no SSG-EG/timers/
ladder effect). Reasons: (1) no extra CPU cost or AudioWorklet complexity in the browser,
(2) loops can be found exactly offline, (3) the same code can later run in real time if needed
(both modules are no_std-friendly plain Rust in `sncore`). Limitation: FM sfx are mixed on top of
the music instead of stealing channels 5/6 for their duration.

Loop detection (`core/src/audio/render.rs`): each track is an independent sequencer, so a repeat
of its control state (`KDriver::track_key`, sampled at its own ticks) proves its period; the song
loop is the LCM of the track periods after the latest track start. Results (`fm.json`):
60 songs: 42 exact loops, 4 approximate (4C, 50: tracks drift by one tick per loop, LCM too
long → longest period used; 67, 69: noise ambiences whose E7 loops accumulate offsets forever),
14 jingles that end; FM sfx 02, 33, 39 loop (exact), the other 59 sfx are one-shots. Verification: `loop_corr` = best waveform correlation between the audio after
loop_start and after loop_end (0.6–1.0 for tonal songs; low values are songs with noise patches
(FB=7) or free-running vibrato/auto-pan; spectrograms of 40 and 71 at both points match).
Regression test: `audio::tests::fm_song_loops_if_present`.

## 5. Engine-facing spec

Build:
```
./target/release/snt audio-pcm assets/raw/files assets/audio   # voices, banks, drum kit, pcm.json
./target/release/snt audio-fm  assets/raw assets/audio         # FM songs+sfx, fm.json (~1.5 min)
python3 -I re/py/audio_encode.py assets/audio                  # → assets/audio/web/**.ogg + json
```
Engine reads `assets/audio/web/pcm.json`, `assets/audio/web/fm.json` and plays Ogg/Opus files.
CD-DA: `assets/web/music/trackNN.ogg` (NN = disc track, 02–21).

- **op 0x04 voice(id)**: `pcm.voices[id]` → play once, mono, volume `env/255`, pan from `pan`
  (nibbles; treat 0xFF/0xEE/... as centre). If `kind==2`, duck music by ~1 dB while playing.
  If id is in `pcm.banks` (ids 1–35) nothing is heard: remember it as the *current bank*.
  Cmd 0x0A (wait PCM idle) waits for the voice end.
- **op 0x05 pcm_sfx(id)**: `pcm.sfx[id]` → segment `offset` of `drum` (src "drum") or of the
  current bank `banks[current][offset]` (src "bank"); `playbackRate = rate_hz / file_rate`
  (file rate = WAV/Ogg nominal rate given in the segment entry); volume env/255. id 0xFF = stop
  all PCM, 0xFC/0xFD mono/stereo.
- **op 0x0A fm_music(id)** (0x3F–0x7A): `fm.sounds[id]` with `music: true`; loop between
  `loop_start` and `loop_end` (48 kHz sample frames) if present, otherwise play once.
  id 0xFF = fade out over ~1.4 s (85 frames). Starting new music replaces the old one.
- **op 0x0F fm_sfx(id)** (0x01–0x3E; may also carry a music id, e.g. 0x60, which then starts
  music without resetting the volume): `fm.sounds[id]` with `music: false`, one-shot (02, 33, 39
  loop until replaced/stopped). 0x3D is silence (peak 0); F1 stop all, F2 stop music,
  F3/F4 mono/stereo.
- **op 0x0D cd_track(n)**: play `trackNN.ogg` once. Cmd 0x1B waits for CD time.
- Relative levels FM/PCM/CD on the real mixer are unknown (**HYPOTHESIS**: start with all at
  unity; FM renders peak ≤ 16073 (≈ −6 dBFS), voices near 0 dBFS, CD tracks are mastered loud).

## For other workstreams
- Sequencer audio commands: 0x07 FM music id, 0x0A wait for PCM idle (S:0x1016C, timeout 0xE10
  frames), 0x0C PCM sfx, 0x0D voice (then S:0x13FAA/0x13FC6: per-voice animation index from
  table S:0x13FEC, 1198 bytes, 0xFF = none, passed to S:0xF68E), 0x0E CD-DA, 0x1B wait CD time,
  0x1C FM sfx id.
- S:$1511E = PCM driver busy flags (bit0 stream start, bit1 voice playing, bit2 bank loaded,
  bit3 sfx playing); scripts/sequencer poll `& 7`/`& 0xF` to wait for voices.
- S:$980E / $980F hold the current FM music / sfx id (restored on load by S:0x1500E from $9C18..).
- Scene programs in DATA_D0/D1/D2/Y16 (loaded at S:0x28000) call `jmp $D476` (CD play),
  `jsr $D428` (PCM sfx), `jsr $D44C` (FM id) directly. DATA_D2 looks like the options screen
  (FM F3/F4 and PCM FC/FD mono/stereo toggles).
- Main CPU sends FM id 0xF1 at boot (M:0xFF05BE) and 0x3D/0xF2 at M:0xFF0996.

## Open questions / blockers
- No reference oracle: comparing the YM2612 model and driver translation against real hardware
  output needs an emulator + Sega CD BIOS (not downloaded, by rule) → blocker for bit-accuracy.
- RF5C164 PAN nibble order (left/right) and the analog mix levels FM vs PCM vs CD-DA.
- `_CDBIOS` function numbers 0x03/0x12 semantics from the BIOS manual (HYPOTHESIS).
