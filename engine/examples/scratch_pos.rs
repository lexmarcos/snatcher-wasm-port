use snengine::game::{Engine, FILE_NAMES};
fn main() {
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() { if *n == "PCMLD_01.BIN" { continue; } if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); } }
    for (i, (c, n)) in e.names.iter().enumerate().take(8) { println!("{i} col {c} {:02x?}", n); }
}
