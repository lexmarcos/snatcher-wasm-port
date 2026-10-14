# Test oracle (dev-only, BIOS-free) – runs the original 68000 code

Status: WORKING up to the title screen (see "Verified" below). Never shipped in the web port.

## What it is
`re/oracle/` runs the ORIGINAL game code — main CPU IP (`header[0x200:0x800] + ip.bin` at
M:0xFF0000) and sub CPU SP (`sp.bin` at S:0x6000), which then loads SUBCODE.BIN and the data
files itself through its own file engine — on two Musashi 68000 interpreters, with high-level
emulation (HLE) of the Sega CD BIOS and of the hardware the game touches. **No BIOS image is used
or needed**: every BIOS entry point the game calls is intercepted and implemented in Python.
It exists to produce ground-truth traces and reference screenshots for differential testing of
the reimplemented engine.

## How to run
```
cd re/oracle
python3 -m venv .venv && .venv/bin/pip install numpy pillow capstone   # once
core/build.sh                                                           # builds core/build/libscdcore.so
.venv/bin/python -I run.py --frames 3600 --shot-every 300 --out out/run \
    --inputs '[[1500,0],[4,128]]'          # [[frames, padbits], ...]
```
Pad bits (same as `engine::pad`): UP=1 DOWN=2 LEFT=4 RIGHT=8 B=16 C=32 A=64 START=128.
Outputs: `out/run/fNNNNN.png` (VDP render of that frame), `out/run/trace.jsonl`.
Speed: ~700 emulated frames/s without screenshots (Musashi + Python MMIO).
Requires `work/snatcher.bin` (raw image from chdman) and `assets/raw/` from `snt extract`.

Musashi source: `re/oracle/third_party/Musashi` (cloned from github.com/kstenerud/Musashi,
MIT; gitignored). `core/scd_core.c` is our glue: two contexts, 64 KB page tables
(host pointer or Python MMIO callback), PC hooks for HLE, level IRQs with ack callback,
a 256-entry PC ring buffer per CPU for crash diagnosis.

### Why not unicorn
unicorn 2.1.4 (m68k/QEMU) was tried first and rejected: when `emu_start(..., count)` stops in
the middle of a translation block, lazily-evaluated condition codes are lost. Evidence:
S:0x6A2E `cmpi.w #4,d1` / S:0x6A32 `blt` evaluated wrongly after a count stop exactly between
the two instructions, truncating the 4-byte LBA copy in the ISO9660 directory parser
(FMWR_1.BIN LBA 0xCA9C became 0xCA00). Musashi (an interpreter) has no such issue.

## Trace format (`trace.jsonl`, one JSON object per line)
- `{"t":"frame","f":N, "sp_state", "state"(S:$9808), "vm_pc"(S:$9800), "script"(S:$980C),
  "comcmd":[8], "comstat":[8], "flags":[main,sub], "main_pc", "sub_pc", "ym":[[part,reg,val],...]}`
  – end-of-frame snapshot; `ym` = every YM2612 register write of that frame (the main CPU
  drives the YM2612 directly, see below).
- `cd_read` (BIOS ROMREAD*: lba, count, file name, sector offset), `cd_dest` (DMA device/address
  or CDCTRN destination), `music` (MSCPLAY*/stop/pause with track), `buram`, `cdbios` (other
  BIOS calls), `seq_start` (action queue S:$9940 contents when S:$9B44 goes non-zero),
  `script_file` (S:$980C changes), `unmapped`, `cpu_exception` (with the last 256 PCs).

## HLE / hardware model (each item HYPOTHESIS unless marked verified)
Boot state
- Main: SR=0x2700, SSP=0xFFFD00, PC=0xFF0584 (`ip_start`, i.e. after the security block).
- Sub: SR=0x2700, SSP=0x5E00, PC=0x5E40 boot stub: `jsr sp_init; move #$2000,sr; jsr sp_main`.
  sp_init/main/vint come from the SP header offset table at S:0x6020.
- Word RAM 2M mode, owned by main (RET=1); sub INTMASK=0x0004 (level 2 enabled).

Main BIOS (no ROM; stubs in a 128 KB RAM page at 0x000000)
- `$364` security check → returns. `$368` → "set VINT handler = a1" (verified by effect: the IP
  passes M:0xFF06C8 and that handler then drives the whole game).
- `$28C` = BIOS VINT tail, modelled as `movem.l (sp)+,d0-d7/a0-a6; rte` (the IP jumps there
  with its registers pushed, M:0xFF0706).
- `$FFFDAE` = main-side backup-RAM entry (codes as _BURAM). It addresses the RAM
  *cartridge*: if it succeeds the game shows "CD Backup RAM Cart SET. Which will you use?";
  modelled as "no cartridge" (carry set, d1=0).
