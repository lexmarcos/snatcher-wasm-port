"""Sega CD test oracle: runs the ORIGINAL Snatcher 68000 code (main IP + sub SP/SUBCODE)
in two unicorn M68000 cores with high-level emulation (HLE) of the BIOS and hardware.
No BIOS image is used. See docs/re/oracle.md for the assumptions (marked HYPOTHESIS).
"""
import ctypes, json, os, struct
from unicorn import Uc, UcError, UC_ARCH_M68K, UC_MODE_BIG_ENDIAN, UC_PROT_ALL, UC_HOOK_CODE, \
    UC_HOOK_INTR, UC_HOOK_MEM_WRITE, UC_HOOK_MEM_UNMAPPED
from unicorn.m68k_const import *
from vdp import VDP

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..'))
RAW = os.path.join(ROOT, 'assets', 'raw')
BIN = os.path.join(ROOT, 'work', 'snatcher.bin')

DREGS = [UC_M68K_REG_D0, UC_M68K_REG_D1, UC_M68K_REG_D2, UC_M68K_REG_D3,
         UC_M68K_REG_D4, UC_M68K_REG_D5, UC_M68K_REG_D6, UC_M68K_REG_D7]
AREGS = [UC_M68K_REG_A0, UC_M68K_REG_A1, UC_M68K_REG_A2, UC_M68K_REG_A3,
         UC_M68K_REG_A4, UC_M68K_REG_A5, UC_M68K_REG_A6, UC_M68K_REG_A7]
CPU_M68000 = 1  # UC_CPU_M68K_M68000 (unicorn.m68k_const)
try:
    from unicorn.m68k_const import UC_CPU_M68K_M68000 as CPU_M68000
except ImportError:
    pass

SR_C = 1
SR_Z = 4


def buf(size):
    return (ctypes.c_ubyte * size)()


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
            if l <= lba < l + (s + 2047) // 2048:
                return n, lba - l
        return None, 0


