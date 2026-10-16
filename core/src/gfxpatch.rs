//! pt-BR picture substitution (docs/i18n-graphics.md).
//!
//! The text that is drawn inside the game's pictures lives in LZKN1 blobs (tile sheets, nametables). A [`GfxPatch`] holds, per blob, the
//! bytes that change in the *decoded* blob. The blob is recognised by the FNV-1a 64 hash and the length of its decoded original, so the
//! patch is independent of file names and addresses, never contains the original picture (only the redrawn bytes) and does nothing when
//! the blob differs (a missing or foreign patch leaves the original picture). Same length in, same length out: nothing else in the
//! engine (VRAM lists, DMA sizes, tile channels) changes.
use std::collections::HashMap;

/// FNV-1a, 64 bit.
pub fn fnv64(b: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &x in b { h ^= x as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}

/// One blob: the decoded original has `len` bytes and hash `hash`; `runs` are (offset, new bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobPatch {
    pub hash: u64,
    pub len: u32,
    pub runs: Vec<(u32, Vec<u8>)>,
}

/// A change in the raw (not compressed) bytes of a DATA file, for the few things that are not tile data: sprite frame descriptions, scripts.
/// `orig` is what the bytes must be for the change to apply (a different disc image is left alone); `new` has the same length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRun { pub off: u32, pub orig: Vec<u8>, pub new: Vec<u8> }

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GfxPatch {
    pub blobs: HashMap<(u64, u32), Vec<(u32, Vec<u8>)>>,
    /// raw patches by DATA file name ("DATA_D2.BIN")
    pub files: HashMap<String, Vec<FileRun>>,
}

const MAGIC: &[u8; 4] = b"GFXP";
/// Largest decoded blob after a growing patch (one 0x2000-byte VRAM block, the largest upload the game makes).
pub const MAX_GROWN: usize = 0x2000;

impl GfxPatch {
    pub fn is_empty(&self) -> bool { self.blobs.is_empty() && self.files.is_empty() }

    pub fn insert(&mut self, p: BlobPatch) { self.blobs.insert((p.hash, p.len), p.runs); }

    /// Apply the patch of this blob (when there is one) to the decoded output; returns true when something changed.
    pub fn apply(&self, out: &mut Vec<u8>) -> bool {
        if self.blobs.is_empty() { return false; }
        let Some(runs) = self.blobs.get(&(fnv64(out), out.len() as u32)) else { return false };
        for (off, b) in runs {
            let o = *off as usize;
            // a run that ends beyond the decoded original grows the blob (more tiles in free VRAM, never more than MAX_GROWN bytes)
            if o + b.len() > out.len() && o + b.len() <= MAX_GROWN { out.resize(o + b.len(), 0); }
            if o + b.len() <= out.len() { out[o..o + b.len()].copy_from_slice(b); }
        }
        true
    }

    /// Apply the raw patches of a DATA file (mapped at 0x28000) when its bytes are the expected ones; returns the number of runs applied.
    pub fn apply_file(&self, name: &str, data: &mut Vec<u8>) -> usize {
        let Some(runs) = self.files.get(name) else { return 0 };
        let mut n = 0;
        for r in runs {
            let o = r.off as usize;
            if r.orig.is_empty() {
                // append: new data (sprite frames) placed right after the end of the file as it is now
                if o == data.len() && !r.new.is_empty() { data.extend_from_slice(&r.new); n += 1; }
            } else if o + r.orig.len() <= data.len() && r.orig.len() == r.new.len() && data[o..o + r.orig.len()] == r.orig[..] {
                data[o..o + r.new.len()].copy_from_slice(&r.new); n += 1;
            }
        }
        n
    }

    /// Layout (big endian): "GFXP", u16 n, n x { u64 hash, u32 len, u16 nruns, nruns x { u32 off, u16 len, bytes } }.
    pub fn build(&self) -> Vec<u8> {
        let mut o = MAGIC.to_vec();
        let mut keys: Vec<_> = self.blobs.keys().copied().collect(); keys.sort();
        o.extend_from_slice(&(keys.len() as u16).to_be_bytes());
        for k in keys {
            let runs = &self.blobs[&k];
            o.extend_from_slice(&k.0.to_be_bytes()); o.extend_from_slice(&k.1.to_be_bytes());
            o.extend_from_slice(&(runs.len() as u16).to_be_bytes());
            for (off, b) in runs {
                o.extend_from_slice(&off.to_be_bytes()); o.extend_from_slice(&(b.len() as u16).to_be_bytes()); o.extend_from_slice(b);
            }
        }
        if !self.files.is_empty() {
            let mut names: Vec<_> = self.files.keys().cloned().collect(); names.sort();
            o.extend_from_slice(&(names.len() as u16).to_be_bytes());
            for n in names {
                o.push(n.len() as u8); o.extend_from_slice(n.as_bytes());
                let runs = &self.files[&n];
                o.extend_from_slice(&(runs.len() as u16).to_be_bytes());
                for r in runs {
                    o.extend_from_slice(&r.off.to_be_bytes()); o.extend_from_slice(&(r.new.len() as u16).to_be_bytes());
                    o.push((!r.orig.is_empty()) as u8); o.extend_from_slice(&r.orig); o.extend_from_slice(&r.new);
                }
            }
        }
        o
    }

