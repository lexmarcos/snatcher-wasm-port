# Audio – RE notes

Conventions as in docs/RE_NOTES.md. S: = sub CPU address, M: = main CPU address.
Listings: `re/out/dis/audio_sub.asm`, `re/out/dis/audio_main.asm` (regenerate with
`python3 re/dis68k.py re/images/sub_prg.bin 6000 're/labels/sub_*.txt' -o re/out/dis/audio_sub.asm`
and the same for `main_ip.bin ff0000 're/labels/main_*.txt'`). Labels: `re/labels/*_30_audio.txt`.

## 0. Overview (who makes which sound)

| Kind | Hardware | Driver | Data | Script op → sequencer cmd |
|---|---|---|---|---|
| Voices (speech) | RF5C164 PCM, streamed from CD | S: PCM driver 0x15100–0x166xx | PCMLT_01 (table) + PCMLD_01 (data) | op 0x04 `voice id` → cmd 0x0D (S:0x101DC) → S:0x154F8 |
| PCM sound effects | RF5C164 | same | PCMLT_01+0x4000 (table), PCMDRMDT.BIN (drum kit), type-1 banks in PCMLD_01 | op 0x05 `id` → cmd 0x0C (S:0x101BC) → subfn_06 S:0x1530E |
| FM music (most BGM, 60 songs) | YM2612 written directly by the **main 68000** | M: Konami 68k sound driver M:0xFF4000–0xFF6141 | FMWR_1.BIN (Word RAM image) | op 0x0A `id` → cmd 0x07 (S:0x1011E) → S:0x15EB4 → COMSTAT2 → M:0xFF5058 |
| FM sound effects (62) | YM2612 | same | FMWR_1.BIN | op 0x0F `id` → cmd 0x1C (S:0x1013E) → same path |
| CD-DA (cutscenes) | CD drive | BIOS `_CDBIOS` | tracks 2–21 | op 0x0D `track` → cmd 0x0E (S:0x10238) → subfn_19 S:0xE30A |

The Z80 is **not** used as a CPU: the IP holds the Z80 bus (M:0xFF0590) and the FM driver
copies the instrument table into Z80 RAM (M:0xFF56AE, 0x1E00 bytes from Word RAM 0x200000
to 0xA00000) and writes the YM2612 at 0xA04000/0xA04002 itself (M:0xFF4126, M:0xFF575E).

## 1. RF5C164 PCM

