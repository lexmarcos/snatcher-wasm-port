"""Helpers to inspect and redraw the pictures of the Snatcher scenes (docs/i18n-graphics.md).
Only dev tooling: reads the user's decoded game data from work/, never stores original art in the repository."""
import numpy as np
from PIL import Image

def md_rgb(w):
    return (((w >> 1) & 7) * 36, ((w >> 5) & 7) * 36, ((w >> 9) & 7) * 36)

def load_cram(path):
    b = open(path, 'rb').read()
    w = [int.from_bytes(b[i:i + 2], 'big') for i in range(0, len(b), 2)]
    return w[:64], w[64:128]

def pal_rgb(words):
    return [md_rgb(w) for w in words]

def tiles_from_bytes(data):
    """4 bpp 8x8 tiles -> array (n, 8, 8) of colour indices."""
    a = np.frombuffer(data, dtype=np.uint8)
    n = len(a) // 32
    a = a[:n * 32].reshape(n, 8, 4)
    out = np.zeros((n, 8, 8), dtype=np.uint8)
    out[:, :, 0::2] = a >> 4
    out[:, :, 1::2] = a & 15
    return out

def tiles_to_bytes(t):
    t = np.asarray(t, dtype=np.uint8)
    n = t.shape[0]
    out = (t[:, :, 0::2] << 4) | t[:, :, 1::2]
    return out.reshape(n * 32).tobytes()

def sheet(tiles, pal, cols=32, scale=1, first=0, pal_base=0):
    n = len(tiles)
    rows = (n + cols - 1) // cols
    img = np.zeros((rows * 8, cols * 8, 3), dtype=np.uint8)
    for i, t in enumerate(tiles):
        r, c = divmod(i, cols)
        for y in range(8):
            for x in range(8):
                img[r * 8 + y, c * 8 + x] = pal[pal_base + t[y, x]]
    im = Image.fromarray(img)
    if scale != 1:
        im = im.resize((im.width * scale, im.height * scale), Image.NEAREST)
    return im

def pal_slot(words, line):
    return pal_rgb(words[line * 16:line * 16 + 16])

def piece_tiles(size, attr):
    """tile numbers of a sprite piece as a (h, w) grid (column-major numbering, flips applied by the caller)"""
    w = ((size >> 2) & 3) + 1; h = (size & 3) + 1
    t0 = attr & 0x7FF
    return [[t0 + c * h + r for c in range(w)] for r in range(h)], w, h

def render_pieces(pieces, tiles, pal, bg=(0, 40, 0), pal_override=None, minx=None, miny=None, maxx=None, maxy=None):
    """pieces: list of (y, size, attr, x); tiles: (n,8,8) indices of the VRAM from tile 0; pal: 4 lines of 16 rgb (or list of lines)
    returns a PIL image cropped to the pieces' bounding box and (x0, y0)"""
    boxes = []
    for (y, size, attr, x) in pieces:
        _, w, h = piece_tiles(size, attr)
        boxes.append((x, y, x + 8 * w, y + 8 * h))
    x0 = min(b[0] for b in boxes) if minx is None else minx; y0 = min(b[1] for b in boxes) if miny is None else miny
    x1 = max(b[2] for b in boxes) if maxx is None else maxx; y1 = max(b[3] for b in boxes) if maxy is None else maxy
    img = np.zeros((y1 - y0, x1 - x0, 3), dtype=np.uint8); img[:] = bg
    for (y, size, attr, x) in pieces:
        grid, w, h = piece_tiles(size, attr)
        line = (attr >> 13) & 3
        hf = attr & 0x800; vf = attr & 0x1000
        for r in range(h):
            for c in range(w):
                tn = grid[r][c]
                t = tiles[tn] if tn < len(tiles) else np.zeros((8, 8), dtype=np.uint8)
                cc = (w - 1 - c) if hf else c; rr = (h - 1 - r) if vf else r
                for py in range(8):
                    for px in range(8):
                        sx = px if not hf else 7 - px; sy = py if not vf else 7 - py
                        v = t[sy, sx]
                        if v:
                            img[y - y0 + rr * 8 + py, x - x0 + cc * 8 + px] = pal[line][v]
    return Image.fromarray(img), (x0, y0)