    pub fn parse(b: &[u8]) -> Result<GfxPatch, String> {
        let mut p = 0usize;
        let rd = |p: &mut usize, n: usize| -> Result<&[u8], String> {
            let s = b.get(*p..*p + n).ok_or("truncated gfx patch")?; *p += n; Ok(s)
        };
        if rd(&mut p, 4)? != MAGIC { return Err("not a gfx patch".into()); }
        let h = rd(&mut p, 2)?; let n = u16::from_be_bytes([h[0], h[1]]);
        let mut g = GfxPatch::default();
        for _ in 0..n {
            let h = rd(&mut p, 14)?;
            let hash = u64::from_be_bytes(h[0..8].try_into().unwrap());
            let len = u32::from_be_bytes(h[8..12].try_into().unwrap());
            let nr = u16::from_be_bytes([h[12], h[13]]);
            let mut runs = vec![];
            for _ in 0..nr {
                let h = rd(&mut p, 6)?;
                let (off, l) = (u32::from_be_bytes(h[0..4].try_into().unwrap()), u16::from_be_bytes([h[4], h[5]]) as usize);
                runs.push((off, rd(&mut p, l)?.to_vec()));
            }
            g.blobs.insert((hash, len), runs);
        }
        if p < b.len() {
            let h = rd(&mut p, 2)?; let nf = u16::from_be_bytes([h[0], h[1]]);
            for _ in 0..nf {
                let l = rd(&mut p, 1)?[0] as usize;
                let name = String::from_utf8(rd(&mut p, l)?.to_vec()).map_err(|_| "bad file name")?;
                let h = rd(&mut p, 2)?; let nr = u16::from_be_bytes([h[0], h[1]]);
                let mut runs = vec![];
                for _ in 0..nr {
                    let h = rd(&mut p, 6)?;
                    let (off, l) = (u32::from_be_bytes(h[0..4].try_into().unwrap()), u16::from_be_bytes([h[4], h[5]]) as usize);
                    let has_orig = rd(&mut p, 1)?[0] != 0;
                    let orig = if has_orig { rd(&mut p, l)?.to_vec() } else { vec![] }; let new = rd(&mut p, l)?.to_vec();
                    runs.push(FileRun { off, orig, new });
                }
                g.files.insert(name, runs);
            }
        }
        Ok(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_apply() {
        let orig: Vec<u8> = (0..64u8).collect();
        let mut g = GfxPatch::default();
        g.insert(BlobPatch { hash: fnv64(&orig), len: 64, runs: vec![(4, vec![9, 9]), (60, vec![1, 2, 3, 4])] });
        let g2 = GfxPatch::parse(&g.build()).unwrap();
        assert_eq!(g, g2);
        let mut a = orig.clone();
        assert!(g2.apply(&mut a));
        assert_eq!(&a[4..6], &[9, 9]);
        assert_eq!(&a[60..], &[1, 2, 3, 4]);
        assert_eq!(a[0], 0);
        let mut other = orig.clone(); other[0] = 77; // a different blob is untouched
        let o2 = other.clone();
        assert!(!g2.apply(&mut other)); assert_eq!(other, o2);
        g.files.insert("DATA_X.BIN".into(), vec![FileRun { off: 2, orig: vec![2, 3], new: vec![7, 7] }]);
        let g3 = GfxPatch::parse(&g.build()).unwrap();
        assert_eq!(g, g3);
        let mut f = vec![0u8, 1, 2, 3, 4];
        assert_eq!(g3.apply_file("DATA_X.BIN", &mut f), 1);
        assert_eq!(f, [0, 1, 7, 7, 4]);
        assert_eq!(g3.apply_file("DATA_X.BIN", &mut f), 0, "bytes no longer the expected ones");
        assert_eq!(g3.apply_file("DATA_Y.BIN", &mut f), 0);
        // a run beyond the end grows the blob (tiles in free VRAM); an append run adds bytes at the end of the file
        let mut gg = GfxPatch::default();
        gg.insert(BlobPatch { hash: fnv64(&orig), len: 64, runs: vec![(60, vec![5; 8])] });
        gg.files.insert("DATA_G.BIN".into(), vec![FileRun { off: 5, orig: vec![], new: vec![8, 9] }]);
        let gg = GfxPatch::parse(&gg.build()).unwrap();
        let mut a = orig.clone();
        assert!(gg.apply(&mut a));
        assert_eq!(a.len(), 68); assert_eq!(&a[60..], &[5; 8]);
        let mut f = vec![0u8; 5];
        assert_eq!(gg.apply_file("DATA_G.BIN", &mut f), 1); assert_eq!(f, [0, 0, 0, 0, 0, 8, 9]);
        let mut f = vec![0u8; 6];
        assert_eq!(gg.apply_file("DATA_G.BIN", &mut f), 0, "an append only applies at the expected end");
        assert!(GfxPatch::parse(b"junk").is_err());
        assert!(GfxPatch::parse(&g.build()[..10]).is_err());
    }
}
