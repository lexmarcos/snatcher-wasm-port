"""Sega CD test oracle: runs the ORIGINAL Snatcher 68000 code (main IP + sub SP/SUBCODE)
on two Musashi 68000 cores (re/oracle/core, dev-only) with high-level emulation (HLE) of the
BIOS and hardware. No BIOS image is used. See docs/re/oracle.md (assumptions marked HYPOTHESIS).

Why Musashi and not unicorn: unicorn 2.1.4's m68k target loses lazily-evaluated condition codes
when emu_start stops on an instruction count in the middle of a translation block (observed at
S:0x6A32 `blt` after `cmpi.w #4,d1`, corrupting the ISO9660 directory parse). Musashi is an
interpreter: stopping between any two instructions is exact.
"""
import ctypes, json, os, struct
from vdp import VDP

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
RAW = os.path.join(ROOT, 'assets', 'raw')
BIN = os.path.join(ROOT, 'work', 'snatcher.bin')

# Musashi register ids (m68k.h)
R_D0, R_A0, R_A7, R_PC, R_SR, R_SP, R_USP, R_ISP = 0, 8, 15, 16, 17, 18, 19, 20
R_PPC = 29

MAIN, SUB = 0, 1
MAIN_HZ = 7670453
SUB_HZ = 12500000

MMIO_R = ctypes.CFUNCTYPE(ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32, ctypes.c_int)
MMIO_W = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32)
HOOK = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_uint32)
ACK = ctypes.CFUNCTYPE(None, ctypes.c_int, ctypes.c_int)


def buf(size):
    return (ctypes.c_ubyte * size)()


