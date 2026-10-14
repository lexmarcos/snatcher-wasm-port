#!/usr/bin/env bash
# Regenerate capstone listings for all known code images (re/out/dis/*.asm).
# Images are built by re/mkimages.py from assets/raw (not committed).
set -e
cd "$(dirname "$0")"
python3 mkimages.py
mkdir -p out/dis
python3 dis68k.py images/main_ip.bin ff0000 'labels/main_*.txt' -o out/dis/main_ip.asm
python3 dis68k.py images/sub_prg.bin 6000 'labels/sub_*.txt' -o out/dis/sub_prg.asm
