"""oracle2: Genesis Plus GX (unmodified core) + the real Sega CD BIOS, driven frame by frame from Python (ctypes).
Dev-only test oracle. Machine2 mirrors the part of re/oracle/machine.py's Machine that the fid_*/scenario tools use
(step_frame, frame, inputs, sub/main memory accessors, frame_image, trace.jsonl in the same format)."""
import ctypes, json, os, struct, sys, tempfile, time
import numpy as np

SEQ_RUN = 0xFFC4                                     # S: sequencer entry (seq_run)
SEQ_CLEAR = (0x608C, 0xD522, 0xD540, 0xDED0, 0x10046)  # S: the `clr.b seq_busy` sites
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


def _level_tables():
    # core output (RGB565 from 3-bit levels L, see vdp_render.c MAKE_PIXEL with r = 2L): back to L, then to the HLE renderer's L * 36
    def inv(vals):
        t = np.zeros(256, np.uint8)
        for v in range(256):
            t[v] = min(range(8), key=lambda L: abs(vals[L] - v))
        return t
    r = [((((2 * L) << 1) | ((2 * L) >> 3)) * 255) // 31 for L in range(8)]
    g = [((((2 * L) << 2) | ((2 * L) >> 2)) * 255) // 63 for L in range(8)]
    return inv(r), inv(g), inv(r)
_LT = None
def normalize_levels(img):
    """core RGB (levels 0..238) -> the HLE oracle / engine palette (L*36, white = 252) so that PNGs compare 1:1."""
    global _LT
    if _LT is None: _LT = _level_tables()
    out = np.empty_like(img)
    for c in range(3): out[..., c] = _LT[c][img[..., c]] * 36
    return out


class Vdp2:
    def __init__(self, m): self.m = m
    def frame_image(self): return normalize_levels(self.m.frame_rgb())
    def vram(self): return self.m.vram()
    def cram(self): return self.m.cram()
    def vsram(self): return self.m.vsram()
    def regs(self): return self.m.vdp_regs()


class Machine2:
    def __init__(self, trace_path=None, verbose=False, bios=None, ym='nuked (ym2612)', cd_latency=None, opts=None,
                 content=CUE, savedir=None, gun=False, port2=None, seq_trace=True):
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
        # The core detects "SNATCHER" in the disc header and forces a Konami Justifier in port 2 (core/loadrom.c, input.system[1]), so the
        # game sees a gun at power-up (A12013 = 1) without any option; `gun` is kept for compatibility. Aiming = m.gun = {'x','y','trig','aux'}.
        self.gunmode = True
        if port2 == 'pad':        # plain control pad in port 2 (the game then finds no gun at power-up)
            L.o2_set_port_device(1, RETRO_DEVICE_JOYPAD); self.gunmode = False
        elif port2 == 'justifier':
            L.o2_set_port_device(1, RETRO_DEVICE_JUSTIFIERS)
        self.srate = L.o2_sample_rate()
        self._last_script = None
        self._music_track = 0
        self.hits = []
        self._run = None
        self._seq_armed = True
        if seq_trace:
            self.add_snap_hook('sub', SEQ_RUN)
            for _pc in SEQ_CLEAR: self.add_hook('sub', _pc)
        self.cd_state = (0, 0, 0, 0)
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
        e.setdefault('f', self.frame)      # the HLE oracle stamps every event with the frame index
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
        self.L.o2_set_gun(1, vx, vy, 0, int(bool(g.get('trig'))), 0, 0, int(bool(g.get('aux'))))   # Justifier: D0 = trigger (INPUT_A), D1 = second button (START)

    def add_hook(self, cpu, pc):
        """PC hook (cpu 'sub'/'main'): from then on the CPU is stepped instruction by instruction (slower) and every execution of `pc` is
        reported in self.hits after each step_frame as (cpu, pc, regs) with regs = dict d0..d3 a0..a2 a7."""
        self.L.o2_hook_set(1 if cpu == 'sub' else 0, pc, 1)
    def add_snap_hook(self, cpu, pc):
        """hook that also keeps 0x240 bytes of sub PRG-RAM at S:$9940 (the sequencer queue and its index/busy words) as seen when the instruction is reached; hits get regs['snap'] (0x240 bytes from S:$9940: queue at +0, seq_index $9B40 at +0x200, seq_busy $9B44 at +0x204)."""
        self.L.o2_hook_snap(1 if cpu == 'sub' else 0, pc, 1); self.add_hook(cpu, pc)
    def del_hook(self, cpu, pc):
        self.L.o2_hook_set(1 if cpu == 'sub' else 0, pc, 0)

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
        self.hits = []
        nh = L.o2_hook_count()
        if nh:
            hc = ctypes.c_int(); hp = ctypes.c_uint(); hr = (ctypes.c_uint * 8)(); snapbuf = (ctypes.c_uint8 * 0x240)()
            for i in range(nh):
                L.o2_hook_get(i, ctypes.byref(hc), ctypes.byref(hp), hr)
                regs = dict(zip(('d0', 'd1', 'd2', 'd3', 'a0', 'a1', 'a2', 'a7'), list(hr)))
                if L.o2_hook_snapshot(i, snapbuf): regs['snap'] = bytes(snapbuf)
                self.hits.append(('sub' if hc.value else 'main', hp.value, regs))
        self._cd_events(ym)
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
        # sequencer queue start (exact, like the HLE oracle's rising edge of S:$9B44): the first run of seq_run (S:0xFFC4) after the busy flag was cleared
        # (hooks on the `clr.b seq_busy` sites) -> the queue as it is at that instant
        for _, pc, r in self.hits:
            if pc in SEQ_CLEAR: self._seq_armed = True
            elif pc == SEQ_RUN and 'snap' in r:
                sn = r['snap']
                idx = (sn[0x200] << 8) | sn[0x201]                      # seq_index $9B40
                cmd = (sn[idx] << 8) | sn[idx + 1] if idx < 0x80 else 1
                if self._seq_armed:
                    self._seq_armed = False
                    self.log({'t': 'seq_start', 'queue': sn[:0x80].rstrip(b'\0').hex(), 'sub_pc': 0})
                if cmd == 0: self._seq_armed = True                     # terminator reached: the next run is a new queue
        if rec['script'] != self._last_script:
            self.log({'t': 'script_file', 'id': rec['script']}); self._last_script = rec['script']
        if self.trace:
            for e in self.events: self.trace.write(json.dumps(e) + '\n')
            self.trace.write(json.dumps(rec) + '\n'); self.trace.flush()
        self.events = []
        return rec

    def _flush_run(self):
        r = self._run
        if r: self.log({'t': 'cd_sectors', 'lba': r[0], 'count': r[1], 'f': r[2], 'f_end': r[3]}); self._run = None

    def _cd_events(self, ym):
        """CD drive events of this frame: CDD commands (cmd 3 play / 4 seek carry the target MSF -> cd_read), data sectors decoded by the
        CDC (runs -> cd_sectors), CD-DA start/stop (music) from the drive state."""
        L = self.L
        n = L.o2_cdev_count()
        kind = ctypes.c_int(); v = (ctypes.c_int * 8)()
        for i in range(n):
            L.o2_cdev_get(i, ctypes.byref(kind), v)
            if kind.value == 2:
                lba, r = v[0], self._run
                if r and lba == r[0] + r[1] and self.frame - r[3] <= 3: r[1] += 1; r[3] = self.frame
                else: self._flush_run(); self._run = [lba, 1, self.frame, self.frame]
            else:
                cmd = v[0]
                if cmd in (3, 4):
                    self._flush_run()
                    # args are BCD digit pairs: reg 0x44 / 0x46 / 0x48 = M / S / F (high byte tens, low byte units)
                    d = lambda w: ((w >> 8) & 0xF) * 10 + (w & 0xF)
                    lba = (d(v[1]) * 60 + d(v[2])) * 75 + d(v[3]) - 150
                    self.log({'t': 'cd_read', 'fn': 'CDD_PLAY' if cmd == 3 else 'CDD_SEEK', 'lba': lba, 'count': -1, 'file': None, 'f': self.frame,
                              'from_lba': v[7]})
                elif cmd != 2:      # cmd 2 = TOC / status requests, polled constantly
                    self.log({'t': 'cdd', 'cmd': cmd, 'f': self.frame})
        if self._run and self.frame - self._run[3] > 3: self._flush_run()
        st = ctypes.c_int(); ix = ctypes.c_int(); lb = ctypes.c_int(); la = ctypes.c_int(); au = ctypes.c_int()
        L.o2_cdd_state(ctypes.byref(st), ctypes.byref(ix), ctypes.byref(lb), ctypes.byref(la), ctypes.byref(au))
        playing_audio = au.value and st.value in (1, 3)       # CD_PLAY / CD_SCAN on an audio track
        key = (ix.value + 1) if playing_audio else 0
        if key != self._music_track:
            if key: self.log({'t': 'music', 'fn': 'MSCPLAY', 'track': key, 'f': self.frame})
            else: self.log({'t': 'music', 'fn': 'MSCSTOP', 'f': self.frame})
            self._music_track = key
        self.cd_state = (st.value, ix.value, lb.value, la.value)

    def boot(self, start_at=300, max_frames=3000):
        """Run the real BIOS from power-on: press START on its 'Press the START BUTTON' screen and return t0 = the first frame in which
        the game's main program (IP in Work RAM at M:0xFF0584) runs. Scenario frame N of the HLE oracle ~ t0 + N."""
        press = start_at
        for i in range(max_frames):
            self.inputs = [[3, 128]] if self.frame + 1 == press else []
            r = self.step_frame()
            if r['main_pc'] >= 0xFF0000:
                self.log({'t': 'boot', 'f': self.frame}); self.t0 = self.frame
                return self.frame
        raise RuntimeError('game did not start')

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
