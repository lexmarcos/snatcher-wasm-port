"""FIDELITY2 helpers shared by fid_play.py / fid_cmp.py: start the ORIGINAL at an arbitrary script position
(the same thing the SP01 developer menu does) with a story state that equals the engine's fresh VM."""
import os
HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, '..', '..', 'assets', 'raw', 'files')

def oracle_jump(m, spec, wait=300, zero_save=True):
    """m: Machine loaded from junker_hq. spec: 'SPnn:pc[:hex,..]' (pc hex; the menu path items are ignored - the oracle starts
    at the CHOICE leaf pc, give the leaf pc of SP01). Runs `wait` frames (Junker HQ command menu up), then writes the script to
    S:0x1A800, clears the VM save block $9C00..$9DBF (the engine starts from Vm::blank), sets $980C/$9800 and exits the menu loop."""
    m.inputs = []
    for _ in range(wait): m.step_frame(capture=False)
    s = m.sub
    parts = spec.split(':')
    n = int(parts[0][2:]); pc = int(parts[1], 16)
    data = open(os.path.join(RAW, 'SP%02d.BIN' % n), 'rb').read()
    s.write(0x1A800, data[:0xD000])
    if zero_save: s.write(0x9C00, bytes(0x1C0))
    s.w16(0x980C, 57 + n)
    s.w16(0x9800, pc); s.w16(0x9802, 0xFFFF)
    for base in (0x9840, 0x9852, 0x9892, 0x98B2, 0x98D8, 0x98E0): s.w8(base + 1, 0)   # all VM stacks empty (Vm::blank)
    s.w8(0x79F4, 0xFF)


def patch_cdtime():
    """The stock oracle answers CDBSTAT with $787A = 0xFFFFFFFF, so commands that wait for the CD time (seq cmd 1B, S:0x103D2,
    cutscene/lip-sync sync) never complete. Same in-process patch as re/py/modules_scn.py (no oracle file is modified):
    CDBSTAT+0x0C = relative track time BCD `MM SS FF 00`, FF in 1/75 s = frames since MSCPLAY * 75/60 (HYPOTHESIS for the 75/60
    conversion, docs/re/modules.md). Idempotent."""
    import struct
    from machine import Machine, bcd
    if getattr(Machine._cdbios, '_fid_patched', False): return
    orig = Machine._cdbios
    def cdbios(self):
        orig(self)
        s = self.sub
        if (s.d(0) & 0xFFFF) == 0x81 and self.music:
            n = max(0, (self.frame - self.music['start_frame']) * 75 // 60)
            rel = (bcd(n // 4500) << 24) | (bcd((n // 75) % 60) << 16) | (bcd(n % 75) << 8)
            s.write(s.a(0) + 0x0C, struct.pack('>I', rel))
    cdbios._fid_patched = True
    Machine._cdbios = cdbios