def bcd(v):
    return ((v // 10) << 4) | (v % 10)


class Disc:
    """Track 1 user data (2048-byte sectors) read from the raw MODE1/2352 image."""
    def __init__(self):
        self.f = open(BIN, 'rb')
        self.files = []
        with open(os.path.join(RAW, 'filelist.tsv')) as fl:
            for line in fl.read().splitlines()[1:]:
                n, lba, size, _ = line.split('\t')
                self.files.append((int(lba), int(size), n))

    def sector(self, lba):
        self.f.seek(lba * 2352 + 16)
        return self.f.read(2048)

    def name_of(self, lba):
        for (l, s, n) in self.files:
            if l <= lba < l + max(1, (s + 2047) // 2048):
                return n, lba - l
        return None, 0


def track_frames():
    """CD-DA track lengths in frames (1/60 s) from extract_report.txt (snt extract)."""
    out = {}
    try:
        for line in open(os.path.join(RAW, 'extract_report.txt')):
            if line.startswith('track '):
                n = int(line.split()[1].rstrip(':'))
                secs = float(line.split('(')[1].split()[0])
                out[n] = int(secs * 60)
    except OSError:
        pass
    return out


class Cpu:
    """Thin wrapper around one Musashi context in libscdcore."""
    def __init__(self, lib, idx, name):
        self.lib, self.i, self.name = lib, idx, name
        self.halted = False
        self.err = None
        self.waiting = None
        self.cycles = 0

    def reg(self, r, v=None):
        if v is None:
            return self.lib.core_get_reg(self.i, r) & 0xFFFFFFFF
        self.lib.core_set_reg(self.i, r, v & 0xFFFFFFFF)

    def d(self, n, v=None): return self.reg(R_D0 + n, v)
    def a(self, n, v=None): return self.reg(R_A0 + n, v)
    def pc(self, v=None): return self.reg(R_PC, v)
    def sr(self, v=None): return self.reg(R_SR, v)

    def set_carry(self, c):
        s = self.sr()
        self.sr((s | 1) if c else (s & ~1))

    def r8(self, a): return self.lib.core_read(self.i, a, 1)
    def r16(self, a): return self.lib.core_read(self.i, a, 2)
    def r32(self, a): return self.lib.core_read(self.i, a, 4)
    def w8(self, a, v): self.lib.core_write(self.i, a, 1, v & 0xFF)
    def w16(self, a, v): self.lib.core_write(self.i, a, 2, v & 0xFFFF)
    def w32(self, a, v): self.lib.core_write(self.i, a, 4, v & 0xFFFFFFFF)

    def read(self, a, n):
        return bytes(self.r8(a + i) for i in range(n))

    def write(self, a, data):
        for i, b in enumerate(data):
            self.w8(a + i, b)

    def pop32(self):
        sp = self.a(7)
        v = self.r32(sp)
        self.a(7, sp + 4)
        return v

    def run(self, cycles):
        if self.halted or self.waiting or cycles <= 0:
            return 0
        n = self.lib.core_run(self.i, cycles)
        self.cycles += n
        return n

    def pc_ring(self):
        out = (ctypes.c_uint32 * 256)()
        self.lib.core_pc_ring(self.i, out)
        return list(out)

    def disasm(self, pc):
        b = ctypes.create_string_buffer(128)
        n = self.lib.core_disasm(self.i, pc, b)
        return n, b.value.decode()


class Machine:
    LINES = 262

    def __init__(self, trace_path=None, verbose=False):
        self.verbose = verbose
        self.trace = open(trace_path, 'w') if trace_path else None
        self.frame = 0
        self.events = []
        self.disc = Disc()
        self.inputs = []
        self.pad = 0
        # memories
        self.mrom = buf(0x20000)          # main BIOS ROM area (HLE stubs + vectors)
        self.prg = buf(0x80000)
        self.wram = buf(0x40000)
        self.mram = buf(0x10000)
        self.z80 = bytearray(0x2000)
        self.bram = buf(0x10000)
        # gate array state
        self.comflag_main = 0
        self.comflag_sub = 0
        self.comcmd = [0] * 8
        self.comstat = [0] * 8
        self.memmode = 0x01               # RET=1: main owns 2M Word RAM at boot (HYPOTHESIS)
        self.wp = 0
        self.intmask = 0x0004             # BIOS leaves level 2 enabled (HYPOTHESIS)
        self.timer = 0
        self.timer_acc = 0.0
        self.ifl2 = False
        self.cdc_mode = 0
        self.cdc_dma = 0
        self.cdc_host = []
        self.stopwatch = 0
        self.asic = {}
        self.main_vint = None
        self.main_hint_vec = 0
        self.pad_th = 0x40
        self.pad_ctrl = 0
        # port 2 / light gun (Justifier, device ID 1; M:0xFF1358 ID read, M:0xFF13F6 button read, M:0xFF148A external-interrupt handler)
        self.gun = None                 # None = no device, else {'x': px, 'y': line, 'trig': 0/1, 'aux': 0/1} (screen coordinates)
        self.pad2_th = 0x40
        self.pad2_ctrl = 0
        self.mcl = 0.0                  # main-CPU cycles consumed in the current video line (H counter)
        self.gun_fired = False
        self.gun_hot = 0
        self.ym_addr = (0, 0)
        self.ym_frame = []
        self.sub_pending = set()
        self.main_pending = set()
        # CDC / drive HLE
        self.cd_lba = 0
        self.cd_left = 0
        self.cd_ready = None
        self.cd_cur = None
        self.cd_first_dest = None
        self.music = None
        self.music_status = 0x0000
        self.track_len = track_frames()
        self.pcm = PCM(self.log)
        self.vdp = VDP(self.main_read16)
        self.vdp.hfn = self._hcount
        self.last_seq_busy = 0
        self.last_script = 0
        # core
        self.lib = ctypes.CDLL(os.path.join(HERE, 'core', 'build', 'libscdcore.so'))
        L = self.lib
        L.core_get_reg.restype = ctypes.c_uint32
        L.core_read.restype = ctypes.c_uint32
        L.core_read.argtypes = [ctypes.c_int, ctypes.c_uint32, ctypes.c_int]
        L.core_write.argtypes = [ctypes.c_int, ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
        L.core_set_reg.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_uint32]
        L.core_map.argtypes = [ctypes.c_int, ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p]
        L.core_add_hook.argtypes = [ctypes.c_int, ctypes.c_uint32]
        L.core_disasm.argtypes = [ctypes.c_int, ctypes.c_uint32, ctypes.c_char_p]
        self._cb = (MMIO_R(self._mmio_r), MMIO_W(self._mmio_w), HOOK(self._hook), ACK(self._ack))
        L.core_init(*self._cb)
        self.main = Cpu(L, MAIN, 'main')
        self.sub = Cpu(L, SUB, 'sub')
        self._map()
        self._load_boot()

    # ------------------------------------------------------------ logging
    def log(self, ev):
        ev = dict(ev)
        ev['f'] = self.frame
        self.events.append(ev)
        if self.verbose:
            print(json.dumps(ev))

    # ------------------------------------------------------------ memory maps
    def _map(self):
        L = self.lib
        p = lambda b: ctypes.addressof(b)
        # main CPU (24-bit): ROM stub, PRG-RAM window (bank 0), Word RAM 2M, RAM; IO via MMIO
        L.core_map(MAIN, 0x000000, 0x20000, p(self.mrom))
        L.core_map(MAIN, 0x020000, 0x20000, p(self.prg))
        L.core_map(MAIN, 0x200000, 0x40000, p(self.wram))
        L.core_map(MAIN, 0xFF0000, 0x10000, p(self.mram))
        # sub CPU: PRG-RAM, Word RAM 2M at 0x80000; 1M window 0xC0000 aliased onto the 2M
        # buffer (only meaningful if the game selects 1M mode, which is logged); backup RAM.
        L.core_map(SUB, 0x000000, 0x80000, p(self.prg))
        L.core_map(SUB, 0x080000, 0x40000, p(self.wram))
        L.core_map(SUB, 0x0C0000, 0x20000, p(self.wram))
        L.core_map(SUB, 0xFE0000, 0x10000, p(self.bram))

    def _mmio_r(self, cpu, addr, size):
        if not hasattr(self, 'sub'):
            return 0                                  # reset vector fetch during core_init
        try:
            return (self._main_r if cpu == MAIN else self._sub_r)(addr, size)
        except Exception as e:
            self.log({'t': 'py_error', 'where': 'mmio_r', 'cpu': cpu, 'addr': addr, 'err': repr(e)})
            return 0

    def _mmio_w(self, cpu, addr, size, val):
        try:
            (self._main_w if cpu == MAIN else self._sub_w)(addr, size, val)
        except Exception as e:
            self.log({'t': 'py_error', 'where': 'mmio_w', 'cpu': cpu, 'addr': addr, 'err': repr(e)})

    def _unmapped(self, cpu, addr, size, val=None):
        if not hasattr(self, '_unm_seen'):
            self._unm_seen = set()
        k = (cpu, addr & 0xFFFF00)
        if k not in self._unm_seen and len(self._unm_seen) < 200:
            self._unm_seen.add(k)
            c = self.main if cpu == MAIN else self.sub
            self.log({'t': 'unmapped', 'cpu': c.name, 'addr': addr, 'size': size, 'w': val, 'ppc': c.reg(R_PPC)})
        return 0

    # ------------------------------------------------------------ main bus
    def _main_r(self, a, size):
        if size == 2:
            if 0xC00000 <= a < 0xC00020:
                return self._vdp_r(a)
            return (self._main_r(a, 1) << 8) | self._main_r(a + 1, 1)
        if 0xC00000 <= a < 0xC00020:
            v = self._vdp_r(a & ~1)
            return (v >> 8) if not (a & 1) else (v & 0xFF)
        if 0xA00000 <= a < 0xA10000:
            if 0xA04000 <= a < 0xA04004:
                return 0                              # YM2612: never busy
            return self.z80[a & 0x1FFF]
        if a == 0xA10001:
            return 0x80                               # overseas NTSC, expansion unit present
        if a == 0xA10003:
            return self._pad_read()
        if a == 0xA10005:
            return self._pad2_read()
        if a == 0xA10007:
            return 0x7F
        if a == 0xA10009:
            return self.pad_ctrl
        if 0xA10000 <= a < 0xA10020:
            return 0
        if 0xA11000 <= a < 0xA11300:
            return 0                                  # Z80 bus always granted
        if 0xA12000 <= a < 0xA12040:
            return self._ga_main_r8(a - 0xA12000)
        if 0xA130F0 <= a < 0xA13100:
            return 0
        return self._unmapped(MAIN, a, size)

    def _main_w(self, a, size, v):
        if size == 2:
            if 0xC00000 <= a < 0xC00020:
                self._vdp_w(a, v)
                return
            if 0xA12000 <= a < 0xA12040:
                self._ga_main_w(a - 0xA12000, v, 2)
                return
            self._main_w(a, 1, v >> 8)
            self._main_w(a + 1, 1, v & 0xFF)
            return
        if 0xC00000 <= a < 0xC00020:
            if (a & 0x1F) >= 0x10:
                return                                # PSG
            self._vdp_w(a & ~1, (v << 8) | v)
            return
        if 0xA00000 <= a < 0xA10000:
            if 0xA04000 <= a < 0xA04004:
                o = a & 3
                if o in (0, 2):
                    self.ym_addr = (o >> 1, v)
                else:
                    self.ym_frame.append((self.ym_addr[0], self.ym_addr[1], v))
                return
            self.z80[a & 0x1FFF] = v
            return
        if a == 0xA10003:
            self.pad_th = v & 0x40 if self.pad_ctrl & 0x40 else 0x40
            return
        if a == 0xA10009:
            self.pad_ctrl = v
            return
        if a == 0xA10005:
            if self.pad2_ctrl & 0x40:
                self.pad2_th = v & 0x40
            return
        if a == 0xA1000B:
            self.pad2_ctrl = v
            return
        if 0xA10000 <= a < 0xA10020 or 0xA11000 <= a < 0xA11300:
            return
        if 0xA12000 <= a < 0xA12040:
            self._ga_main_w(a - 0xA12000, v, 1)
            return
        if 0xA130F0 <= a < 0xA13100:
            return
        self._unmapped(MAIN, a, size, v)

    def main_read16(self, a):
        a &= 0xFFFFFE
        if 0x200000 <= a < 0x240000:
            o = a - 0x200000
            return (self.wram[o] << 8) | self.wram[o + 1]
        if a >= 0xFF0000:
            o = a - 0xFF0000
            return (self.mram[o] << 8) | self.mram[o + 1]
        if a < 0x20000:
            return (self.mrom[a] << 8) | self.mrom[a + 1]
        return 0

    def _install_gun_vector(self):
        # level-2 (external interrupt / TH line) vector -> jump through the RAM vector the program installs at $FFFD14 (M:0xFF139E);
        # also re-installed after a state load (saved ROM stubs predate it)
        ctypes.memmove(ctypes.addressof(self.mrom) + 0x430, bytes.fromhex('2f38fd144e75'), 6)
        struct.pack_into('>I', self.mrom, 0x68, 0x430)

    def _pad2_read(self):
        g = self.gun
        if g is None:
            return 0x7F
        if not self.pad2_ctrl & 0x80:
            # device-ID phase (TH is an output): ID 1 reads 0 on D3..D0 with TH = 1, and D3:D2 = 0, D1:D0 = 3 with TH = 0 (M:0xFF1386 table)
            return 0x40 if self.pad2_th else 0x03
        b = (1 if g.get('trig') else 0) | (2 if g.get('aux') else 0)
        return 0x40 | (0x07 ^ b)                      # D0 trigger, D1 second button, active low

    def _hcount(self):
        if self.gun is None:
            return 0x80
        # H32 counter: 342 pixel clocks per line, value = clock >> 1, jumps from 0x93 to 0xE9 in the blanking
        px = self.mcl * 342.0 / (MAIN_HZ / 60 / self.LINES)
        h = int(px) >> 1
        if h > 0x93:
            h += 0xE9 - 0x94
        return h & 0xFF

    def _pad_read(self):
        p = self.pad   # engine::pad bits: UP DOWN LEFT RIGHT B C A START
        u, d, l, r = p & 1, p >> 1 & 1, p >> 2 & 1, p >> 3 & 1
        b, c, aa, st = p >> 4 & 1, p >> 5 & 1, p >> 6 & 1, p >> 7 & 1
        if self.pad_th & 0x40:
            v = 0x40 | ((not c) << 5) | ((not b) << 4) | ((not r) << 3) | ((not l) << 2) | ((not d) << 1) | (not u)
        else:
            v = ((not st) << 5) | ((not aa) << 4) | ((not d) << 1) | (not u)
        return v & 0x7F

    # ------------------------------------------------------------ gate array (main side)
    def _ga_main_r8(self, o):
        if o == 0x00:
            return (0x80 if self.intmask & 4 else 0) | (1 if self.ifl2 else 0)
        if o == 0x01:
            return 0x01                               # SRES=1 (sub running), SBRQ=0
        if o == 0x02:
            return self.wp
        if o == 0x03:
            return self.memmode & 0x07
        if o == 0x04:
            return (self.cdc_mode >> 8) & 0xC7
        if o in (0x06, 0x07):
            return (self.main_hint_vec >> (8 if o == 6 else 0)) & 0xFF
        if o in (0x08, 0x09):
            return self._host_data_byte(o)
        if o == 0x0E:
            return self.comflag_main
        if o == 0x0F:
            return self.comflag_sub
        if 0x10 <= o < 0x20:
            w = self.comcmd[(o - 0x10) >> 1]
            return (w >> 8) if not (o & 1) else (w & 0xFF)
        if 0x20 <= o < 0x30:
            w = self.comstat[(o - 0x20) >> 1]
            return (w >> 8) if not (o & 1) else (w & 0xFF)
        return 0

    def _ga_main_w(self, o, v, size):
        if size == 2:
            if 0x10 <= o < 0x20:
                self.comcmd[(o - 0x10) >> 1] = v & 0xFFFF
                self.lib.core_end_timeslice()
                return
            self._ga_main_w(o, v >> 8, 1)
            self._ga_main_w(o + 1, v & 0xFF, 1)
            return
        if o == 0x00:
            if v & 0x01 and not self.ifl2:
                self.ifl2 = True                      # IFL2: request sub level-2 interrupt
                self.lib.core_end_timeslice()
        elif o == 0x02:
            self.wp = v
        elif o == 0x03:
            if v & 0x02:                              # DMNA: give 2M Word RAM to sub (HYPOTHESIS: immediate)
                if self.memmode & 0x04:
                    self.log({'t': 'warn', 'msg': '1M mode DMNA'})
                self.memmode = (self.memmode & ~0x03) | 0x02
        elif o in (0x06, 0x07):
            sh = 8 if o == 6 else 0
            self.main_hint_vec = (self.main_hint_vec & ~(0xFF << sh)) | (v << sh)
            # level-4 autovector fetch returns 0x00FF:HINTVEC (Sega CD hardware manual)
            struct.pack_into('>I', self.mrom, 0x70, 0xFF0000 | self.main_hint_vec)
        elif o == 0x0E:
            if self.comflag_main != v:
                self.comflag_main = v
                self.lib.core_end_timeslice()
        elif 0x10 <= o < 0x20:
            i = (o - 0x10) >> 1
            if o & 1:
                self.comcmd[i] = (self.comcmd[i] & 0xFF00) | v
            else:
                self.comcmd[i] = (self.comcmd[i] & 0x00FF) | (v << 8)
            self.lib.core_end_timeslice()

    # ------------------------------------------------------------ sub bus
    def _sub_r(self, a, size):
        if 0xFF8000 <= a:
            o = a & 0x1FF
            if size == 2:
                return (self._ga_sub_r8(o) << 8) | self._ga_sub_r8(o + 1)
            return self._ga_sub_r8(o)
        if 0xFF0000 <= a < 0xFF8000:
            if size == 2:
                return (self.pcm.read(a & 0x7FFF) << 8) | self.pcm.read((a + 1) & 0x7FFF)
            return self.pcm.read(a & 0x7FFF)
        return self._unmapped(SUB, a, size)

    def _sub_w(self, a, size, v):
        if 0xFF8000 <= a:
            o = a & 0x1FF
            if size == 2:
                if 0x20 <= o < 0x30:
                    self.comstat[(o - 0x20) >> 1] = v & 0xFFFF
                    self.lib.core_end_timeslice()
                    return
                if o == 0x0A:
                    self.cdc_dma = v & 0xFFFF
                    return
                if o == 0x04:
                    self.cdc_mode = (self.cdc_mode & 0xC000) | (v & 0x0700)
                    return
                self._ga_sub_w8(o, v >> 8)
                self._ga_sub_w8(o + 1, v & 0xFF)
                return
            self._ga_sub_w8(o, v)
            return
        if 0xFF0000 <= a < 0xFF8000:
            if size == 2:
                self.pcm.write(a & 0x7FFF, v >> 8)
                self.pcm.write((a + 1) & 0x7FFF, v & 0xFF)
            else:
                self.pcm.write(a & 0x7FFF, v)
            return
        self._unmapped(SUB, a, size, v)

    def _ga_sub_r8(self, o):
        if o == 0x01:
            return 0x01
        if o == 0x02:
            return self.wp
        if o == 0x03:
            return self.memmode
        if o == 0x04:
            return self.cdc_mode >> 8
        if o == 0x05:
            return self.cdc_mode & 0xFF
        if o in (0x08, 0x09):
            return self._host_data_byte(o)
        if o in (0x0A, 0x0B):
            return (self.cdc_dma >> (8 if o == 0x0A else 0)) & 0xFF
        if o in (0x0C, 0x0D):
            return (self.stopwatch >> (8 if o == 0x0C else 0)) & 0xFF
        if o == 0x0E:
            return self.comflag_main
        if o == 0x0F:
            return self.comflag_sub
        if 0x10 <= o < 0x20:
            w = self.comcmd[(o - 0x10) >> 1]
            return (w >> 8) if not (o & 1) else (w & 0xFF)
        if 0x20 <= o < 0x30:
            w = self.comstat[(o - 0x20) >> 1]
            return (w >> 8) if not (o & 1) else (w & 0xFF)
        if o == 0x31:
            return self.timer
        if o == 0x33:
            return self.intmask
        if 0x58 <= o < 0x68:
            return self.asic.get(o, 0)
        return 0

    def _ga_sub_w8(self, o, v):
        if o == 0x03:
            if v & 0x04:
                self.log({'t': 'warn', 'msg': 'sub selects 1M mode %02x' % v})
            if v & 0x01:                              # RET=1: return 2M Word RAM to main (HYPOTHESIS)
                self.memmode = (self.memmode & ~0x03) | 0x01
            self.memmode = (self.memmode & 0x03) | (v & 0x1C)
        elif o == 0x04:
            self.cdc_mode = (self.cdc_mode & 0xC000) | ((v & 7) << 8)
        elif o == 0x0A:
            self.cdc_dma = (self.cdc_dma & 0xFF) | (v << 8)
        elif o == 0x0B:
            self.cdc_dma = (self.cdc_dma & 0xFF00) | v
        elif o in (0x0C, 0x0D):
            self.stopwatch = 0
        elif o == 0x0F:
            if self.comflag_sub != v:
                self.comflag_sub = v
                self.lib.core_end_timeslice()
        elif 0x20 <= o < 0x30:
            i = (o - 0x20) >> 1
            if o & 1:
                self.comstat[i] = (self.comstat[i] & 0xFF00) | v
            else:
                self.comstat[i] = (self.comstat[i] & 0x00FF) | (v << 8)
            self.lib.core_end_timeslice()
        elif o == 0x31:
            self.timer = v
        elif o == 0x33:
            self.intmask = v
        elif 0x58 <= o < 0x68:
            self.asic[o] = v
            self.log({'t': 'asic_w', 'reg': 0xFF8000 + o, 'v': v})

    def _host_data_byte(self, o):
        if not self.cdc_host:
            return 0
        w = self.cdc_host[0]
        if o & 1:
            self.cdc_host.pop(0)
            if not self.cdc_host:
                self.cdc_mode = (self.cdc_mode & ~0x4000) | 0x8000
            return w & 0xFF
        return w >> 8

    # ------------------------------------------------------------ VDP
    def _vdp_r(self, a):
        o = a & 0x1F
        if o < 4:
            return self.vdp.data_read()
        if o < 8:
            return self.vdp.status()
        if o < 0x10:
            return self.vdp.hv()
        return 0

    def _vdp_w(self, a, v):
        o = a & 0x1F
        if o < 4:
            self.vdp.data_write(v)
        elif o < 8:
            self.vdp.ctrl_write(v)

    # ------------------------------------------------------------ boot / BIOS HLE
    def _load_boot(self):
        hdr = open(os.path.join(RAW, 'header.bin'), 'rb').read()
        ip = hdr[0x200:0x800] + open(os.path.join(RAW, 'ip.bin'), 'rb').read()
        ctypes.memmove(self.mram, ip, len(ip))
        sp = open(os.path.join(RAW, 'sp.bin'), 'rb').read()
        ctypes.memmove(ctypes.addressof(self.prg) + 0x6000, sp, len(sp))
        m, s = self.main, self.sub
        # ---- main: vectors + BIOS stubs ----
        for v in range(2, 64):
            struct.pack_into('>I', self.mrom, v * 4, 0x400)   # all exceptions -> fatal stub
        struct.pack_into('>H', self.mrom, 0x400, 0x4E73)
        self._install_gun_vector()
        for a in (0x364, 0x368):
            struct.pack_into('>H', self.mrom, a, 0x4E75)        # rts after the hook ran
        ctypes.memmove(ctypes.addressof(self.mrom) + 0x28C, bytes.fromhex('4cdf7fff4e73'), 6)  # BIOS VINT tail
        struct.pack_into('>H', self.mram, 0xFDAE, 0x4E75)        # main BIOS _BURAM vector (HLE)
        for a in (0x364, 0x368, 0x400, 0xFFFDAE):
            self.lib.core_add_hook(MAIN, a)
        m.sr(0x2700)
        m.a(7, 0xFFFD00)                                         # HYPOTHESIS: BIOS SSP
        m.pc(0xFF0584)                                           # ip_start
        # ---- sub: vectors, BIOS jump table area, boot stub ----
        sp_init = 0x6020 + struct.unpack('>H', sp[0x20:0x22])[0]
        sp_main = 0x6020 + struct.unpack('>H', sp[0x22:0x24])[0]
        sp_vint = 0x6020 + struct.unpack('>H', sp[0x24:0x26])[0]
        P = lambda a, b: ctypes.memmove(ctypes.addressof(self.prg) + a, b, len(b))
        for v in range(2, 64):
            P(v * 4, struct.pack('>I', 0x5E02))
        P(0x68, struct.pack('>I', 0x5E10))          # level 2 (main->sub) -> BIOS stub -> SP vint
        P(0x6C, struct.pack('>I', 0x5F82))          # level 3 timer -> _LEVEL3 jump slot
        P(0x5E00, bytes.fromhex('4e734e73'))        # 5E00 rte (default), 5E02 fatal (hooked)
        P(0x5E10, bytes.fromhex('48e7fffe4eb9') + struct.pack('>I', sp_vint) + bytes.fromhex('4cdf7fff4e73'))
        P(0x5E40, bytes.fromhex('4eb9') + struct.pack('>I', sp_init) + bytes.fromhex('46fc2000')
          + bytes.fromhex('4eb9') + struct.pack('>I', sp_main) + bytes.fromhex('60fe'))
        for a in range(0x5F00, 0x5F76, 2):
            P(a, bytes.fromhex('4e75'))
        for slot in (0x5F76, 0x5F7C, 0x5F82, 0x5F88, 0x5F8E, 0x5F94, 0x5F9A):
            P(slot, bytes.fromhex('4ef900005e00'))
        for a in [0x5E02] + list(range(0x5F0A, 0x5F76, 6)):
            self.lib.core_add_hook(SUB, a)
        s.sr(0x2700)
        s.a(7, 0x5E00)                               # HYPOTHESIS: BIOS supervisor stack top
        s.pc(0x5E40)

    def _hook(self, cpu, pc):
        try:
            (self._main_bios if cpu == MAIN else self._sub_bios)(pc)
        except Exception:
            import traceback
            self.log({'t': 'py_error', 'where': 'hook', 'pc': pc, 'err': traceback.format_exc()})

    def _ack(self, cpu, level):
        if cpu == MAIN:
            self.main_pending.discard(level)
            if self.main_pending:
                self.lib.core_set_irq(MAIN, max(self.main_pending))
            return
        if cpu == SUB:
            self.sub_pending.discard(level)
            if level == 2:
                self.ifl2 = False
                self.sub.waiting = None
            if self.sub_pending:
                self.lib.core_set_irq(SUB, max(self.sub_pending))

    def _main_bios(self, pc):
        m = self.main
        if pc == 0x364:
            self.log({'t': 'bios_main', 'fn': 'security'})
        elif pc == 0x368:
            # HYPOTHESIS: BIOS $368 = set VINT (level 6) handler to a1
            self.main_vint = m.a(1)
            struct.pack_into('>I', self.mrom, 0x78, self.main_vint)
            self.log({'t': 'bios_main', 'fn': 'set_vint', 'handler': self.main_vint})
        elif pc == 0xFFFDAE:
            self._buram(m)
        elif pc == 0x400:
            self.main.halted = True
            self.main.err = 'fatal exception (vector stub) ppc=%06x' % m.reg(R_PPC)
            self.log({'t': 'cpu_exception', 'cpu': 'main', 'ppc': m.reg(R_PPC), 'sp': m.a(7),
                      'stack': m.read(m.a(7), 16).hex(), 'pcs': ['%06x' % x for x in m.pc_ring()],
                      'd': [m.d(i) for i in range(8)], 'a': [m.a(i) for i in range(8)]})
            self.lib.core_end_timeslice()

    def enable_vm_trace(self):
        """Log every script-VM node dispatch (S:0x16974 vm_dispatch; a3 = node offset in
        script_buf S:0x1A800). Verbose: thousands of events per second of game time."""
        self.lib.core_add_hook(SUB, 0x16974)

    def _sub_bios(self, pc):
        s = self.sub
        if pc == 0x16974:
            a3 = s.a(3) & 0xFFFF
            self.log({'t': 'vm', 'node': a3, 'op': self.prg[0x1A800 + a3], 'script': self.last_script})
            return
        if pc == 0x5F10:            # _WAITVSYNC
            s.waiting = 'vsync'
            self.lib.core_end_timeslice()
        elif pc == 0x5F16:          # _BURAM
            self._buram()
        elif pc == 0x5F22:          # _CDBIOS
            self._cdbios()
        elif pc == 0x5E02:
            s.halted = True
            s.err = 'fatal exception (vector stub) ppc=%06x' % s.reg(R_PPC)
            self.log({'t': 'cpu_exception', 'cpu': 'sub', 'ppc': s.reg(R_PPC), 'sp': s.a(7),
                      'stack': s.read(s.a(7), 16).hex(), 'pcs': ['%06x' % x for x in s.pc_ring()[-40:]],
                      'd': [s.d(i) for i in range(8)],
                      'a': [s.a(i) for i in range(8)]})
            self.lib.core_end_timeslice()
        else:
            self.log({'t': 'bios_sub', 'fn': 'unknown', 'addr': pc, 'd0': s.d(0)})
        # the slot itself contains `rts`, executed after this hook

    BURAM_FN = {0: 'BRMINIT', 1: 'BRMSTAT', 2: 'BRMSERCH', 3: 'BRMREAD', 4: 'BRMWRITE',
                5: 'BRMDEL', 6: 'BRMFORMAT', 7: 'BRMDIR', 8: 'BRMVERIFY'}

    def _buram(self, s=None):
        """Backup RAM HLE (HYPOTHESIS: function codes per Sega CD BIOS manual); in-memory files.
        Used by the sub (_BURAM $5F16) and by the main CPU through its RAM vector $FFFDAE."""
        s = s or self.sub
        fn = s.d(0) & 0xFFFF
        name = self.BURAM_FN.get(fn, '?%d' % fn)
        if not hasattr(self, 'bram_files'):
            self.bram_files = {}
        a0, a1 = s.a(0), s.a(1)
        fname = lambda a: s.read(a, 11).split(b'\0')[0].decode('latin1')
        ok = True
        if s is self.main:
            # Main-side BURAM ($FFFDAE) addresses the RAM *cartridge* (HYPOTHESIS, evidence: the
            # game shows "CD Backup RAM Cart SET. Which will you use?" when it succeeds).
            # Model: no cartridge -> every call fails with d1=0.
            s.d(1, 0)
            self.log({'t': 'buram', 'cpu': 'main', 'fn': name, 'ok': False, 'note': 'no RAM cart'})
            s.set_carry(True)
            return
        if fn == 0:
            s.d(0, 0x7D); s.d(1, 0)
        elif fn == 1:
            used = sum((len(v) + 63) // 64 for v in self.bram_files.values())
            s.d(0, 0x7D - used); s.d(1, len(self.bram_files))
        elif fn == 2:
            n = fname(a0); ok = n in self.bram_files
            if ok:
                s.d(0, (len(self.bram_files[n]) + 63) // 64); s.d(1, 0)
        elif fn == 3:
            n = fname(a0); ok = n in self.bram_files
            if ok:
                s.write(a1, self.bram_files[n]); s.d(0, (len(self.bram_files[n]) + 63) // 64); s.d(1, 0)
        elif fn == 4:
            n = fname(a0); blocks = s.r16(a0 + 12)
            self.bram_files[n] = s.read(a1, blocks * 64)
        elif fn == 5:
            self.bram_files.pop(fname(a0), None)
        elif fn == 8:
            n = fname(a0); blocks = s.r16(a0 + 12)
            ok = self.bram_files.get(n) == s.read(a1, blocks * 64)
        self.log({'t': 'buram', 'cpu': s.name, 'fn': name, 'ok': ok, 'name': fname(a0) if fn in (2, 3, 4, 5, 8) else None})
        s.set_carry(not ok)

    CDBIOS_FN = {0x02: 'MSCSTOP', 0x03: 'MSCPAUSEON', 0x04: 'MSCPAUSEOFF', 0x08: 'ROMPAUSEON',
                 0x09: 'ROMPAUSEOFF', 0x10: 'DRVINIT', 0x11: 'MSCPLAY', 0x12: 'MSCPLAY1',
                 0x13: 'MSCPLAYR', 0x14: 'MSCPLAYT', 0x17: 'ROMREAD', 0x20: 'ROMREADN',
                 0x21: 'ROMREADE', 0x81: 'CDBSTAT', 0x84: 'CDBPAUSE', 0x89: 'CDCSTOP',
                 0x8A: 'CDCSTAT', 0x8B: 'CDCREAD', 0x8C: 'CDCTRN', 0x8D: 'CDCACK'}

    def _cd_fill(self):
        if self.cd_ready is None and self.cd_left > 0:
            self.cd_ready = (self.cd_lba, self.disc.sector(self.cd_lba))
            self.cd_lba += 1
            self.cd_left -= 1

    def _cdbios(self):
        """CD BIOS HLE. Function codes are HYPOTHESIS (Sega CD BIOS manual); the data-path
        semantics were checked against the SP file engine (S:0x6E06–0x7078)."""
        s = self.sub
        fn = s.d(0) & 0xFFFF
        name = self.CDBIOS_FN.get(fn, '?%02x' % fn)
        carry = False
        if fn in (0x17, 0x20, 0x21):        # ROMREAD*: a0 -> [lba.l, count.l]
            lba = s.r32(s.a(0))
            cnt = s.r32(s.a(0) + 4) if fn != 0x17 else 0x7FFFFFFF
            self.cd_lba, self.cd_left, self.cd_ready, self.cd_first_dest = lba, cnt, None, None
            n, off = self.disc.name_of(lba)
            self.log({'t': 'cd_read', 'fn': name, 'lba': lba, 'count': cnt if cnt < 0x7FFFFFFF else -1,
                      'file': n, 'sector_in_file': off})
            self._cd_fill()
        elif fn == 0x8A:                    # CDCSTAT: cc if a sector is ready
            self._cd_fill()
            carry = self.cd_ready is None
        elif fn == 0x8B:                    # CDCREAD: header in d0, start transfer to DD device
            self._cd_fill()
            if self.cd_ready is None:
                carry = True
            else:
                lba, data = self.cd_ready
                self.cd_cur, self.cd_ready = self.cd_ready, None
                t = lba + 150
                s.d(0, (bcd(t // 4500) << 24) | (bcd((t // 75) % 60) << 16) | (bcd(t % 75) << 8) | 0x01)
                s.d(1, 0)
                self._cdc_transfer(data)
        elif fn == 0x8C:                    # CDCTRN: 2048 bytes to a0, header to a1
            if self.cd_cur is None:
                carry = True
            else:
                lba, data = self.cd_cur
                a0, a1 = s.a(0), s.a(1)
                if a0 + 2048 <= 0x80000:
                    ctypes.memmove(ctypes.addressof(self.prg) + a0, data, 2048)
                else:
                    s.write(a0, data)
                t = lba + 150
                s.write(a1, bytes([bcd(t // 4500), bcd((t // 75) % 60), bcd(t % 75), 1]))
                if self.cd_first_dest is None:
                    self.cd_first_dest = a0
                    self.log({'t': 'cd_dest', 'mode': 'cdctrn', 'addr': a0})
                s.a(0, a0 + 2048)
                s.a(1, a1 + 4)
                self.cdc_mode &= ~0x4000
        elif fn == 0x8D:                    # CDCACK
            self.cd_cur = None
        elif fn == 0x89:                    # CDCSTOP
            self.cd_left = 0
            self.cd_ready = None
        elif fn == 0x81:                    # CDBSTAT: a0 -> status block (HYPOTHESIS layout)
            blk = 0x5E80
            s.write(blk, struct.pack('>HHIII', self.music_status, 0, 0, 0xFFFFFFFF, 0xFFFFFFFF))
            s.a(0, blk)
        elif fn in (0x11, 0x12, 0x13):      # MSCPLAY*: a0 -> track number word
            tr = s.r16(s.a(0))
            self.music = {'track': tr, 'mode': name, 'start_frame': self.frame,
                          'end_frame': self.frame + self.track_len.get(tr, 10 ** 9) if fn == 0x12 else None}
            self.music_status = 0x0100
            self.log({'t': 'music', 'fn': name, 'track': tr})
        elif fn in (0x02, 0x03, 0x04):
            self.music_status = {0x02: 0x0000, 0x03: 0x0500, 0x04: 0x0100 if self.music else 0}[fn]
            self.log({'t': 'music', 'fn': name})
        else:
            self.log({'t': 'cdbios', 'fn': name, 'd1': s.d(1), 'a0': s.a(0)})
        s.set_carry(carry)

    def _cdc_transfer(self, data):
        """CDC destination DD (HYPOTHESIS, Sega docs/GPGX cdc.c): 2 main read, 3 sub read,
        4 PCM DMA, 5 PRG-RAM DMA, 7 Word RAM DMA. DMA completes instantly (EDT set)."""
        dd = (self.cdc_mode >> 8) & 7
        self.cdc_mode &= ~0xC000
        if self.cd_first_dest is None and dd in (4, 5, 7):
            self.cd_first_dest = self.cdc_dma
            self.log({'t': 'cd_dest', 'mode': 'dma', 'dd': dd, 'reg': self.cdc_dma})
        if dd in (2, 3):
            self.cdc_mode |= 0x4000
            self.cdc_host = list(struct.unpack('>1024H', data))
        elif dd == 5:
            a = (self.cdc_dma << 3) & 0x7FFF8
            ctypes.memmove(ctypes.addressof(self.prg) + a, data, min(2048, 0x80000 - a))
            self.cdc_dma = (self.cdc_dma + 256) & 0xFFFF
            self.cdc_mode |= 0x8000
        elif dd == 7:
            a = (self.cdc_dma << 3) & 0x3FFF8
            ctypes.memmove(ctypes.addressof(self.wram) + a, data, min(2048, 0x40000 - a))
            self.cdc_dma = (self.cdc_dma + 256) & 0xFFFF
            self.cdc_mode |= 0x8000
        elif dd == 4:
            self.pcm.dma((self.cdc_dma << 2) & 0xFFC, data)
            self.cdc_dma = (self.cdc_dma + 512) & 0xFFFF
            self.cdc_mode |= 0x8000
        else:
            self.log({'t': 'warn', 'msg': 'cdc transfer dd=%d' % dd})

    # ------------------------------------------------------------ scheduler
    def _poll_trace(self):
        busy = self.prg[0x9B44]
        if busy and not self.last_seq_busy:
            q = bytes(self.prg[0x9940:0x9940 + 0x80])
            self.log({'t': 'seq_start', 'queue': q.rstrip(b'\0').hex(), 'sub_pc': self.sub.pc()})
        self.last_seq_busy = busy
        sc = (self.prg[0x980C] << 8) | self.prg[0x980D]
        if sc != self.last_script:
            self.log({'t': 'script_file', 'id': sc})
            self.last_script = sc

    def _set_sub_irq(self, level):
        self.sub_pending.add(level)
        self.lib.core_set_irq(SUB, max(self.sub_pending))

    # ------------------------------------------------------------ save states
    STATE_ATTRS = ['frame', 'pad', 'comflag_main', 'comflag_sub', 'comcmd', 'comstat', 'memmode', 'wp',
                   'intmask', 'timer', 'timer_acc', 'ifl2', 'cdc_mode', 'cdc_dma', 'cdc_host', 'stopwatch',
                   'asic', 'main_vint', 'main_hint_vec', 'pad_th', 'pad_ctrl', 'ym_addr', 'sub_pending',
                   'main_pending', 'cd_lba', 'cd_left', 'cd_ready', 'cd_cur', 'cd_first_dest', 'music',
                   'music_status', 'last_seq_busy', 'last_script', 'z80']

    def save_state(self, path):
        import pickle
        n = self.lib.core_ctx_size() + 4
        ctxs = []
        for c in (MAIN, SUB):
            b = (ctypes.c_ubyte * n)()
            self.lib.core_save_ctx(c, b)
            ctxs.append(bytes(b))
        st = {k: getattr(self, k) for k in self.STATE_ATTRS}
        st['bram_files'] = getattr(self, 'bram_files', {})
        st['mem'] = {k: bytes(getattr(self, k)) for k in ('mrom', 'prg', 'wram', 'mram', 'bram')}
        st['ctx'] = ctxs
        st['halted_waiting'] = [(c.halted, c.waiting) for c in (self.main, self.sub)]
        st['vdp'] = {k: getattr(self.vdp, k) for k in ('vram', 'cram', 'vsram', 'reg', 'addr', 'code',
                                                    'pending', 'fill_pending', 'vblank', 'line')}
        st['vdp']['fill_len'] = getattr(self.vdp, 'fill_len', 0)
        st['pcm'] = {k: v for k, v in self.pcm.__dict__.items() if k != 'log'}
        with open(path, 'wb') as f:
            pickle.dump(st, f)

    def load_state(self, path):
        import pickle
        with open(path, 'rb') as f:
            st = pickle.load(f)
        for k in self.STATE_ATTRS:
            setattr(self, k, st[k])
        self.bram_files = st['bram_files']
        for k, v in st['mem'].items():
            ctypes.memmove(getattr(self, k), v, len(v))
        for c, b in zip((MAIN, SUB), st['ctx']):
            self.lib.core_load_ctx(c, (ctypes.c_ubyte * len(b)).from_buffer_copy(b))
        for c, (h, w) in zip((self.main, self.sub), st['halted_waiting']):
            c.halted, c.waiting = h, w
        for k, v in st['vdp'].items():
            setattr(self.vdp, k, v)
        self.pcm.__dict__.update(st['pcm'])
        struct.pack_into('>I', self.mrom, 0x70, 0xFF0000 | self.main_hint_vec)
        self._install_gun_vector()

    def _set_main_irq(self, level):
        self.main_pending.add(level)
        self.lib.core_set_irq(MAIN, max(self.main_pending))

    def step_frame(self, capture=False):
        # zero-length script entries ([0, bits]) are skipped: scripts built as [[f-t-1,0],[1,b],...] produce them for
        # consecutive frames and each one used to eat a frame with pad = 0 (held buttons arrived every other frame)
        while self.inputs and self.inputs[0][0] <= 0:
            self.inputs.pop(0)
        if self.inputs:
            n, bits = self.inputs[0]
            self.pad = bits
            self.inputs[0] = [n - 1, bits]
            if n - 1 <= 0:
                self.inputs.pop(0)
        else:
            self.pad = 0
        L = self.LINES
        mcyc = MAIN_HZ / 60 / L
        scyc = SUB_HZ / 60 / L
        dt = 1.0 / 60 / L
        if capture:
            self.vdp.begin_frame()
        hcount = self.vdp.reg[10]
        macc = sacc = 0.0
        self.gun_fired = False
        for line in range(L):
            self.vdp.line = line
            self.mcl = 0.0
            if capture and line < 224:
                self.vdp.render_line(line)
            vint_delay = None
            if line == 224:
                # V-blank flag rises at the end of line 223; VINT asserted ~one HBLANK later
                # (GPGX timing). The game relies on it: M:0xFF2D34 polls the flag with ints on.
                self.vdp.vblank = True
                if self.vdp.reg[1] & 0x20 and self.main_vint:
                    vint_delay = 180
            elif line == 0:
                self.vdp.vblank = False
            # H-interrupt line counter (Sega VDP manual: reloaded from reg 10 outside the display)
            if line <= 224:
                if hcount == 0:
                    hcount = self.vdp.reg[10]
                    if self.vdp.reg[0] & 0x10:
                        self._set_main_irq(4)
                else:
                    hcount -= 1
            else:
                hcount = self.vdp.reg[10]
            if self.intmask & 0x08 and self.timer:
                self.timer_acc += dt
                period = (self.timer + 1) * 30.72e-6
                if self.timer_acc >= period:
                    self.timer_acc %= period
                    self._set_sub_irq(3)
            macc += mcyc
            sacc += scyc
            mleft, sleft = int(macc), int(sacc)
            macc -= mleft
            sacc -= sleft
            CH = 250
            while mleft > 0 or sleft > 0:
                if self.ifl2 and self.intmask & 0x04 and 2 not in self.sub_pending:
                    self._set_sub_irq(2)
                if vint_delay is not None and mleft > 0 and not self.main.halted:
                    used = max(self.main.run(vint_delay), 1)
                    mleft -= used
                    self.mcl += used
                    self._set_main_irq(6)
                    vint_delay = None
                if mleft > 0:
                    if self.main.halted:
                        mleft = 0
                    else:
                        chunk = min(CH, mleft)
                        g = self.gun
                        if g is not None and self.pad2_ctrl & 0x80 and self.vdp.reg[11] & 0x08 and line < 224:
                            # the gun's sensor sees the beam at (x, y): external interrupt (level 2) at that moment; the HV counter is read live by the
                            # handler, so the main CPU is run in small slices from the trigger until the handler is done
                            if not self.gun_fired and line == min(max(int(g['y']), 0), 223):
                                tgt = min(max(int(g['x']), 0), 255) * (MAIN_HZ / 60 / L) / 342.0
                                if self.mcl >= tgt:
                                    self._set_main_irq(2)
                                    self.gun_fired = True
                                    self.gun_hot = 1100
                                else:
                                    chunk = max(1, min(chunk, int(tgt - self.mcl) + 1))
                            if self.gun_fired and self.gun_hot > 0:
                                chunk = min(chunk, 6)
                        used = max(self.main.run(chunk), 1)
                        mleft -= used
                        self.mcl += used
                        if self.gun_hot > 0:
                            self.gun_hot -= used
                if self.sub.waiting == 'vsync' and 2 in self.sub_pending:
                    self.sub.waiting = None          # _WAITVSYNC returns once the level-2 int is taken
                if sleft > 0:
                    if self.sub.halted or self.sub.waiting:
                        sleft = 0
                    else:
                        sleft -= max(self.sub.run(min(CH * 163 // 100, sleft)), 1)
                    self._poll_trace()
        self.pcm.advance(1.0 / 60)
        self.gun_hot = 0
        if self.music and self.music.get('end_frame') and self.frame >= self.music['end_frame'] \
                and self.music_status == 0x0100:
            # MSCPLAY1 (play once) finished: HYPOTHESIS status returns to 0x0000 (stopped)
            self.music_status = 0x0000
            self.log({'t': 'music', 'fn': 'end_of_track', 'track': self.music['track']})
        self.frame += 1
        w = lambda a: (self.prg[a] << 8) | self.prg[a + 1]
        rec = {'t': 'frame', 'f': self.frame, 'sp_state': w(0x7840), 'state': w(0x9808), 'vm_pc': w(0x9800),
               'script': w(0x980C), 'comcmd': list(self.comcmd), 'comstat': list(self.comstat),
               'flags': [self.comflag_main, self.comflag_sub], 'main_pc': self.main.pc(),
               'sub_pc': self.sub.pc()}
        if self.ym_frame:
            rec['ym'] = self.ym_frame
            self.ym_frame = []
        if self.trace:
            for e in self.events:
                self.trace.write(json.dumps(e) + '\n')
            self.trace.write(json.dumps(rec) + '\n')
            self.trace.flush()
        self.events = []
        return rec


class PCM:
    """RF5C164 model (HYPOTHESIS: register map per Sega CD hardware manual). Sub addresses
    0xFF0001.. odd bytes: 0 ENV, 1 PAN, 2 FDL, 3 FDH, 4 LSL, 5 LSH, 6 ST, 7 CTRL, 8 ON/OFF;
    current address read at regs 0x10..0x1F; wave RAM window regs 0x1000-0x1FFF."""
    def __init__(self, log=None):
        self.log = log or (lambda e: None)
        self.ram = bytearray(0x10000)
        self.ch = [dict(env=0, pan=0, fd=0, ls=0, st=0, addr=0.0) for _ in range(8)]
        self.ctrl = 0
        self.onoff = 0xFF
        self.cur = 0
        self.bank = 0

    def read(self, off):
        if off & 1 == 0:
            return 0
        r = off >> 1
        if 0x10 <= r < 0x20:
            c = self.ch[(r - 0x10) >> 1]
            a = int(c['addr']) & 0xFFFF
            return (a >> 8) & 0xFF if r & 1 else a & 0xFF
        if r >= 0x1000:
            return self.ram[(self.bank << 12) | (r & 0xFFF)]
        return 0

    def write(self, off, v):
        if off & 1 == 0:
            return
        r = off >> 1
        if r >= 0x1000:
            self.ram[(self.bank << 12) | (r & 0xFFF)] = v
            return
        c = self.ch[self.cur]
        if r == 0: c['env'] = v
        elif r == 1: c['pan'] = v
        elif r == 2: c['fd'] = (c['fd'] & 0xFF00) | v
        elif r == 3: c['fd'] = (c['fd'] & 0xFF) | (v << 8)
        elif r == 4: c['ls'] = (c['ls'] & 0xFF00) | v
        elif r == 5: c['ls'] = (c['ls'] & 0xFF) | (v << 8)
        elif r == 6: c['st'] = v
        elif r == 7:
            self.ctrl = v
            if v & 0x40:
                self.cur = v & 7
            else:
                self.bank = v & 0x0F
        elif r == 8:
            for i in range(8):
                if (self.onoff >> i & 1) and not (v >> i & 1):
                    c = self.ch[i]
                    c['addr'] = float(c['st'] << 8)
                    self.log({'t': 'pcm_on', 'ch': i, 'st': c['st'], 'fd': c['fd'], 'ls': c['ls'],
                              'env': c['env'], 'pan': c['pan']})
            self.onoff = v

    def dma(self, a, data):
        base = self.bank << 12
        for i, b in enumerate(data):
            self.ram[base + ((a + i) & 0xFFF)] = b

    def advance(self, dt):
        if not (self.ctrl & 0x80):
            return
        n = int(dt * 32552)
        for i, c in enumerate(self.ch):
            if self.onoff >> i & 1 or c['fd'] == 0:
                continue
            a = c['addr']
            end = a + c['fd'] / 2048.0 * n
            ia, ie = int(a) & 0xFFFF, int(end)
            if ie - int(a) < 0x10000 and ia <= (ie & 0xFFFF) and 0xFF in self.ram[ia:(ie & 0xFFFF) + 1]:
                c['addr'] = float(c['ls'])
            else:
                c['addr'] = end % 0x10000
