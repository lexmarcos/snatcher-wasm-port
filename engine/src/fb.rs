//! 320x224 RGBA framebuffer and Genesis colour helpers.
use crate::{H, W};

pub struct Framebuffer { pub px: Vec<u8> }

/// Convert a Genesis CRAM word (0000BBB0GGG0RRR0) to RGBA8.
/// Uses the common 3-bit → 8-bit ramp (v * 36, i.e. 0..252). HYPOTHESIS: exact DAC levels
/// differ slightly on hardware; this is the usual emulator approximation.
pub fn cram_to_rgba(c: u16) -> [u8; 4] {
    let r = ((c >> 1) & 7) as u8; let g = ((c >> 5) & 7) as u8; let b = ((c >> 9) & 7) as u8;
    [r * 36, g * 36, b * 36, 255]
}

impl Framebuffer {
    pub fn new() -> Self { Framebuffer { px: vec![0; W * H * 4] } }
    pub fn clear(&mut self, rgba: [u8; 4]) { for p in self.px.chunks_exact_mut(4) { p.copy_from_slice(&rgba) } }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, rgba: [u8; 4]) {
        if x < W && y < H { let o = (y * W + x) * 4; self.px[o..o + 4].copy_from_slice(&rgba) }
    }
    pub fn test_pattern(&mut self, t: u32, pad: u16) {
        for y in 0..H { for x in 0..W {
            let v = ((x as u32 + t) ^ (y as u32)) as u8;
            self.set(x, y, [v / 4, v / 3, if pad != 0 { 200 } else { v / 2 }, 255]);
        }}
    }
}

impl Default for Framebuffer { fn default() -> Self { Self::new() } }
