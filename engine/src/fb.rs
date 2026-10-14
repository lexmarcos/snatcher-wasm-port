//! Framebuffer helpers (the composer itself is `sncore::gfx::Vdp::render`).
pub fn clear(px: &mut [u8], rgba: [u8; 4]) { for p in px.chunks_exact_mut(4) { p.copy_from_slice(&rgba) } }
