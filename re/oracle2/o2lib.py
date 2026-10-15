"""oracle2: Genesis Plus GX (unmodified core) + the real Sega CD BIOS, driven frame by frame from Python (ctypes).
Dev-only test oracle. Machine2 mirrors the part of re/oracle/machine.py's Machine that the fid_*/scenario tools use
(step_frame, frame, inputs, sub/main memory accessors, frame_image, trace.jsonl in the same format)."""
import ctypes, json, os, struct, sys, tempfile, time
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
LIB = os.path.join(HERE, 'build', 'liboracle2.so')
CUE = os.path.join(ROOT, 'work', 'snatcher.cue')

# snatcher pad bits (engine::pad) -> libretro joypad mask (Genesis A = libretro Y, B = B, C = libretro A)
_SN2RA = [(1, 4), (2, 5), (4, 6), (8, 7), (16, 0), (32, 8), (64, 1), (128, 3)]
def pad_mask(bits):
    m = 0
    for sb, rb in _SN2RA:
        if bits & sb:
            m |= 1 << rb
    return m

RETRO_DEVICE_JOYPAD = 1
RETRO_DEVICE_LIGHTGUN = 4
RETRO_DEVICE_JUSTIFIERS = (2 << 8) | 4

_lib = None
def lib():
    global _lib
    if _lib is None:
        L = ctypes.CDLL(LIB)
        L.o2_frame.restype = ctypes.POINTER(ctypes.c_uint8)
        for n in ('o2_main_pc', 'o2_sub_pc', 'o2_main_reg', 'o2_sub_reg', 'o2_asic_reg', 'o2_main_sr', 'o2_sub_sr'):
            getattr(L, n).restype = ctypes.c_uint
        L.o2_sample_rate.restype = ctypes.c_double
        L.o2_state_size.restype = ctypes.c_size_t
        L.o2_audio_take.restype = ctypes.c_size_t
        L.o2_audio_take.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        _lib = L
    return _lib


class Mem:
    """Read/write view of one CPU's memory (canonical addresses, big-endian) with the Cpu API of the HLE oracle."""
    def __init__(self, m, kind):
        self.m, self.kind, self.name = m, kind, kind
        self.halted = False
        self.waiting = False
    def _buf(self):
        return self.m._prg() if self.kind == 'sub' else self.m._wram()
    def _base(self, addr):
        return addr if self.kind == 'sub' else addr - 0xFF0000
    def read(self, addr, n):
        b = self._buf(); a = self._base(addr)
        return bytes(b[a:a + n])
    def r8(self, addr): return self._buf()[self._base(addr)]
    def r16(self, addr):
        b = self._buf(); a = self._base(addr); return (b[a] << 8) | b[a + 1]
    def r32(self, addr):
        b = self._buf(); a = self._base(addr); return int.from_bytes(bytes(b[a:a + 4]), 'big')
    def write(self, addr, data):
        data = bytes(data); a = self._base(addr)
        if self.kind == 'sub': self.m.L.o2_write_prg(a, data, len(data))
        else: self.m.L.o2_write_main_ram(a, data, len(data))
        self.m._invalidate()
    def w8(self, addr, v): self.write(addr, bytes([v & 0xFF]))
    def w16(self, addr, v): self.write(addr, struct.pack('>H', v & 0xFFFF))
    def w32(self, addr, v): self.write(addr, struct.pack('>I', v & 0xFFFFFFFF))
    def pc(self): return (self.m.L.o2_sub_pc() if self.kind == 'sub' else self.m.L.o2_main_pc()) & 0xFFFFFF
    # M68K_REG_D0.. = 0, A0.. = 8 in Musashi's enum
    def d(self, n): return (self.m.L.o2_sub_reg if self.kind == 'sub' else self.m.L.o2_main_reg)(n)
    def a(self, n): return (self.m.L.o2_sub_reg if self.kind == 'sub' else self.m.L.o2_main_reg)(8 + n)


class Vdp2:
    def __init__(self, m): self.m = m
    def frame_image(self): return self.m.frame_rgb()
    def vram(self): return self.m.vram()
    def cram(self): return self.m.cram()
    def vsram(self): return self.m.vsram()
    def regs(self): return self.m.vdp_regs()