### 1.1 Sample format (verified)
8-bit sign-magnitude, bit 7 set = positive (smoothness test: mean |Δ| sign-mag 5.8 vs
two's complement 27.8 / offset binary 27.4 on 200 KB of PCMLD_01, `re/py` snippet in history;
decoder `core::audio::sm8_to_i16`). Byte 0xFF = end marker: the driver copies bytes into PCM wave
RAM until it meets 0xFF (S:0x15C32 / S:0x15D02 `cmp.b d0,d7` with d0=#$FF) and records that
position as the stop point (`$12(a0,d3)`), then keys the channel off when the play pointer
reaches it (S:0x15974..0x159A0 → S:0x15E24).

Playback rate: RF5C164 FD register; FD 0x0800 = one sample per output tick at
12.5 MHz/384 = 32552 Hz (**HYPOTHESIS** from RF5C164/Sega CD hardware docs; consistent with
segment headers: FD 0x800 entries point at segments whose header says 32000, FD 0x400 → 16000).
`core::audio::fd_to_hz`.

### 1.2 PCMLT_01.BIN (loaded at S:0x50000)
Voice table, 8 bytes per id, id = index (entry 0 = header `ae00 0000 0000 0000`):

| byte | meaning | evidence |
|---|---|---|
| 0 | priority → S:$7F807 | S:0x15518 |
| 1 | kind: 1 = load whole entry to S:0x65000 as a segment *bank* (PCM sfx), 2 = streamed voice + duck FM/sfx volume, 0x82 = streamed voice without ducking | S:0x1552C, 0x155A4, 0x155C4; ducking S:0x155AA sets $7F813 bit0 → S:0x15282 lowers $7F817 to 0x9A |
| 2 | ENV (volume) → $7F808 | S:0x15532 / 0x155F6 |
| 3 | PAN → $7F809 (driver swaps nibbles when writing, S:0x154A2) | |
| 4–5 | start sector relative to PCMLD_01; driver adds 0x17 (= LBA 23 of PCMLD_01) | S:0x15540 `addi.l #$17,d0` |
| 6–7 | length in sectors | S:0x1554A; kind 1 must be < 0x35 sectors (S:0x1554C, "pcm.size_over_error!") |

Verified (`core::audio` test `real_tables_if_present`): entries 1..=1197 are contiguous and
cover PCMLD_01 exactly (41381 sectors). Entries 1–35 are kind 1 (banks), 36–1197 kind 2/0x82.

Streaming (kinds 2/0x82): ROMREADN (`_CDBIOS` 0x20) of 0x20 sectors to S:0x65000, then 0x10
sectors at a time alternating S:0x65000/S:0x6D000 (S:0x15898/S:0x158A0), copied into PCM wave
RAM by S:0x15CB8. Voice FD is fixed to 0x0400 (S:0x156B0) → **16276 Hz**. Four RF5C164
channels (0–3) are programmed with the same ENV/PAN/FD/start (S:0x156E6), i.e. mono.
Every raw voice ends with a 0xFF inside its last sector (`re/py/audio_ffcheck.py`: 1160/1162
end in the last sector, 2 in the one before; no non-padding data after the marker).

PCM sound-effect table at file offset 0x4000 (S:0x54000), 16 bytes per id, byte 0 of entry 0
= count (0x4A; S:0x15338 rejects ids ≥ count):
`[prio][chan 0/1][ENV][PAN][FD u16][addr u32]`, sample data at addr+0x10 (S:0x15402 skips the
segment header). addr 0x55000.. → PCMDRMDT.BIN (loaded at S:0x55000), addr 0x65000.. → the bank
(kind-1 voice entry) currently loaded at S:0x65000. `re/py/audio_sfxmap.py` shows every sfx
address hits a segment start (ids 0x01–0x0C drum kit; 0x0D–0x49 bank segments; many use bank
offset 0 so they depend on which bank is loaded).

Driver entry points (S:): `pcm_play_sfx` 0x1530E (d0=id, d1=channel or 0xFF=from table; ids ≥ 0xF8
are special commands → S:0x162E0), `pcm_play_voice` 0x154F8 (d0=id), per-frame update
0x15140, stream refill 0x1573C.

### 1.3 Segment chains (PCMDRMDT.BIN, kind-1 banks)
`[u32 n][FFFFFFFF][u16 authoring rate][6 × 00]` + n bytes (samples + 4×FF), next header
immediately after; a header with n = FFFFFFFF ends the chain. `core::audio::parse_segments`.
PCMDRMDT: 12 segments. Banks: 1–7 segments each.

## 2. CD-DA

Script op 0x0D (operand type 6 = track number) → queue cmd 0x0E → S:0x10238:
state 0: `subfn_19` S:0xE30A = `_CDBIOS` 0x12 with a0→track word (MSCPLAY1, play track once:
**HYPOTHESIS** from the Sega CD BIOS manual function list); state 1: poll `_CDBIOS` 0x81
(CDBSTAT) until status word == 0x0100 (playing), then clear the CD-time counter S:$787A.
No MSCPLAYR (repeat) call exists in SUBCODE (only function 0x12 is used for music): CD tracks
play once. S:0xD5FE copies CDBSTAT absolute/relative time into S:$7876/$787A every frame;
cmd 0x1B (S:0x103D2) waits until the relative time reaches an operand (lip-sync of cutscenes).
Loading a file first issues `_CDBIOS` 3 (S:0xE31A, MSCPAUSEON? **HYPOTHESIS**), so data
access interrupts music.

Track usage by scripts (`re/py/audio_scriptuse.py`):

| track | script files |
|---|---|
| 6 | SP08 |
| 7 | SP16 |
| 8, 9 | SP20 |
| 10, 11 | SP29 |
| 12, 13, 14 | SP33 |
| 15 | SP36 |
| 16, 17, 18 | SP38 |
| 20 | SP06 |
| 21 | SP31 |

Tracks 2–5 and 19 are not started by script op 0x0D (TODO: find other callers, e.g. title/intro
code, op 0x41 sub-commands).

## 3. FM (YM2612) – main-CPU Konami driver

Sound ids arrive from the sub CPU: S:0x15EB4 queues ids in a 128-byte ring (S:$7F880); each
frame S:0x15F18 writes one id to COMSTAT2 (S:0xFF8024), volume (through log table S:0x15FBA)
to COMSTAT1 hi/lo, and a change counter to S:0xFF8025. id 0xFF = fade out (S:0x15EBE).
Main side: `fm_request` M:0xFF5058 (d0=id) queues into M:$FF7F00 (32 entries),
`fm_update` M:0xFF5106 dequeues; ids ≥ 0xF1 are commands (table M:0xFF51A8: F1 = stop all
M:0xFF52AC, F2 = M:0xFF51E4, F3/F4 set/clear $FF7F2A bit 3), others → `fm_start` M:0xFF5306.

Sound table M:0xFF6142, 8 bytes per id: `[type][?][slot][prio][u32 index]`.
ids 0x01–0x3E: sfx (type low nibble 0), index into sfx pointer table at M:0xFF9000;
ids 0x3F–0x7A: music (type 1), index = song number 0..0x3B.

FMWR_1.BIN = Word RAM image at M:0x200000 (verified by layout):
- 0x0000–0x1DFF: 256 FM instruments × 26 bytes (pointer table of 256 longs then data),
  copied to Z80 RAM M:0xA00000 at boot (M:0xFF56AE).
- 0x1E00–0x2845: sfx bank, copied to M:0xFF9000 (0xA46 bytes).
- 0x2846: song offset table (60 longs, relative to 0x2846); songs copied on demand to
  M:0xFF8000, 0x54 longs per frame (M:0xFF55FE), size table M:0xFF5586 (= song size + 16).

(Driver internals: section 4, in progress.)

## 5. Engine-facing spec (current)

Generated by `snt audio-pcm assets/raw/files assets/audio` then
`python3 -I re/py/audio_encode.py assets/audio 32k` → `assets/audio/web/{voice,bank,drum}/*.ogg`
and `assets/audio/web/pcm.json`:
- `voices[id]` → `{file, samples, seconds, kind, env, pan, prio}`; play mono at 16276 Hz
  (file rate), volume env/255. Script op 0x04 id = key. If `kind` is 1 the id is not a voice
  but loads `banks[id]`.
- `banks[id][offset]` → segment file (rate = authoring rate in header).
- `drum[offset]` → PCMDRMDT segment file.
- `sfx[id]` → `{src: "drum"|"bank", offset, fd, rate_hz, env, pan, prio, chan}`.
  Play the segment at `offset` of the drum kit or of the **currently loaded bank** with
  `playbackRate = rate_hz / file_rate`.
- Music: `assets/web/music/trackNN.ogg` (CD-DA, play once).

## For other workstreams
- Sequencer commands that are audio: 0x07 FM music id, 0x0A wait for PCM idle (S:0x1016C,
  timeout 0xE10 frames), 0x0C PCM sfx, 0x0D voice (also calls S:0x13FAA/0x13FC6 – lip sync?),
  0x0E CD-DA track, 0x1B wait CD time, 0x1C FM sfx id.
- S:$1511E bits = PCM driver busy flags (bit0 stream start, bit1 voice playing, bit2 bank loaded,
  bit3 sfx playing).