- All exception vectors → a fatal stub (logged with PC ring). Level 6 vector = handler from
  `$368`. Level 4 vector = 0xFF0000|HINT vector register (A12006).

Sub BIOS (stubs in PRG-RAM 0x5E00–0x5FFF; jump-table slots contain `rts` + PC hook)
- `_WAITVSYNC $5F10`: blocks the sub until the next level-2 interrupt is taken.
- `_BURAM $5F16`: in-memory file store (BRMINIT ok/0x7D blocks, BRMSTAT, BRMSERCH, BRMREAD,
  BRMWRITE (a0→11-char name, block count at a0+12), BRMDEL, BRMVERIFY).
- `_CDBIOS $5F22` function codes (from the BIOS manual; the data path verified by the SP file
  engine S:0x6E06–0x7078 working end-to-end): ROMREADN 0x20 (a0→[lba,count]), CDCSTAT 0x8A
  (cc = sector ready), CDCREAD 0x8B (d0 = BCD MSF header, starts transfer to the CDC_MODE DD
  device), CDCTRN 0x8C (2048 bytes → a0, 4-byte header → a1, a0/a1 advanced), CDCACK 0x8D,
  CDCSTOP 0x89, ROMPAUSEON 0x08, DRVINIT 0x10, CDBPAUSE 0x84, CDBSTAT 0x81 (a0 → block,
  word 0 = 0x0100 while music plays), MSCPLAY/MSCPLAY1/MSCPLAYR 0x11/0x12/0x13 (a0 → track
  word), MSCSTOP 0x02, MSCPAUSEON 0x03, MSCPAUSEOFF 0x04.
- `_LEVEL3` jump slot $5F82 (the game patches the address at $5F84, S:0xE0AE).
- Level 2 vector → stub that saves regs and calls the SP vint entry (S:0x60BE).

Gate array
- Comm flags/cmd/status as documented; any write to comm registers ends the writer's
  timeslice so the other CPU can observe it (the IP/SP handshakes rely on fast polling).
- IFL2 (main A12000 bit 8) stays pending until the sub acknowledges level 2.
- MEMMODE 2M: main DMNA=1 → sub owns (DMNA=1, RET=0) immediately; sub RET=1 → main owns.
  1M mode is only logged (not seen so far).
- CDC DMA (DD from CDC_MODE): 5 PRG-RAM (addr = reg<<3), 7 Word RAM (reg<<3), 4 PCM (reg<<2),
  3/2 host read. DMA completes instantly (EDT set).
- Timer (level 3): period (T+1)·30.72 µs, delivered at slice granularity (~0.5 ms).
- Graphics ASIC registers: logged only (no rotation/scaling implemented – not seen yet).
PCM: register file + wave RAM + a coarse play-address model (advanced once per frame).
YM2612 at A04000: writes logged in the trace, status always "not busy". Z80 not emulated.
VDP: ports, registers, VRAM/CRAM/VSRAM, 68k DMA (Word RAM source read with a one-word lag,
HYPOTHESIS), fill, copy; whole-frame renderer (planes A/B, window, sprites, priorities;
no shadow/highlight, no mid-frame effects).

Timing
- 33 slices per frame; main 7.67 MHz and sub 12.5 MHz cycle budgets (Musashi cycle counts);
  CPUs interleaved every ~400 main cycles.
- The VBlank status flag rises ~180 main cycles before VINT is asserted. Needed: M:0xFF2D34
  polls the VBlank flag with interrupts enabled (inside the VINT handler) and would livelock
  if the interrupt arrived together with the flag.

## Verified so far (by looking at the PNGs)
- (a) SP file engine loads FMWR_1 → Word RAM 0, SUBCODE.BIN → S:0xD400, PCMLT_01 → S:0x50000,
  PCMDRMDT → S:0x55000 (trace `cd_dest`), then jumps into SUBCODE (`sp_state` 6).
- Copyright screen (frames ~60–180), Konami logo drawn by a laser line, full Konami logo,
  title screen "CYBERPUNK ADVENTURE SNATCHER … NEW GAME" with CD track 2 (MSCPLAY1).
- DATA_D2.BIN is loaded to S:0x28000 for the logo/title; voice data read from PCMLD_01.

## For other workstreams
- Main CPU writes the YM2612 directly (M:0xFF5824 loop, waits on busy bit at A04000) –
  no Z80 sound driver involvement for FM (AUDIO). The trace records every YM write per frame.
- Save system (ACTIONS): main helper code at M:0xFFE100 (copied from Word RAM 0x37800 =
  sp.bin+0x1300, 0x2D0 bytes) saves 14 blocks (896 bytes) from M:$FF7700 as backup-RAM file
  `SNATCHER_0n` via `$FFFDAE` (cart); the sub also searches `SNATCHER_01..04` in internal RAM.
