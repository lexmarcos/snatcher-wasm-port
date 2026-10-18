//! A small binary codec for engine snapshots (autosave, docs/re/web.md section 9).
//!
//! No dependencies and no derive: a type implements [`Snap`] with the [`snap_struct!`] macro, which lists every field.
//! The macro destructures the struct without `..`, so a field that is added later and not listed (or listed under
//! `skip { .. }` on purpose) is a compile error, never a silent hole in a snapshot.
//! Numbers are big-endian. A byte vector is stored as zero runs and literals (a VRAM image is mostly zeros).
//! While writing, every type and field name can be fed into a fingerprint: it changes when the layout changes, which
//! is how an autosave from another build is told apart (see `Engine::layout_fingerprint`).
use std::collections::HashMap;

pub type SnapResult<T> = Result<T, String>;

pub struct Writer {
    pub buf: Vec<u8>,
    /// FNV-1a over the names fed through [`Writer::tag`]; `None` = not collecting
    pub fp: Option<u64>,
}

impl Writer {
    pub fn new() -> Self { Writer { buf: Vec::new(), fp: None } }
    /// A writer that only collects the layout fingerprint (the bytes are thrown away at the end).
    pub fn fingerprint() -> Self { Writer { buf: Vec::new(), fp: Some(0xCBF2_9CE4_8422_2325) } }
    pub fn tag(&mut self, s: &str) {
        if let Some(h) = self.fp.as_mut() {
            for b in s.bytes().chain(std::iter::once(0)) { *h = (*h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3); }
        }
    }
    pub fn raw(&mut self, b: &[u8]) { self.buf.extend_from_slice(b); }
    pub fn varint(&mut self, mut v: u32) {
        while v >= 0x80 { self.buf.push((v & 0x7F) as u8 | 0x80); v >>= 7; }
        self.buf.push(v as u8);
    }
}

impl Default for Writer { fn default() -> Self { Self::new() } }

pub struct Reader<'a> { b: &'a [u8], p: usize }

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self { Reader { b, p: 0 } }
    pub fn remaining(&self) -> usize { self.b.len() - self.p }
    pub fn raw(&mut self, n: usize) -> SnapResult<&'a [u8]> {
        if n > self.remaining() { return Err("snapshot is truncated".into()); }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    pub fn varint(&mut self) -> SnapResult<u32> {
        let (mut v, mut sh) = (0u32, 0u32);
        loop {
            let b = self.raw(1)?[0];
            if sh > 28 { return Err("bad varint".into()); }
            v |= ((b & 0x7F) as u32) << sh;
            if b & 0x80 == 0 { return Ok(v); }
            sh += 7;
        }
    }
}

pub trait Snap: Sized {
    fn put(&self, w: &mut Writer);
    fn get(r: &mut Reader) -> SnapResult<Self>;
    /// Elements of a vector or array (bytes override this with the zero-run form).
    fn put_slice(xs: &[Self], w: &mut Writer) { for x in xs { x.put(w); } }
    fn get_vec(r: &mut Reader, n: usize) -> SnapResult<Vec<Self>> {
        // an element takes at least one byte, so a length beyond the data left is corrupt (and must not allocate)
        if n > r.remaining() { return Err("snapshot is truncated".into()); }
        let mut v = Vec::with_capacity(n);
        for _ in 0..n { v.push(Self::get(r)?); }
        Ok(v)
    }
}

macro_rules! snap_int {
    ($($t:ty),*) => {$(
        impl Snap for $t {
            fn put(&self, w: &mut Writer) { w.raw(&self.to_be_bytes()); }
            fn get(r: &mut Reader) -> SnapResult<Self> {
                const N: usize = std::mem::size_of::<$t>();
                let mut a = [0u8; N];
                a.copy_from_slice(r.raw(N)?);
                Ok(<$t>::from_be_bytes(a))
            }
        }
    )*};
}
snap_int!(u16, u32, u64, i8, i16, i32, i64);

