//! Shared data-format code for the Snatcher web port.
//! Used both by the offline tools (`snt`) and by the wasm engine.
pub mod disc;
pub mod lzkn;

pub fn be16(d: &[u8], o: usize) -> u16 { u16::from_be_bytes([d[o], d[o + 1]]) }
pub fn be32(d: &[u8], o: usize) -> u32 { u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) }
