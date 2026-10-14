# re/ – reverse-engineering workspace

- `mkimages.py` builds memory images (`images/`, gitignored) from `assets/raw`.
- `disall.sh` regenerates capstone listings `out/dis/{main_ip,sub_prg}.asm` (gitignored)
  using all label files `labels/{main,sub}_*.txt` (format: `ADDR NAME [f]`, `f` = code entry).
- `dis68k.py` – recursive-descent 68000 disassembler (24-bit addresses, auto jump tables).
- `analyze.sh NAME IMAGE BASE LABELS MEMMAP` – Ghidra 12.1.4 headless import (68000:BE:32:default,
  BinaryLoader) + labels + auto-analysis + dump of decompilation to `out/NAME/`.
  Example: `./analyze.sh sub_prg images/sub_prg.bin 0x6000 <(cat labels/sub_*.txt) sub_memmap.txt`.
  REA's `open_binary` cannot load raw 68000 images (raw-binary supports only x86/arm/6502),
  so the Ghidra route is headless-only; label files are the persistent "database".
- `py/` – Python experiments (script tree parser etc.). Run with `python3 -I`.
