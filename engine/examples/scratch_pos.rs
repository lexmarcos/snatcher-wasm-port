use snengine::game::{Engine, FILE_NAMES, H, W};
fn main() {
    let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw/files");
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() { if *n == "PCMLD_01.BIN" { continue; } if let Ok(d) = std::fs::read(raw.join(n)) { e.load_file(n, d); } }
    let n = e.load_translation(&std::fs::read("i18n/build/pt-BR.pack").unwrap()).unwrap(); e.set_language(true); let _ = n;
    e.jump_path(57 + 6, 0x4d, &[]);
    use sncore::script::vm::Action;
    e.queue.push(Action { cmd: 0x0F, args: vec![] }); e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, 8] }); e.queue.push(Action { cmd: 0x12, args: vec![2] });
    for f in 0..260 { e.tick(0); if f == 250 { break; } }
    // bitmap bbox of ink
    let (mut x0, mut y0, mut x1, mut y1) = (999, 999, 0, 0);
    for y in 0..64 { for x in 0..256 { if e.text.pix[y * 256 + x] != 0 && !(y >= 56 && x >= 248) { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); } } }
    println!("bitmap ink bbox x {x0}..{x1} y {y0}..{y1}");
    let (mut a, mut b, mut c, mut d) = (999, 999, 0, 0);
    for y in 140..H { for x in 0..W { let i = (y * W + x) * 4; if e.fb[i] > 200 || e.fb[i+1] > 200 || e.fb[i+2] > 200 { a = a.min(x); b = b.min(y); c = c.max(x); d = d.max(y); } } }
    println!("screen bright bbox x {a}..{c} y {b}..{d}");
    println!("cram {:?}", &e.disp.vdp.cram[..16]); println!("cram_b {:?}", &e.disp.vdp.cram_b[..16]);
    // colours
    for y in 150..214 { for x in 0..256 { let i=(y*W+x)*4; if e.fb[i]!=0||e.fb[i+1]!=0||e.fb[i+2]!=0 { println!("first non-black {x},{y} {:?}", &e.fb[i..i+3]); break; } } if false {break;} }
}