impl Snap for u8 {
    fn put(&self, w: &mut Writer) { w.buf.push(*self); }
    fn get(r: &mut Reader) -> SnapResult<Self> { Ok(r.raw(1)?[0]) }
    /// zero runs and literals: `varint len`, then pairs (`varint zeros`, `varint literal length`, literal bytes) until `len` bytes are covered
    fn put_slice(xs: &[u8], w: &mut Writer) {
        w.varint(xs.len() as u32);
        let mut i = 0;
        while i < xs.len() {
            let z = xs[i..].iter().take_while(|&&b| b == 0).count();
            i += z;
            // a literal ends at the first run of 4+ zeros (shorter runs cost more as a pair than as literals)
            let mut j = i;
            while j < xs.len() && !(j + 4 <= xs.len() && xs[j..j + 4].iter().all(|&b| b == 0)) { j += 1; }
            w.varint(z as u32);
            w.varint((j - i) as u32);
            w.raw(&xs[i..j]);
            i = j;
        }
    }
    fn get_vec(r: &mut Reader, n: usize) -> SnapResult<Vec<u8>> {
        // `n` is unused for vectors (the length is in the stream); arrays pass their own length and check it
        let _ = n;
        let len = r.varint()? as usize;
        if len > 1 << 28 { return Err("byte vector too long".into()); }
        let mut v = Vec::with_capacity(len.min(1 << 20));
        while v.len() < len {
            let z = r.varint()? as usize;
            let l = r.varint()? as usize;
            if v.len() + z + l > len { return Err("corrupt byte run".into()); }
            v.resize(v.len() + z, 0);
            v.extend_from_slice(r.raw(l)?);
        }
        Ok(v)
    }
}

impl Snap for bool {
    fn put(&self, w: &mut Writer) { w.buf.push(*self as u8); }
    fn get(r: &mut Reader) -> SnapResult<Self> { match r.raw(1)?[0] { 0 => Ok(false), 1 => Ok(true), _ => Err("bad bool".into()) } }
}

impl Snap for usize {
    fn put(&self, w: &mut Writer) { (*self as u32).put(w); }
    fn get(r: &mut Reader) -> SnapResult<Self> { Ok(u32::get(r)? as usize) }
}

impl<T: Snap + 'static> Snap for Vec<T> {
    fn put(&self, w: &mut Writer) {
        if std::any::TypeId::of::<T>() != std::any::TypeId::of::<u8>() { (self.len() as u32).put(w); }
        T::put_slice(self, w);
    }
    fn get(r: &mut Reader) -> SnapResult<Self> {
        let n = if std::any::TypeId::of::<T>() == std::any::TypeId::of::<u8>() { 0 } else { u32::get(r)? as usize };
        T::get_vec(r, n)
    }
}

impl<T: Snap, const N: usize> Snap for [T; N] {
    fn put(&self, w: &mut Writer) { T::put_slice(self, w); }
    fn get(r: &mut Reader) -> SnapResult<Self> {
        let v = T::get_vec(r, N)?;
        v.try_into().map_err(|_| "array length mismatch".to_string())
    }
}

impl<T: Snap> Snap for Option<T> {
    fn put(&self, w: &mut Writer) { match self { None => w.buf.push(0), Some(x) => { w.buf.push(1); x.put(w); } } }
    fn get(r: &mut Reader) -> SnapResult<Self> { match r.raw(1)?[0] { 0 => Ok(None), 1 => Ok(Some(T::get(r)?)), _ => Err("bad option".into()) } }
}

impl<A: Snap, B: Snap> Snap for (A, B) {
    fn put(&self, w: &mut Writer) { self.0.put(w); self.1.put(w); }
    fn get(r: &mut Reader) -> SnapResult<Self> { Ok((A::get(r)?, B::get(r)?)) }
}
impl<A: Snap, B: Snap, C: Snap> Snap for (A, B, C) {
    fn put(&self, w: &mut Writer) { self.0.put(w); self.1.put(w); self.2.put(w); }
    fn get(r: &mut Reader) -> SnapResult<Self> { Ok((A::get(r)?, B::get(r)?, C::get(r)?)) }
}

/// A map is written in key order, so the same state always gives the same bytes.
impl<K: Snap + Ord + std::hash::Hash + Eq + Clone, V: Snap + Clone> Snap for HashMap<K, V> {
    fn put(&self, w: &mut Writer) {
        let mut v: Vec<(&K, &V)> = self.iter().collect();
        v.sort_by(|a, b| a.0.cmp(b.0));
        (v.len() as u32).put(w);
        for (k, x) in v { k.put(w); x.put(w); }
    }
    fn get(r: &mut Reader) -> SnapResult<Self> {
        let n = u32::get(r)? as usize;
        if n > r.remaining() { return Err("snapshot is truncated".into()); }
        let mut m = HashMap::with_capacity(n);
        for _ in 0..n { let k = K::get(r)?; let v = V::get(r)?; m.insert(k, v); }
        Ok(m)
    }
}

