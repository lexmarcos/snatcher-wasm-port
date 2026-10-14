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
