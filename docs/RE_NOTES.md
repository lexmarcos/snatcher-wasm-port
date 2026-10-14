# Snatcher (USA, Sega CD) – reverse-engineering notes

Conventions: every claim cites evidence (address in a listing under `re/out/dis/*.asm`,
a tool/test, or a hexdump offset). Unverified ideas are marked **HYPOTHESIS**.
Addresses are 24-bit. Main CPU = "M:", sub CPU = "S:".

## 1. Disc layout

Source: `snt extract` (tools/src/main.rs), `assets/raw/extract_report.txt`.

- Track 1 MODE1/2352, 56014 sectors (to track 2 INDEX 00). ISO9660 PVD at sector 16,
  volume "SNATCHER", 55364 sectors, flat root directory (100 entries).
- Tracks 2–21: CD-DA (LE 16-bit stereo in the chdman .bin; verified by sample
  smoothness: mean |Δ| LE=417 vs BE=10881).
- Header (sector 0): `SEGADISCSYSTEM`, volume `SEGAIPMENU`, system `KONAMI R002`,
  IP offset 0x800 size 0x6000, SP offset 0x6800 size 0x1800, date `10101994`,
  serial `GM T-95035 -00`, region `J`(!) at 0x1F0 (US disc still says J in header area).
- Security code (US) occupies sector 0 bytes 0x200–0x583; IP code continues at 0x584.
  Main RAM image = header[0x200:0x800] ++ ip.bin, loaded at M:0xFF0000 (verified:
  0xFF0008 `bra.w $FF0584` and code continues seamlessly across 0xFF0600 = disc 0x800).

### File table (ISO9660) – also the game's own file index
The SP (S:0x62FA) contains a 16-byte-stride file-name table; the index of a name in that
table is the file number used by the game. It equals the ISO order minus the .TXT files.

| idx | file | LBA | size |
|---|---|---|---|
| 0–52 | DATA_A0 … DATA_Y16 | 53265… | see filelist |
| 53 | FMWR_1.BIN | 51868 | 53232 |
| 54 | PCMDRMDT.BIN | 51772 | 52814 |
| 55 | PCMLD_01.BIN | 23 | 84,748,288 (voice stream, 41381 sectors) |
| 56 | PCMLT_01.BIN | 51762 | 17568 (voice table?) |
| 57–95 | SP00 … SP38 | 52212… | scenario scripts (English text inside) |
| 96 | SUBCODE.BIN | 51735 | 53110 (sub-CPU main program) |

Full list with LBAs: `assets/raw/filelist.tsv` (generated).

## 2. Boot / programs

- S: SP module header "MAIN A014" at S:0x6000; offset table at 0x6020 →
  init 0x602E, main 0x60A8, vint 0x60BE, user 0x629C (from header words 000e 0088 009e 027c).
- SP is a small file engine + state machine (state word at S:0x7840, table S:0x6162).
  State 6 handler S:0x6296 = `jmp $D404`.
- SUBCODE.BIN loads at **S:0xD400** (evidence: SP jumps to $D404/$D47C/$D482, and those
  offsets in SUBCODE are `jmp.l` slots of its jump table: file+4 `jmp $D4C4`, +0x7C `jmp $166BA`,
  +0x82 `jmp $D4DC`). Jump table = 23 `jmp.l` entries at S:0xD404..
- M: IP: `ip_start` 0xFF0584 initialises, sets VINT via `lea $FF06C8,a1; jsr $368.w`
  (BIOS set-vector, **HYPOTHESIS** for $368), then idles in `bra *` (M:0xFF0696).
  All main-CPU work happens in `vint_handler` M:0xFF06C8 → the main CPU is a
  display/input slave; **HYPOTHESIS: game logic (script VM) runs on the sub CPU**.
- M:0xFF07D4: copies 0xB4 longs from Word RAM 0x237800 to RAM 0xFFE100 (main code
  provided by the sub CPU through Word RAM), then calls `jsr $E100.w`/`jmp $E106.w`.

## 3. Script VM (summary; details in docs/re/script.md)

- Sub CPU main loop `subfn_20` S:0x166BA, state word S:$9808 (table S:0x166D4).
- Scenario scripts = SPxx.BIN (file idx 57+xx), loaded at S:0x1A800 (`script_buf`);
  current script file id in S:$980C; command 0x11 in the action queue loads a file
  (S:0x17130: `move.w #$11,(a5)+; move.l #$1A800,(a5)+; move.w id,(a5)+`).
- Bytecode region 0x0000–0x37FF (padded with 0xFF), text/data from file offset 0x3800
  (verified on all 39 SP files by `re/py/spcov.py`: tree parse from 0 runs until the 0xFF pad).
- Dispatch S:0x16974: opcode byte at (a2=script_buf + a3) → table S:0x16CDC, 4 bytes/opcode
  `[type1][type2<<4|type3][handler-0x15400]`, 0x4B opcodes (0x00–0x4A).
- Operand types: 0/7 none (7 also marks list end), 1 variable descriptor (2 bytes),
  2/3/5/6 16-bit immediates (meaning per opcode), 4 nested node, 8 = "link": a 2-byte
  pointer follows the opcode (S:0x16A64 tests (t2|t3)&8; S:0x16A20 follows it).
- Variables are bit-fields in a bit array at S:$9C60: descriptor bits 15..7 = byte offset,
  6..4 = bit shift, 3..0 = width-1 (read S:0x16B14, write S:0x16B80).
- Action queue at S:$9940 (a5), executed by the sequencer S:0xFFC4 (36 commands,
  table S:0xFFE2), progress index S:$9B40, busy flag S:$9B44.
