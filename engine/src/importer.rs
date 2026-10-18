//! wasm bindings of the disc importer (`sncore::import`), used by the import Web Worker (web/src/import.worker.ts).
//! The worker passes one JS object with four methods: `read(file, offset, buf)` fills `buf` from a byte range of an image file
//! (FileReaderSync on a `File.slice`), `write(stream, bytes)` appends to an OPFS file, `finish(stream)` closes it and
//! `progress(phase, done, total)` reports. Errors are JSON `{"code": ..., "detail": ...}` strings.
use sncore::import::{self, DiscIo, FmRenderer, ImportError, Options, Sink};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type JsIo;
    #[wasm_bindgen(method, catch)]
    fn read(this: &JsIo, file: u32, offset: f64, buf: &mut [u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn write(this: &JsIo, stream: &str, data: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn finish(this: &JsIo, stream: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(method)]
    fn progress(this: &JsIo, phase: &str, done: f64, total: f64);
}

fn js_err(e: JsValue) -> String { e.as_string().or_else(|| js_sys_message(&e)).unwrap_or_else(|| format!("{e:?}")) }
fn js_sys_message(e: &JsValue) -> Option<String> {
    // Error objects: use their toString without pulling in js-sys
    #[wasm_bindgen]
    extern "C" { #[wasm_bindgen(js_name = String)] fn to_string(v: &JsValue) -> String; }
    Some(to_string(e))
}

struct Disc<'a>(&'a JsIo);
impl DiscIo for Disc<'_> {
    fn read(&mut self, file: usize, offset: u64, buf: &mut [u8]) -> Result<(), String> { self.0.read(file as u32, offset as f64, buf).map_err(js_err) }
}
struct Out<'a>(&'a JsIo);
impl Sink for Out<'_> {
    fn write(&mut self, stream: &str, data: &[u8]) -> Result<(), String> { self.0.write(stream, data).map_err(js_err) }
    fn finish(&mut self, stream: &str) -> Result<(), String> { self.0.finish(stream).map_err(js_err) }
    fn progress(&mut self, phase: &str, done: u64, total: u64) { self.0.progress(phase, done as f64, total as f64) }
}

/// The `FILE` names of a cue sheet, as a JSON array (`Err` = error JSON), so the page can match them with the files the user picked.
#[wasm_bindgen]
pub fn cue_files(cue: &str) -> Result<String, String> {
    let c = sncore::disc::Cue::parse(cue).map_err(|e| ImportError { code: "cue", detail: e }.to_json())?;
    Ok(format!("[{}]", c.files.iter().map(|f| import::jstr(f)).collect::<Vec<_>>().join(",")))
}

/// Import the disc: `sizes` = byte size of each `FILE` in cue order. Returns meta.json text; `Err` = error JSON.
#[wasm_bindgen]
pub fn import_disc(cue: &str, sizes: Vec<f64>, copy_cdda: bool, io: &JsIo) -> Result<String, String> {
    let sizes: Vec<u64> = sizes.into_iter().map(|s| s as u64).collect();
    import::import(cue, &sizes, Disc(io), &mut Out(io), Options { copy_cdda }).map(|s| s.meta_json).map_err(|e| e.to_json())
}

/// FM sound renderer for the cache (the Rust port of the 68000 FM driver).
#[wasm_bindgen]
pub struct FmRender { inner: FmRenderer }

#[wasm_bindgen]
impl FmRender {
    #[wasm_bindgen(constructor)]
    pub fn new(header: &[u8], ip: &[u8], fmwr: &[u8], pcmlt: &[u8], drmdt: &[u8]) -> Result<FmRender, String> {
        Ok(FmRender { inner: FmRenderer::new(header, ip, fmwr, pcmlt, drmdt)? })
    }
    /// Render sound `id` and append it to its stream (`fm/song_XX.s16` / `fm/sfx_XX.s16`, 16-bit LE stereo 48 kHz). Returns the length in frames.
    pub fn render(&self, id: u8, io: &JsIo) -> Result<u32, String> { self.inner.render_to(id, &mut Out(io)).map(|n| n as u32) }
}