class Machine2:
    def __init__(self, trace_path=None, verbose=False, bios=None, ym='nuked (ym2612)', cd_latency=None, opts=None,
                 content=CUE, savedir=None, gun=False):
        self.L = L = lib()
        self.trace = open(trace_path, 'w') if trace_path else None
        self.frame = 0
        self.inputs = []
        self.pad = 0
        self.gun = None             # {'x','y','trig','aux'} in screen pixels (like Machine.gun), only with gun=True
        self.events = []
        self.main = Mem(self, 'main'); self.sub = Mem(self, 'sub'); self.vdp = Vdp2(self)
        self._cache = {}
        self.ym_frame = []
        self.audio = []             # list of int16 stereo arrays when record_audio
        self.record_audio = False
        self.last_cd_event = None
        self.sysdir = os.path.join(HERE, 'sys')
        # a fresh save dir per run: the core persists internal backup RAM (scd_U.brm) there, a leftover file would break determinism
        self.savedir = savedir or tempfile.mkdtemp(prefix='o2save_', dir=os.environ.get('TMPDIR'))
        os.makedirs(self.savedir, exist_ok=True)
        if ym: L.o2_set_option(b'genesis_plus_gx_ym2612', ym.encode())
        if cd_latency is not None: L.o2_set_option(b'genesis_plus_gx_cd_latency', cd_latency.encode())
        L.o2_set_option(b'genesis_plus_gx_audio_filter', b'disabled')
        L.o2_set_option(b'genesis_plus_gx_cd_precache', b'disabled')
        L.o2_set_option(b'genesis_plus_gx_cart_size', b'disabled')   # no RAM cartridge (like the HLE oracle; otherwise the game asks which RAM to use)
        for k, v in (opts or {}).items(): L.o2_set_option(k.encode(), v.encode())
        r = L.o2_init(self.sysdir.encode(), self.savedir.encode(), content.encode(), 1 if verbose else 0)
        if r != 0: raise RuntimeError('o2_init failed (BIOS in re/oracle2/sys/bios_CD_U.bin?)')
        self.gunmode = gun
        if gun:
            L.o2_set_port_device(0, RETRO_DEVICE_JOYPAD)
            L.o2_set_port_device(1, RETRO_DEVICE_JUSTIFIERS)
        self.srate = L.o2_sample_rate()
        self._last_script = None
        self.shot_w = self.shot_h = 0

    # --- memory snapshots (cached per frame) ---
    def _invalidate(self): self._cache.clear()
    def _prg(self):
        b = self._cache.get('prg')
        if b is None:
            b = (ctypes.c_uint8 * 0x80000)(); self.L.o2_read_prg_ram(b); self._cache['prg'] = b
        return b
    def _wram(self):
        b = self._cache.get('wram')
        if b is None:
            b = (ctypes.c_uint8 * 0x10000)(); self.L.o2_read_main_ram(b); self._cache['wram'] = b
        return b
    def word_ram(self):
        b = (ctypes.c_uint8 * 0x40000)(); self.L.o2_read_word_ram_2m(b); return bytes(b)
    def vram(self):
        b = (ctypes.c_uint8 * 0x10000)(); self.L.o2_read_vram(b); return bytes(b)
    def cram(self):
        b = (ctypes.c_uint16 * 64)(); self.L.o2_read_cram(b); return list(b)
    def vsram(self):
        b = (ctypes.c_uint16 * 40)(); self.L.o2_read_vsram(b); return list(b)
    def vdp_regs(self):
        b = (ctypes.c_uint8 * 32)(); self.L.o2_read_vdp_regs(b); return list(b)
    def asic(self, off):
        """gate array register at byte offset off (0xFF8000 + off in the sub map, 0xA12000 + off in the main map), big-endian word"""
        return self.L.o2_asic_reg(off >> 1) & 0xFFFF
    def frame_rgb(self):
        w = ctypes.c_int(); h = ctypes.c_int()
        p = self.L.o2_frame(ctypes.byref(w), ctypes.byref(h))
        if not p: return np.zeros((224, 320, 3), np.uint8)
        return np.ctypeslib.as_array(p, shape=(h.value, w.value, 3)).copy()

    def log(self, e):
        self.events.append(e)

    def set_pad(self, bits):
        self.pad = bits
        self.L.o2_set_pad(0, pad_mask(bits))

    def _apply_gun(self):
        g = self.gun
        if not self.gunmode: return
        if g is None:
            self.L.o2_set_gun(1, 0, 0, 1, 0, 0, 0, 0); return
        w, h = (self.shot_w or 256), (self.shot_h or 224)
        # core: analog = ((v + 0x7fff) * viewport.w) / 0xfffe  -> v = x * 0xfffe / w - 0x7fff (+ half step)
        vx = int((g['x'] + 0.5) * 0xfffe / w) - 0x7fff
        vy = int((g['y'] + 0.5) * 0xfffe / h) - 0x7fff
        self.L.o2_set_gun(1, vx, vy, 0, int(bool(g.get('trig'))), int(bool(g.get('aux'))), 0, 0)

    def step_frame(self, capture=False):
        while self.inputs and self.inputs[0][0] <= 0:
            self.inputs.pop(0)
        if self.inputs:
            n, bits = self.inputs[0]
            self.inputs[0] = [n - 1, bits]
            if n - 1 <= 0: self.inputs.pop(0)
        else:
            bits = 0
        self.set_pad(bits)
        self._apply_gun()
        self.L.o2_run()
        self._invalidate()
        self.frame += 1
        L = self.L
        n = L.o2_ym_count()
        ym = []
        c = ctypes.c_uint32(); a = ctypes.c_uint8(); d = ctypes.c_uint8()
        part = 0; reg = 0
        for i in range(n):
            L.o2_ym_get(i, ctypes.byref(c), ctypes.byref(a), ctypes.byref(d))
            ad, dv = a.value, d.value
            # same format as the HLE oracle: [part, reg, val] for data writes (addr 1 / 3); address writes (0 / 2) select part+reg
            if ad & 1 == 0: part, reg = ad >> 1, dv
            else: ym.append([part, reg, dv])
        if self.record_audio:
            m = L.o2_audio_take(None, 0)
            buf = np.zeros((m, 2), np.int16)
            L.o2_audio_take(buf.ctypes.data, m)
            self.audio.append(buf)
        else:
            m = L.o2_audio_take(None, 0)
            L.o2_audio_take(None, 0) if m == 0 else self._drop_audio(m)
        if capture:
            self.shot_h, self.shot_w = self.frame_rgb().shape[:2]
        w = lambda a: (self._prg()[a] << 8) | self._prg()[a + 1]
        rec = {'t': 'frame', 'f': self.frame, 'sp_state': w(0x7840), 'state': w(0x9808), 'vm_pc': w(0x9800),
               'script': w(0x980C),
               'comcmd': [self.asic(0x10 + 2 * i) for i in range(8)], 'comstat': [self.asic(0x20 + 2 * i) for i in range(8)],
               'flags': [self.asic(0x0E) >> 8, self.asic(0x0E) & 0xFF], 'main_pc': self.main.pc(), 'sub_pc': self.sub.pc()}
        if ym: rec['ym'] = ym
        if rec['script'] != self._last_script:
            self.log({'t': 'script_file', 'id': rec['script']}); self._last_script = rec['script']
        if self.trace:
            for e in self.events: self.trace.write(json.dumps(e) + '\n')
            self.trace.write(json.dumps(rec) + '\n'); self.trace.flush()
        self.events = []
        return rec

    def _drop_audio(self, m):
        buf = np.zeros((m, 2), np.int16)
        self.L.o2_audio_take(buf.ctypes.data, m)

    # --- save states (core serialize) ---
    def save_state(self, path):
        n = self.L.o2_state_size()
        b = ctypes.create_string_buffer(n)
        if not self.L.o2_state_save(b, n): raise RuntimeError('serialize failed')
        with open(path, 'wb') as f:
            f.write(struct.pack('<III', 0x4F325354, self.frame, self._last_script or 0)); f.write(b.raw)
    def load_state(self, path):
        d = open(path, 'rb').read()
        magic, fr, sc = struct.unpack('<III', d[:12])
        assert magic == 0x4F325354
        if not self.L.o2_state_load(d[12:], len(d) - 12): raise RuntimeError('unserialize failed')
        self.frame = fr; self._invalidate()
        self._last_script = sc
