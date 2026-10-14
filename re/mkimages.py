#!/usr/bin/env python3
"""Build analysis memory images from assets/raw (see docs/RE_NOTES.md §2)."""
import os
R = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'assets', 'raw')
O = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'images')
os.makedirs(O, exist_ok=True)
rd = lambda p: open(os.path.join(R, p), 'rb').read()
# Sub CPU PRG-RAM from 0x6000: SP at 0x6000, SUBCODE.BIN at 0xD400.
sp, sc = rd('sp.bin'), rd('files/SUBCODE.BIN')
img = bytearray(0xD400 - 0x6000 + len(sc)); img[:len(sp)] = sp; img[0xD400 - 0x6000:] = sc
open(os.path.join(O, 'sub_prg.bin'), 'wb').write(img)
# Main CPU: security block (disc 0x200..0x7FF) + IP, at 0xFF0000.
open(os.path.join(O, 'main_ip.bin'), 'wb').write(rd('header.bin')[0x200:0x800] + rd('ip.bin'))
