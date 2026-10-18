//! Shared data-format code for the Snatcher web port.
//! Used both by the offline tools (`snt`) and by the wasm engine.
pub mod disc;
pub mod sha256;
pub mod import;
pub mod lzkn;
pub mod gfx;     // graphics formats (owner: graphics workstream)
pub mod script;  // script bytecode (owner: script workstream)
pub mod audio;   // PCM / sound formats (owner: audio workstream)
pub mod actions; // sequencer, text, menus (owner: actions workstream)
pub mod prompt;  // button-prompt icons (controllers)
pub mod gfxpatch; // pt-BR picture substitution (decoded-blob patches)
pub mod i18n;    // translation: markup, pt-BR glyphs, pack, text layout rules
pub mod vwrap;   // voice subtitle layouts and run-time wrapping
pub mod snap;    // binary codec for engine snapshots (autosave)

pub fn be16(d: &[u8], o: usize) -> u16 { u16::from_be_bytes([d[o], d[o + 1]]) }
pub fn be32(d: &[u8], o: usize) -> u32 { u32::from_be_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) }
