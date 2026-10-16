"""Plane pictures (nametable + tile blobs of a scene): compose, edit, re-tile.  Used for the full-screen text pictures of DATA_D1 / D0."""
import sys, os
sys.path.insert(0, os.path.dirname(__file__))
from lib import *; from data import *

class Plane:
    def __init__(self, d, scene, plane_base=0xE000, tile_blob=0, name_blob=None, w=32):
        self.d = d; self.scene = scene; self.w = w
        self.entries = [u for k, v in d.chunks(scene) if k == 'vram' for u in v]
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