/// `snap_struct!(Type { a, b } skip { c })`: implement [`Snap`] for a struct from the listed fields. A field in `skip`
/// is not stored and comes back as `Default::default()`. Every field must be named in one of the two lists.
#[macro_export]
macro_rules! snap_struct {
    ($t:ty { $($f:ident),* $(,)? } $(skip { $($s:ident),* $(,)? })?) => {
        impl $crate::snap::Snap for $t {
            fn put(&self, w: &mut $crate::snap::Writer) {
                w.tag(stringify!($t));
                let Self { $($f,)* $($($s: _,)*)? } = self;
                $( w.tag(stringify!($f)); $crate::snap::Snap::put($f, w); )*
            }
            fn get(r: &mut $crate::snap::Reader) -> $crate::snap::SnapResult<Self> {
                Ok(Self { $($f: $crate::snap::Snap::get(r)?,)* $($($s: Default::default(),)*)? })
            }
        }
    };
}

/// `snap_enum!(Type { A, B, C })`: a fieldless enum, stored as the variant index.
#[macro_export]
macro_rules! snap_enum {
    ($t:ty { $($v:ident),* $(,)? }) => {
        impl $crate::snap::Snap for $t {
            fn put(&self, w: &mut $crate::snap::Writer) {
                w.tag(stringify!($t));
                let mut i = 0u8;
                $( if matches!(self, <$t>::$v) { w.buf.push(i); return; } i += 1; )*
                let _ = i;
                unreachable!()
            }
            fn get(r: &mut $crate::snap::Reader) -> $crate::snap::SnapResult<Self> {
                let n = r.raw(1)?[0];
                let mut i = 0u8;
                $( if n == i { return Ok(<$t>::$v); } i += 1; )*
                let _ = i;
                Err(concat!("bad ", stringify!($t)).into())
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Default)]
    struct S { a: u8, b: Vec<u8>, c: [u16; 3], d: Option<(u8, u32)>, e: HashMap<u32, u16>, junk: u8 }
    snap_struct!(S { a, b, c, d, e } skip { junk });

    #[test]
    fn round_trip_and_runs() {
        let mut b = vec![0u8; 5000];
        b[3] = 9; b[4000] = 1; b[4001] = 2; b[4003] = 7; b[4999] = 5;
        let s = S { a: 7, b, c: [1, 2, 65535], d: Some((3, 0xDEADBEEF)), e: [(5, 6), (1, 2)].into_iter().collect(), junk: 99 };
        let mut w = Writer::new();
        s.put(&mut w);
        assert!(w.buf.len() < 100, "zero runs are stored compactly: {}", w.buf.len());
        let mut r = Reader::new(&w.buf);
        let t = S::get(&mut r).unwrap();
        assert_eq!(r.remaining(), 0);
        assert_eq!(t, S { junk: 0, ..s });
        // same state, same bytes (map order)
        let mut w2 = Writer::new();
        t.put(&mut w2);
        assert_eq!(w.buf, w2.buf);
    }

    #[test]
    fn corrupt_input_is_an_error() {
        let s = S { a: 1, b: vec![1, 2, 3, 0, 0, 0, 0, 0, 4], c: [0; 3], d: None, e: HashMap::new(), junk: 0 };
        let mut w = Writer::new();
        s.put(&mut w);
        for n in 0..w.buf.len() { assert!(S::get(&mut Reader::new(&w.buf[..n])).is_err(), "truncated at {n}"); }
        let huge = [0u8, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F];
        assert!(S::get(&mut Reader::new(&huge)).is_err());
    }

    #[test]
    fn fingerprint_follows_names() {
        let mut w = Writer::fingerprint();
        S::default().put(&mut w);
        let f = w.fp.unwrap();
        let mut w = Writer::fingerprint();
        S::default().put(&mut w);
        assert_eq!(w.fp.unwrap(), f);
        assert_ne!(f, 0xCBF2_9CE4_8422_2325);
    }
}
