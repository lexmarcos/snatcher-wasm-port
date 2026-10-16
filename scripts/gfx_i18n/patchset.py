"""Collects the pt-BR picture patches and writes i18n/pt-BR/gfx.tsv (docs/i18n-graphics.md).
Only the changed bytes are stored (decoded-blob runs / raw DATA file runs); the originals are read from assets/raw at build time."""
import os, sys
sys.path.insert(0, os.path.dirname(__file__))
from data import Data, fnv64, BASE

class PatchSet:
    def __init__(self):
        self.blobs = []   # (file, addr, hash, len, [(off, bytes)], note)
        self.files = []   # (file, off, orig, new, note)
        self._d = {}
    def data(self, name):
        if name not in self._d: self._d[name] = Data(name)
        return self._d[name]
    def blob(self, file, addr, new, note='', gap=4):
        orig = self.data(file).blob(addr)
        assert len(orig) == len(new), 'blob patches keep the size'
        runs = []; i = 0; n = len(orig)
        while i < n:
            if orig[i] != new[i]:
                j = i
                while j < n:
                    # extend over equal stretches shorter than `gap`
                    k = j
                    while k < n and orig[k] == new[k]: k += 1
                    if k >= n or k - j >= gap and orig[j] == new[j]: break
                    if orig[j] != new[j]: j += 1
                    else: j = k
                end = j
                while end > i and orig[end - 1] == new[end - 1]: end -= 1
                runs.append((i, bytes(new[i:end]))); i = max(j, end)
            else: i += 1
        if runs: self.blobs.append((file, addr, fnv64(orig), len(orig), runs, note))
        return runs
    def raw(self, file, off, new, note=''):
        orig = bytes(self.data(file).b[off:off + len(new)])
        if orig != bytes(new): self.files.append((file, off, orig, bytes(new), note))
    def write(self, path, header=''):
        with open(path, 'w') as f:
            f.write('# pt-BR pictures: runs of changed bytes (docs/i18n-graphics.md). Do not edit by hand: scripts/gfx_i18n/make_patch.py\n')
            f.write('# blob <file> <sub address> <fnv64 of the decoded original> <decoded length> <offset> <new bytes hex>\n')
            f.write('# file <file> <offset> <original bytes hex> <new bytes hex>\n')
            for (file, addr, h, ln, runs, note) in sorted(self.blobs, key=lambda b: (b[0], b[1])):
                if note: f.write('# %s\n' % note)
                for off, b in runs: f.write('blob\t%s\t0x%x\t%016x\t%d\t%d\t%s\n' % (file, addr, h, ln, off, b.hex()))
            for (file, off, orig, new, note) in sorted(self.files, key=lambda b: (b[0], b[1])):
                if note: f.write('# %s\n' % note)
                f.write('file\t%s\t%d\t%s\t%s\n' % (file, off, orig.hex(), new.hex()))
