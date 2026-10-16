"""Plane pictures (nametable + tile blobs of a scene): compose, edit, re-tile.  Used for the full-screen text pictures of DATA_D1 / D0."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from lib import *; from data import *

class Plane:
    def __init__(self, d, scene, plane_base=0xE000, tile_blob=0, name_blob=None, w=32, entries=None):
        self.d = d; self.scene = scene; self.w = w
        # entries: VRAM uploads {src, dst}; from the scene's VRAM list unless given (pictures loaded by tile channels)
        self.entries = entries if entries is not None else [u for k, v in d.chunks(scene) if k == 'vram' for u in v]
        self.name_u = [u for u in self.entries if u['dst'] == plane_base][0]
        self.name = bytearray(d.blob(self.name_u['src']))
        self.tile_us = [u for u in self.entries if u['dst'] != plane_base and u['dst'] < 0xC000]
        # tile memory: VRAM image of all non-nametable uploads
        self.vram = bytearray(0x10000)
        self.blob_of = {}
        for u in self.entries:
            if u is self.name_u: continue
            b = d.blob(u['src']); self.vram[u['dst']:u['dst'] + len(b)] = b
        self.orig_name = bytes(self.name)
        self.orig_vram = bytes(self.vram)
        self.tiles = tiles_from_bytes(bytes(self.vram))  # (2048, 8, 8)
        self.rows = len(self.name) // 2 // w
    def cell(self, cx, cy):
        v = (self.name[(cy * self.w + cx) * 2] << 8) | self.name[(cy * self.w + cx) * 2 + 1]
        return dict(tile=v & 0x7FF, hf=bool(v & 0x800), vf=bool(v & 0x1000), pal=(v >> 13) & 3, prio=bool(v & 0x8000))
    def index_image(self, rows=None):
        """(H, W) colour indices (0..15) and (H, W) palette line per pixel"""
        rows = rows or self.rows
        im = np.zeros((rows * 8, self.w * 8), dtype=np.uint8); pl = np.zeros_like(im)
        for cy in range(rows):
            for cx in range(self.w):
                c = self.cell(cx, cy); t = self.tiles[c['tile']].copy()
                if c['hf']: t = t[:, ::-1]
                if c['vf']: t = t[::-1, :]
                im[cy * 8:cy * 8 + 8, cx * 8:cx * 8 + 8] = t; pl[cy * 8:cy * 8 + 8, cx * 8:cx * 8 + 8] = c['pal']
        return im, pl

def final_palette(d, scene, cram_words=None):
    """colours of palette frame 0 of the scene's palette channel (the target of the fade)"""
    for k, v in d.chunks(scene):
        if k == 'pal' and v['frames']:
            return d.palette(v['frames'][0])
    return None


class PlaneEdit(Plane):
    """Plane picture with an editable index image; `commit` re-tiles only the cells that changed: their new contents reuse an identical tile of the
    picture, else a tile that no unchanged cell uses any more, else a new tile after the last one (the tile blob grows), and the nametable entries
    of those cells are rewritten.  Unchanged cells keep their tile ids, so the patch holds only the redrawn tiles."""
    def __init__(self, d, scene, **kw):
        super().__init__(d, scene, **kw)
        self.im, self.pl = self.index_image(self.rows if self.rows <= 32 else 32)
        self.im0 = self.im.copy()

    def cell_content(self, im, cx, cy):
        return im[cy * 8:cy * 8 + 8, cx * 8:cx * 8 + 8]

    def commit(self, P, file, tile_note='', name_note='', max_tiles=None, no_grow=False, grow_dst=None):
        """patch the tile blob(s) and the nametable.  Tile ids are VRAM addresses / 32, so a picture whose tiles live in several blobs works too;
        new tiles that fit no freed id are appended after the last tile blob (it grows)."""
        d = self.d
        fname = d.name if d.name.endswith('.BIN') else d.name + '.BIN'
        tblobs = sorted(self.tile_us, key=lambda u: u['dst'])
        orig = {u['src']: d.blob(u['src']) for u in tblobs}
        last = [u for u in tblobs if u['dst'] == grow_dst][0] if grow_dst is not None else tblobs[-1]
        end_id = (last['dst'] + len(orig[last['src']])) // 32
        rows = self.im.shape[0] // 8
        changed = [(cx, cy) for cy in range(rows) for cx in range(self.w)
                   if not (self.cell_content(self.im, cx, cy) == self.cell_content(self.im0, cx, cy)).all()]
        ch_set = set(changed)
        used_by_unchanged = set(); used_by_changed = set()
        for cy in range(self.rows):
            for cx in range(self.w):
                t = self.cell(cx, cy)['tile']
                (used_by_changed if (cx, cy) in ch_set else used_by_unchanged).add(t)
        free = sorted(used_by_changed - used_by_unchanged)
        vram0 = bytes(self.orig_vram)
        content_to_id = {}
        for t in sorted(used_by_unchanged):     # only tiles that stay: a freed tile may be overwritten below
            content_to_id.setdefault(vram0[t * 32:t * 32 + 32], t)
        new_tiles = {}; nxt = end_id; name = bytearray(self.name)
        for (cx, cy) in changed:
            data = tiles_to_bytes(self.cell_content(self.im, cx, cy)[None])
            tid = content_to_id.get(data)
            if tid is None:
                if free: tid = free.pop(0)
                else: tid = nxt; nxt += 1
                new_tiles[tid] = data; content_to_id[data] = tid
            e = (name[(cy * self.w + cx) * 2] << 8) | name[(cy * self.w + cx) * 2 + 1]
            e = (e & ~0x7FF & ~0x1800 & 0xFFFF) | tid     # flips off: the new tile is drawn as is
            name[(cy * self.w + cx) * 2] = e >> 8; name[(cy * self.w + cx) * 2 + 1] = e & 0xFF
        if max_tiles is not None: assert nxt <= max_tiles, 'out of tiles %d > %d' % (nxt, max_tiles)
        if no_grow: assert nxt == end_id, 'needs %d new tiles but this picture is loaded by a tile channel and cannot grow' % (nxt - end_id)
        for u in tblobs:
            b = bytearray(orig[u['src']])
            if u is last and nxt > end_id: b += bytes((nxt - end_id) * 32)
            lo = u['dst'] // 32
            for t, data in new_tiles.items():
                o = (t - lo) * 32
                if 0 <= o and o + 32 <= len(b): b[o:o + 32] = data
            P.blob(fname, u['src'], bytes(b), tile_note)
        P.blob(fname, self.name_u['src'], bytes(name), name_note)
        return dict(changed=len(changed), new=nxt - end_id, reused_free=len(free))