def bcd(v):
    return ((v // 10) << 4) | (v % 10)


class Cpu:
    def __init__(self, name, m):
        self.name = name
        self.m = m
        self.uc = Uc(UC_ARCH_M68K, UC_MODE_BIG_ENDIAN, CPU_M68000)
        self.uc.hook_add(UC_HOOK_INTR, self._intr)
        self.uc.hook_add(UC_HOOK_MEM_UNMAPPED, self._unmapped)
        self.halted = False
        self.waiting = None      # e.g. 'vsync'
        self.err = None
        self.icount = 0

    # registers
    def d(self, i, v=None):
        if v is None:
            return self.uc.reg_read(DREGS[i]) & 0xFFFFFFFF
        self.uc.reg_write(DREGS[i], v & 0xFFFFFFFF)

    def a(self, i, v=None):
        if v is None:
            return self.uc.reg_read(AREGS[i]) & 0xFFFFFFFF
        self.uc.reg_write(AREGS[i], v & 0xFFFFFFFF)

    def pc(self, v=None):
        if v is None:
            return self.uc.reg_read(UC_M68K_REG_PC) & 0xFFFFFFFF
        self.uc.reg_write(UC_M68K_REG_PC, v & 0xFFFFFFFF)

    def sr(self, v=None):
        if v is None:
            return self.uc.reg_read(UC_M68K_REG_SR) & 0xFFFF
        self.uc.reg_write(UC_M68K_REG_SR, v & 0xFFFF)

    def set_carry(self, c):
        s = self.sr()
        self.sr((s | SR_C) if c else (s & ~SR_C))

    # memory
    def r8(self, a):
        return self.uc.mem_read(a & 0xFFFFFFFF, 1)[0]

    def r16(self, a):
        return struct.unpack('>H', self.uc.mem_read(a, 2))[0]

    def r32(self, a):
        return struct.unpack('>I', self.uc.mem_read(a, 4))[0]

    def w8(self, a, v):
        self.uc.mem_write(a, bytes([v & 0xFF]))

    def w16(self, a, v):
        self.uc.mem_write(a, struct.pack('>H', v & 0xFFFF))

    def w32(self, a, v):
        self.uc.mem_write(a, struct.pack('>I', v & 0xFFFFFFFF))

    def push32(self, v):
        sp = self.a(7) - 4
        self.w32(sp, v)
        self.a(7, sp)

    def pop32(self):
        sp = self.a(7)
        v = self.r32(sp)
        self.a(7, sp + 4)
        return v

    def rts(self):
        self.pc(self.pop32())

    # exceptions
    def _intr(self, uc, intno, ud):
        if intno == 256:   # EXCP_RTE: emulate 68000 short frame (SR, PC)
            sp = self.a(7)
            sr = self.r16(sp)
            pc = self.r32(sp + 2)
            self.a(7, sp + 6)
            self.sr(sr)
            self.pc(pc)
            return
        self.err = 'exception %d at pc=%06x' % (intno, self.pc())
        sp = self.a(7)
        self.m.log({'t': 'cpu_exception', 'cpu': self.name, 'intno': intno, 'pc': self.pc(),
                    'd': [self.d(i) for i in range(8)], 'a': [self.a(i) for i in range(8)],
                    'stack': bytes(self.uc.mem_read(sp, 32)).hex()})
        self.halted = True
        uc.emu_stop()

    def _unmapped(self, uc, access, addr, size, value, ud):
        self.err = 'unmapped access %d @%08x size %d pc=%06x' % (access, addr, size, self.pc())
        self.m.log({'t': 'unmapped', 'cpu': self.name, 'addr': addr, 'pc': self.pc(), 'access': access})
        self.halted = True
        return False

    def irq(self, level, vector):
        """Take an autovectored interrupt now if the mask allows (68000 short frame)."""
        s = self.sr()
        if ((s >> 8) & 7) >= level and level < 7:
            return False
        if not (s & 0x2000):
            self.m.log({'t': 'warn', 'msg': '%s irq in user mode' % self.name})
        self.waiting = None if self.waiting == 'vsync' and level == 2 and self.name == 'sub' else self.waiting
        pc = self.pc()
        self.push32(pc)
        sp = self.a(7) - 2
        self.w16(sp, s)
        self.a(7, sp)
        self.sr((s | 0x2000) & ~0x0700 | (level << 8))
        self.pc(vector)
        return True

    def run(self, count):
        if self.halted or self.waiting or count <= 0:
            return
        try:
            self.uc.emu_start(self.pc(), 0xFFFFFFFF, 0, count)
        except UcError as e:
            if not self.err:
                self.err = '%s at pc=%06x' % (e, self.pc())
                self.m.log({'t': 'uc_error', 'cpu': self.name, 'err': str(e), 'pc': self.pc()})
            self.halted = True
        self.icount += count


class Machine:
    MAIN_INSN_PER_FRAME = 16000    # ~7.67 MHz / 60 / ~8 cycles per instruction (HYPOTHESIS)
    SUB_INSN_PER_FRAME = 26000     # ~12.5 MHz / 60 / ~8
    LINES = 262

    def __init__(self, trace_path=None, verbose=False):
        self.verbose = verbose
        self.trace = open(trace_path, 'w') if trace_path else None
        self.frame = 0
        self.events = []
        self.disc = Disc()
        self.inputs = []
        self.pad = 0
        # shared memories
        self.prg = buf(0x80000)
        self.wram = buf(0x40000)
        self.mram = buf(0x10000)
        self.z80 = buf(0x10000)
        self.bram = buf(0x4000)
        # gate array state
        self.comflag_main = 0
        self.comflag_sub = 0
        self.comcmd = [0] * 8
        self.comstat = [0] * 8
        self.memmode = 0x01        # sub: RET=1 (main owns word RAM) at boot (HYPOTHESIS)
        self.wp = 0
        self.intmask = 0x0004      # BIOS leaves level 2 enabled (HYPOTHESIS)
        self.timer = 0
        self.timer_acc = 0.0
        self.ifl2 = False
        self.cdc_mode = 0          # DD bits / EDT / DSR
        self.cdc_dma = 0
        self.cdc_host = []         # words for main/sub host-data reads
        self.stopwatch = 0
        self.asic = {}
        self.main_vint = None
        self.main_hint_vec = 0
        self.z80_busreq = 0
        self.pad_th = 0x40
        self.pad_ctrl = 0
        # CDC / drive HLE
        self.cd_lba = 0
        self.cd_left = 0
        self.cd_ready = None       # (lba, data) of the sector waiting in the CDC buffer
        self.cd_cur = None         # sector being transferred after CDCREAD
        self.music = None
        self.music_status = 0x0000
        # PCM
        self.pcm = PCM()
        self.vdp = VDP(self.main_read16)
        self.main = Cpu('main', self)
        self.sub = Cpu('sub', self)
        self._map_main()
        self._map_sub()
        self._load_boot()
        self.pending_cmds = []
        self.ym_frame = []
        self.timer_irq_due = 0
        self.cd_first_dest = None

    # ------------------------------------------------------------ logging
    def log(self, ev):
        ev = dict(ev)
        ev['f'] = self.frame
        self.events.append(ev)
        if self.verbose:
            print(json.dumps(ev))

    # ------------------------------------------------------------ memory maps
    def _map_main(self):
        uc = self.main.uc
        self.mrom = buf(0x20000)
        uc.mem_map_ptr(0x000000, 0x20000, UC_PROT_ALL, self.mrom)
        uc.mem_map_ptr(0x020000, 0x20000, UC_PROT_ALL, self.prg)
        uc.mem_map_ptr(0x200000, 0x40000, UC_PROT_ALL, self.wram)
        uc.mem_map_ptr(0xA00000, 0x4000, UC_PROT_ALL, self.z80)
        uc.mmio_map(0xA04000, 0x1000, self._ym_r, None, self._ym_w, None)
        self.z80hi = buf(0xB000)
        uc.mem_map_ptr(0xA05000, 0xB000, UC_PROT_ALL, self.z80hi)
        uc.mmio_map(0xA10000, 0x10000, self._main_io_r, None, self._main_io_w, None)
        uc.mmio_map(0xC00000, 0x10000, self._vdp_r, None, self._vdp_w, None)
        uc.mem_map_ptr(0xFF0000, 0x10000, UC_PROT_ALL, self.mram)
        uc.mem_map_ptr(0xFFFF0000, 0x10000, UC_PROT_ALL, self.mram)   # abs-short mirror
        # also catch high-byte aliases of IO used via sign-extended addressing (none expected)

    def _map_sub(self):
        uc = self.sub.uc
        uc.mem_map_ptr(0x000000, 0x80000, UC_PROT_ALL, self.prg)
        uc.mem_map_ptr(0x080000, 0x40000, UC_PROT_ALL, self.wram)
        self.wram1m = buf(0x20000)
        uc.mem_map_ptr(0x0C0000, 0x20000, UC_PROT_ALL, self.wram1m)
        uc.mem_map_ptr(0xFE0000, 0x4000, UC_PROT_ALL, self.bram)
        for base in (0xFF0000, 0xFFFF0000):
            uc.mmio_map(base, 0x8000, self._pcm_r, None, self._pcm_w, None)
            uc.mmio_map(base + 0x8000, 0x8000, self._ga_sub_r, None, self._ga_sub_w, None)
        # watch the action queue busy flag to log queue contents (S:$9B44)
        uc.hook_add(UC_HOOK_MEM_WRITE, self._seq_hook, None, 0x9B44, 0x9B44)
        uc.hook_add(UC_HOOK_MEM_WRITE, self._file_hook, None, 0x980C, 0x980D)

    def _load_boot(self):
        hdr = open(os.path.join(RAW, 'header.bin'), 'rb').read()
        ip = hdr[0x200:0x800] + open(os.path.join(RAW, 'ip.bin'), 'rb').read()
        ctypes.memmove(self.mram, ip, len(ip))
        sp = open(os.path.join(RAW, 'sp.bin'), 'rb').read()
        ctypes.memmove(ctypes.addressof(self.prg) + 0x6000, sp, len(sp))
        m, s = self.main, self.sub
        # ---- main BIOS stubs (HLE hooks) ----
        # $364 security check, $368 set VINT handler (a1) [HYPOTHESIS], $28C BIOS VINT tail.
        m.w16(0x364, 0x4E75)
        m.w16(0x368, 0x4E75)
        m.uc.mem_write(0x28C, bytes.fromhex('4cdf7fff4e73'))   # movem.l (sp)+,d0-d7/a0-a6; rte
        m.uc.hook_add(UC_HOOK_CODE, self._main_bios, None, 0x000000, 0x0003FF)
        m.sr(0x2700)       # set SR first: changing S swaps A7 banks in unicorn/QEMU
        m.a(7, 0xFFFD00)   # HYPOTHESIS: BIOS leaves SSP near top of RAM below its vectors
        m.pc(0xFF0584)     # ip_start (after the security code)
        # ---- sub BIOS area ----
        hle = 0x5E00
        s.uc.mem_write(hle, bytes.fromhex('4e73'))                       # 5E00: rte (default vector)
        sp_vint = 0x6020 + struct.unpack('>H', sp[0x24:0x26])[0]
        sp_init = 0x6020 + struct.unpack('>H', sp[0x20:0x22])[0]
        sp_main = 0x6020 + struct.unpack('>H', sp[0x22:0x24])[0]
        # 5E10: level-2 handler = BIOS calling the SP's vint entry (HYPOTHESIS)
        s.uc.mem_write(0x5E10, bytes.fromhex('48e7fffe4eb9') + struct.pack('>I', sp_vint) + bytes.fromhex('4cdf7fff4e73'))
        # 5E40: boot: jsr init; enable interrupts; jsr main; bra *
        s.uc.mem_write(0x5E40, bytes.fromhex('4eb9') + struct.pack('>I', sp_init) + bytes.fromhex('46fc2000')
                       + bytes.fromhex('4eb9') + struct.pack('>I', sp_main) + bytes.fromhex('60fe'))
        # BIOS jump table slots 0x5F0A..0x5FFF: 'nop' filler, intercepted by the code hook
        for a in range(0x5F00, 0x6000, 2):
            s.w16(a, 0x4E71)
        # _LEVELx jump slots: jmp to rte stub (game patches _LEVEL3 at $5F82/$5F84)
        for slot in (0x5F76, 0x5F7C, 0x5F82, 0x5F88, 0x5F8E, 0x5F94, 0x5F9A):
            s.uc.mem_write(slot, bytes.fromhex('4ef9') + struct.pack('>I', hle))
        s.uc.hook_add(UC_HOOK_CODE, self._sub_bios, None, 0x5F00, 0x5F75)
        s.sr(0x2700)
        s.a(7, 0x5E00)     # HYPOTHESIS: BIOS supervisor stack top (BIOS uses 0x5E80 status area)
        s.pc(0x5E40)
        self.sp_vint = sp_vint

    # ------------------------------------------------------------ main IO
    def main_read16(self, a):
        a &= 0xFFFFFF
        if 0x200000 <= a < 0x240000:
            o = a - 0x200000
            return (self.wram[o] << 8) | self.wram[o + 1]
        if a >= 0xFF0000:
            o = a - 0xFF0000
            return (self.mram[o] << 8) | self.mram[o + 1]
        if a < 0x20000:
            return (self.mrom[a] << 8) | self.mrom[a + 1]
        return 0

    def _main_io_r(self, uc, off, size, ud):
        a = 0xA10000 + off
        if size == 2:
            return (self._main_io_r8(a) << 8) | self._main_io_r8(a + 1)
        if size == 4:
            return (self._main_io_r(uc, off, 2, ud) << 16) | self._main_io_r(uc, off + 2, 2, ud)
        return self._main_io_r8(a)

    def _main_io_r8(self, a):
        if a == 0xA10001:
            return 0x80                          # overseas NTSC, MCD present (bit5=0)
        if a == 0xA10003:
            return self._pad_read()
        if a in (0xA10005, 0xA10007):
            return 0x7F
        if a == 0xA10009:
            return self.pad_ctrl
        if 0xA1000B <= a <= 0xA1001F:
            return 0
        if a in (0xA11100, 0xA11101):
            return 0x00 if a == 0xA11100 else 0     # bus granted immediately
        if 0xA12000 <= a < 0xA12040:
            return self._ga_main_r8(a - 0xA12000)
        return 0

    def _pad_read(self):
        p = self.pad   # bits: UP DOWN LEFT RIGHT B C A START (engine::pad)
        u, d, l, r = p & 1, p >> 1 & 1, p >> 2 & 1, p >> 3 & 1
        b, c, aa, st = p >> 4 & 1, p >> 5 & 1, p >> 6 & 1, p >> 7 & 1
        if self.pad_th & 0x40:
            v = 0x40 | ((not c) << 5) | ((not b) << 4) | ((not r) << 3) | ((not l) << 2) | ((not d) << 1) | (not u)
        else:
            v = ((not st) << 5) | ((not aa) << 4) | ((not d) << 1) | (not u)
        return v & 0x7F | (self.pad_th & 0x80)

    def _main_io_w(self, uc, off, size, value, ud):
        a = 0xA10000 + off
        if size == 4:
            self._main_io_w(uc, off, 2, value >> 16, ud)
            self._main_io_w(uc, off + 2, 2, value & 0xFFFF, ud)
            return
        if size == 2:
            if 0xA12000 <= a < 0xA12040:
                self._ga_main_w(a - 0xA12000, value, 2)
                return
            self._main_io_w8(a, value >> 8)
            self._main_io_w8(a + 1, value & 0xFF)
            return
        if 0xA12000 <= a < 0xA12040:
            self._ga_main_w(a - 0xA12000, value, 1)
            return
        self._main_io_w8(a, value)

    def _main_io_w8(self, a, v):
        if a == 0xA10003:
            self.pad_th = v & 0x40 if self.pad_ctrl & 0x40 else 0x40
        elif a == 0xA10009:
            self.pad_ctrl = v
        elif a == 0xA11100:
            self.z80_busreq = v
        # Z80 reset/other writes ignored (sound CPU not emulated)

    # ------------------------------------------------------------ gate array
    def _ga_main_r8(self, o):
        if o == 0x00:
            return (0x80 if self.intmask & 4 else 0) | (1 if self.ifl2 else 0)
        if o == 0x01:
            return 0x01                          # SRES=1 (sub running), SBRQ=0
        if o == 0x02:
            return self.wp
        if o == 0x03:
            return self.memmode & 0x07 | 0x00
        if o == 0x04:
            return self.cdc_mode >> 8 & 0xC7
        if o == 0x05:
            return 0
        if o in (0x06, 0x07):
            return (self.main_hint_vec >> (8 if o == 6 else 0)) & 0xFF
        if o in (0x08, 0x09):
            return self._host_data_byte(o)
        if o in (0x0C, 0x0D):
            return 0
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
                return
            self._ga_main_w(o, v >> 8, 1)
            self._ga_main_w(o + 1, v & 0xFF, 1)
            return
        if o == 0x00:
            if v & 0x01 and not self.ifl2:
                self.ifl2 = True                 # request sub level-2 interrupt
                self.main.uc.emu_stop()
        elif o == 0x01:
            pass
        elif o == 0x02:
            self.wp = v
        elif o == 0x03:
            # bit1 DMNA: main gives Word RAM (2M) to the sub CPU (HYPOTHESIS: immediate swap)
            if v & 0x02:
                if self.memmode & 0x04:
                    self.log({'t': 'warn', 'msg': '1M mode DMNA'})
                self.memmode = (self.memmode & ~0x03) | 0x02
        elif o in (0x06, 0x07):
            sh = 8 if o == 6 else 0
            self.main_hint_vec = (self.main_hint_vec & ~(0xFF << sh)) | (v << sh)
        elif o == 0x0E:
            # NOTE: emu_stop from an MMIO callback restarts the instruction; only stop on change
            if self.comflag_main != v:
                self.comflag_main = v
                self.main.uc.emu_stop()
        elif 0x10 <= o < 0x20:
            i = (o - 0x10) >> 1
            if o & 1:
                self.comcmd[i] = (self.comcmd[i] & 0xFF00) | v
            else:
                self.comcmd[i] = (self.comcmd[i] & 0x00FF) | (v << 8)

    def _ga_sub_r(self, uc, off, size, ud):
        o = off & 0x1FF
        if size == 1:
            return self._ga_sub_r8(o)
        if size == 2:
            return (self._ga_sub_r8(o) << 8) | self._ga_sub_r8(o + 1)
        return (self._ga_sub_r(uc, off, 2, ud) << 16) | self._ga_sub_r(uc, off + 2, 2, ud)

    def _ga_sub_r8(self, o):
        if o == 0x00:
            return 0
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

    def _ga_sub_w(self, uc, off, size, value, ud):
        o = off & 0x1FF
        if size == 4:
            self._ga_sub_w(uc, off, 2, value >> 16, ud)
            self._ga_sub_w(uc, off + 2, 2, value & 0xFFFF, ud)
            return
        if size == 2:
            if 0x20 <= o < 0x30:
                self.comstat[(o - 0x20) >> 1] = value & 0xFFFF
                return
            if o == 0x0A:
                self.cdc_dma = value & 0xFFFF
                return
            if o == 0x04:
                self.cdc_mode = (self.cdc_mode & 0xC000) | (value & 0x0700)
                return
            self._ga_sub_w8(o, value >> 8)
            self._ga_sub_w8(o + 1, value & 0xFF)
            return
        self._ga_sub_w8(o, value)

    def _ga_sub_w8(self, o, v):
        if o == 0x03:
            if v & 0x04:
                self.log({'t': 'warn', 'msg': 'sub selects 1M mode %02x' % v})
            # 2M: RET=1 returns Word RAM to main (DMNA cleared) (HYPOTHESIS)
            if v & 0x01:
                self.memmode = (self.memmode & ~0x03) | 0x01
            self.memmode = (self.memmode & 0x03) | (v & 0x1C)
        elif o == 0x04:
            self.cdc_mode = (self.cdc_mode & 0xC000) | ((v & 7) << 8)
        elif o == 0x05:
            pass  # CDC register address
        elif o == 0x0A:
            self.cdc_dma = (self.cdc_dma & 0xFF) | (v << 8)
        elif o == 0x0B:
            self.cdc_dma = (self.cdc_dma & 0xFF00) | v
        elif o == 0x0F:
            if self.comflag_sub != v:
                self.comflag_sub = v
                self.sub.uc.emu_stop()
        elif 0x20 <= o < 0x30:
            i = (o - 0x20) >> 1
            if o & 1:
                self.comstat[i] = (self.comstat[i] & 0xFF00) | v
            else:
                self.comstat[i] = (self.comstat[i] & 0x00FF) | (v << 8)
        elif o == 0x31:
            self.timer = v
        elif o == 0x33:
            self.intmask = v
        elif o == 0x0C or o == 0x0D:
            self.stopwatch = 0
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
                self.cdc_mode &= ~0x4000
                self.cdc_mode |= 0x8000
            return w & 0xFF
        return w >> 8

    # ------------------------------------------------------------ YM2612 (log only)
    def _ym_r(self, uc, off, size, ud):
        return 0                                  # never busy, no timers

    def _ym_w(self, uc, off, size, value, ud):
        if size != 1:
            self.log({'t': 'warn', 'msg': 'ym word write'})
            return
        o = off & 3
        if o in (0, 2):
            self.ym_addr = (o >> 1, value)
        else:
            part, reg = getattr(self, 'ym_addr', (0, 0))
            self.ym_frame.append((part, reg, value))

    # ------------------------------------------------------------ PCM
    def _pcm_r(self, uc, off, size, ud):
        if size == 2:
            return (self.pcm.read(off) << 8) | self.pcm.read(off + 1)
        if size == 4:
            return (self._pcm_r(uc, off, 2, ud) << 16) | self._pcm_r(uc, off + 2, 2, ud)
        return self.pcm.read(off)

    def _pcm_w(self, uc, off, size, value, ud):
        if size == 4:
            self._pcm_w(uc, off, 2, value >> 16, ud)
            self._pcm_w(uc, off + 2, 2, value & 0xFFFF, ud)
        elif size == 2:
            self.pcm.write(off, value >> 8)
            self.pcm.write(off + 1, value & 0xFF)
        else:
            self.pcm.write(off, value)

    # ------------------------------------------------------------ VDP
    def _vdp_r(self, uc, off, size, ud):
        o = off & 0x1F
        if o < 4:
            v = self.vdp.data_read()
        elif o < 8:
            v = self.vdp.status()
        elif o < 0x10:
            v = self.vdp.hv()
        else:
            v = 0
        if size == 1:
            return (v >> 8) if not (off & 1) else (v & 0xFF)
        if size == 4:
            return (v << 16) | (self.vdp.data_read() if o < 4 else v)
        return v

    def _vdp_w(self, uc, off, size, value, ud):
        o = off & 0x1F
        if size == 4:
            self._vdp_w(uc, off, 2, value >> 16, ud)
            self._vdp_w(uc, off + 2, 2, value & 0xFFFF, ud)
            return
        if size == 1:
            value = (value << 8) | value
        if o < 4:
            self.vdp.data_write(value)
        elif o < 8:
            self.vdp.ctrl_write(value)
        # PSG writes (0x11) ignored

    # ------------------------------------------------------------ BIOS HLE
    def _main_bios(self, uc, addr, size, ud):
        m = self.main
        if addr == 0x364:
            self.log({'t': 'bios_main', 'fn': 'security'})
        elif addr == 0x368:
            self.main_vint = m.a(1)
            self.log({'t': 'bios_main', 'fn': 'set_vint', 'handler': self.main_vint})
        elif addr == 0x28C:
            pass
        else:
            self.log({'t': 'bios_main', 'fn': 'unknown', 'addr': addr})

    def _sub_bios(self, uc, addr, size, ud):
        s = self.sub
        if addr == 0x5F10:            # _WAITVSYNC
            s.rts()
            s.waiting = 'vsync'
            uc.emu_stop()
        elif addr == 0x5F16:          # _BURAM
            self._buram()
            s.rts()
        elif addr == 0x5F22:          # _CDBIOS
            self._cdbios()
            s.rts()
        else:
            self.log({'t': 'bios_sub', 'fn': 'unknown', 'addr': addr, 'd0': s.d(0)})
            s.rts()

    BURAM_FN = {0: 'BRMINIT', 1: 'BRMSTAT', 2: 'BRMSERCH', 3: 'BRMREAD', 4: 'BRMWRITE',
                5: 'BRMDEL', 6: 'BRMFORMAT', 7: 'BRMDIR', 8: 'BRMVERIFY'}

    def _buram(self):
        """Backup RAM HLE (HYPOTHESIS: function codes per Sega CD BIOS manual).
        Implemented as an in-memory dict of named files."""
        s = self.sub
        fn = s.d(0) & 0xFFFF
        name = self.BURAM_FN.get(fn, '?%d' % fn)
        if not hasattr(self, 'bram_files'):
            self.bram_files = {}
        a0, a1 = s.a(0), s.a(1)
        def fname(a):
            return bytes(s.uc.mem_read(a, 11)).split(b'\0')[0].decode('latin1')
        ok = True
        if fn == 0:      # BRMINIT: cs=error; d0=#blocks? d1=status; a1 -> display strings
            s.d(0, 0x7D); s.d(1, 0)
        elif fn == 1:    # BRMSTAT: d0 = free blocks, d1 = number of files
            s.d(0, 0x7D - sum((len(v) + 63) // 64 for v in self.bram_files.values())); s.d(1, len(self.bram_files))
        elif fn == 2:    # BRMSERCH a0 -> name; d0 = size in blocks, d1 = mode
            n = fname(a0)
            ok = n in self.bram_files
            if ok:
                s.d(0, (len(self.bram_files[n]) + 63) // 64); s.d(1, 0)
        elif fn == 3:    # BRMREAD a0 name, a1 dest
            n = fname(a0)
            ok = n in self.bram_files
            if ok:
                s.uc.mem_write(a1, self.bram_files[n]); s.d(0, (len(self.bram_files[n]) + 63) // 64); s.d(1, 0)
        elif fn == 4:    # BRMWRITE a0 -> 11-byte name, flag byte, block count word; a1 data
            n = fname(a0)
            blocks = s.r16(a0 + 12)
            self.bram_files[n] = bytes(s.uc.mem_read(a1, blocks * 64))
        elif fn == 5:
            self.bram_files.pop(fname(a0), None)
        elif fn == 8:    # BRMVERIFY: compare
            n = fname(a0)
            blocks = s.r16(a0 + 12)
            ok = self.bram_files.get(n) == bytes(s.uc.mem_read(a1, blocks * 64))
        self.log({'t': 'buram', 'fn': name, 'ok': ok})
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
        """CD BIOS HLE (function codes HYPOTHESIS from the Sega CD BIOS manual; semantics checked
        against the SP file engine at S:0x6E06–0x7078)."""
        s = self.sub
        fn = s.d(0) & 0xFFFF
        name = self.CDBIOS_FN.get(fn, '?%02x' % fn)
        carry = False
        if fn in (0x17, 0x20, 0x21):        # ROMREAD* : a0 -> [lba.l, count.l] (READ: a0 -> lba, until stop)
            lba = s.r32(s.a(0))
            cnt = s.r32(s.a(0) + 4) if fn != 0x17 else 0x7FFFFFFF
            self.cd_lba, self.cd_left, self.cd_ready = lba, cnt, None
            self.cd_first_dest = None
            n, off = self.disc.name_of(lba)
            self.log({'t': 'cd_read', 'lba': lba, 'count': cnt if cnt < 0x7FFFFFFF else -1, 'file': n, 'sector_in_file': off})
            self._cd_fill()
        elif fn == 0x8A:                    # CDCSTAT: cc if a sector is ready
            self._cd_fill()
            carry = self.cd_ready is None
        elif fn == 0x8B:                    # CDCREAD: prepare transfer of the ready sector
            self._cd_fill()
            if self.cd_ready is None:
                carry = True
            else:
                lba, data = self.cd_ready
                self.cd_cur = self.cd_ready
                self.cd_ready = None
                t = lba + 150
                mm, ss, ff = t // 4500, (t // 75) % 60, t % 75
                hdr = (bcd(mm) << 24) | (bcd(ss) << 16) | (bcd(ff) << 8) | 0x01
                s.d(0, hdr)
                s.d(1, 0)
                self._cdc_transfer(data)
        elif fn == 0x8C:                    # CDCTRN: copy 2048 bytes to a0, header to a1
            if self.cd_cur is None:
                carry = True
            else:
                lba, data = self.cd_cur
                a0, a1 = s.a(0), s.a(1)
                s.uc.mem_write(a0, data)
                if self.cd_first_dest is None:
                    self.cd_first_dest = a0
                    self.log({'t': 'cd_dest', 'mode': 'cdctrn', 'addr': a0})
                t = lba + 150
                s.uc.mem_write(a1, bytes([bcd(t // 4500), bcd((t // 75) % 60), bcd(t % 75), 1]))
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
            st = self.music_status
            s.uc.mem_write(blk, struct.pack('>HHIII', st, 0, 0, 0xFFFFFFFF, 0xFFFFFFFF))
            s.a(0, blk)
        elif fn in (0x11, 0x12, 0x13):      # MSCPLAY*: a0 -> track number word
            tr = s.r16(s.a(0))
            self.music = {'track': tr, 'mode': name, 'start_frame': self.frame}
            self.music_status = 0x0100
            self.log({'t': 'music', 'fn': name, 'track': tr})
        elif fn in (0x02, 0x03):
            self.music_status = 0x0000 if fn == 0x02 else 0x0300
            self.log({'t': 'music', 'fn': name})
        elif fn == 0x04:
            self.music_status = 0x0100 if self.music else 0
            self.log({'t': 'music', 'fn': name})
        else:
            self.log({'t': 'cdbios', 'fn': name, 'd1': s.d(1), 'a0': s.a(0)})
        s.set_carry(carry)

    def _cdc_transfer(self, data):
        """Destination device in CDC mode DD bits (HYPOTHESIS per Sega docs/GPGX cdc.c):
        2 main read, 3 sub read, 4 PCM DMA, 5 PRG-RAM DMA, 7 Word RAM DMA."""
        dd = (self.cdc_mode >> 8) & 7
        self.cdc_mode &= ~0xC000
        if self.cd_first_dest is None and dd in (4, 5, 7):
            self.cd_first_dest = self.cdc_dma
            self.log({'t': 'cd_dest', 'mode': 'dma', 'dd': dd, 'reg': self.cdc_dma})
        if dd == 3:
            self.cdc_mode |= 0x4000           # DSR: data ready for CDCTRN / host reads
            self.cdc_host = list(struct.unpack('>1024H', data))
        elif dd == 2:
            self.cdc_mode |= 0x4000
            self.cdc_host = list(struct.unpack('>1024H', data))
        elif dd == 5:
            a = (self.cdc_dma << 3) & 0x7FFF8
            ctypes.memmove(ctypes.addressof(self.prg) + a, data, 2048)
            self.cdc_dma = (self.cdc_dma + 256) & 0xFFFF
            self.cdc_mode |= 0x8000           # EDT
        elif dd == 7:
            a = (self.cdc_dma << 3) & 0x3FFF8
            ctypes.memmove(ctypes.addressof(self.wram) + a, data, 2048)
            self.cdc_dma = (self.cdc_dma + 256) & 0xFFFF
            self.cdc_mode |= 0x8000
        elif dd == 4:
            a = (self.cdc_dma << 2) & 0xFFC
            self.pcm.dma(a, data)
            self.cdc_dma = (self.cdc_dma + 512) & 0xFFFF
            self.cdc_mode |= 0x8000
        else:
            self.log({'t': 'warn', 'msg': 'cdc transfer dd=%d' % dd})

    # ------------------------------------------------------------ trace hooks
    def _seq_hook(self, uc, access, addr, size, value, ud):
        if value & 0xFF:
            q = bytes(uc.mem_read(0x9940, 0x80))
            self.log({'t': 'seq_start', 'queue': q.hex(), 'pc': self.sub.pc()})

    def _file_hook(self, uc, access, addr, size, value, ud):
        self.log({'t': 'script_file_w', 'value': value, 'pc': self.sub.pc()})

    # ------------------------------------------------------------ scheduler
    def step_frame(self):
        if self.inputs:
            n, bits = self.inputs[0]
            self.pad = bits
            self.inputs[0] = [n - 1, bits]
            if n - 1 <= 0:
                self.inputs.pop(0)
        else:
            self.pad = 0
        slices = 33
        lines_per = self.LINES / slices
        mper = self.MAIN_INSN_PER_FRAME // slices
        sper = self.SUB_INSN_PER_FRAME // slices
        CH = 128
        for k in range(slices):
            line = int(k * lines_per)
            self.vdp.line = line
            if line >= 224 and not self.vdp.vblank:
                self.vdp.vblank = True
                if self.vdp.reg[1] & 0x20 and self.main_vint:
                    self.main.irq(6, self.main_vint)
            if line < 224:
                self.vdp.vblank = False
            mleft, sleft = mper, sper
            dt = 1.0 / 60 / slices
            self._sub_irqs(dt)
            while mleft > 0 or sleft > 0:
                if mleft > 0:
                    self.main.run(min(CH, mleft)); mleft -= CH
                if self.ifl2:
                    self._sub_irqs(0)
                if sleft > 0:
                    n = min(CH * 2, sleft)
                    self.sub.run(n); sleft -= n
                    if self.timer_irq_due:
                        self.timer_irq_due -= 1
                        self.sub.irq(3, 0x5F82)
                if self.main.halted and self.sub.halted:
                    break
        self.pcm.advance(1.0 / 60)
        self.frame += 1
        sv = lambda a: self.sub.r16(a)
        rec = {'t': 'frame', 'f': self.frame, 'sp_state': sv(0x7840), 'vm_pc': sv(0x9800), 'script': sv(0x980C),
               'state': sv(0x9808), 'comcmd': list(self.comcmd), 'comstat': list(self.comstat),
               'flags': [self.comflag_main, self.comflag_sub], 'main_pc': self.main.pc(), 'sub_pc': self.sub.pc()}
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

    def _sub_irqs(self, dt):
        if self.ifl2 and self.intmask & 0x04:
            if self.sub.irq(2, 0x5E10):     # IFL2 stays pending until the sub accepts it
                self.ifl2 = False
                self.sub.waiting = None
        if self.intmask & 0x08 and self.timer and dt:
            self.timer_acc += dt
            period = (self.timer + 1) * 30.72e-6
            while self.timer_acc >= period:
                self.timer_acc -= period
                self.timer_irq_due += 1


class PCM:
    """RF5C164 register/RAM model (HYPOTHESIS: register map per Sega CD hardware manual):
    sub 0xFF0001.. odd bytes: 0 ENV, 1 PAN, 2 FDL, 3 FDH, 4 LSL, 5 LSH, 6 ST, 7 CTRL, 8 ON/OFF;
    current address read at 0x10..0x1F (sub 0xFF0021..), wave RAM window 0x1000-0x1FFF (sub 0xFF2001..)."""
    def __init__(self):
        self.ram = bytearray(0x10000)
        self.ch = [dict(env=0, pan=0, fd=0, ls=0, st=0, addr=0.0) for _ in range(8)]
        self.ctrl = 0
        self.onoff = 0xFF
        self.cur = 0
        self.bank = 0
        self.writes = 0

    def read(self, off):
        off &= 0x7FFF
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
        off &= 0x7FFF
        if off & 1 == 0:
            return
        r = off >> 1
        if r >= 0x1000:
            self.ram[(self.bank << 12) | (r & 0xFFF)] = v
            self.writes += 1
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
                was_off = self.onoff >> i & 1
                now_on = not (v >> i & 1)
                if was_off and now_on:
                    self.ch[i]['addr'] = float(self.ch[i]['st'] << 8)
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
            step = c['fd'] / 2048.0
            a = c['addr']
            # step sample-by-sample cheaply: jump, then check loop markers in the span
            end = a + step * n
            ia, ie = int(a), int(end)
            span = self.ram[ia & 0xFFFF: min(0x10000, (ie & 0xFFFF) + 1)] if ie - ia < 0x10000 and (ia & 0xFFFF) <= (ie & 0xFFFF) else None
            if span is not None and 0xFF in span:
                c['addr'] = float(c['ls'])
            else:
                c['addr'] = end % 0x10000
