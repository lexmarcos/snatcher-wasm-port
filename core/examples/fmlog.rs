//! Debug: log YM2612 writes of the FM driver for one sound id.
//! cargo run -p sncore --example fmlog -- <raw-dir> <id-hex> <frames>
use sncore::audio::kdriver::KDriver;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let raw = std::path::Path::new(&a[1]);
    let mut ip = std::fs::read(raw.join("header.bin")).unwrap()[0x200..0x800].to_vec();
    ip.extend_from_slice(&std::fs::read(raw.join("ip.bin")).unwrap());
    let fm = std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap();
    let mut d = KDriver::new(ip, fm);
    d.ym.log = Some(Vec::new());
    let id = u8::from_str_radix(&a[2], 16).unwrap();
    d.request(id);
    for f in 0..a[3].parse::<usize>().unwrap() {
        d.tick();
        let l = d.ym.log.replace(Vec::new()).unwrap();
        if !l.is_empty() {
            println!("frame {f}: {}", l.iter().map(|(p, r, v)| format!("{p}:{r:02x}={v:02x}")).collect::<Vec<_>>().join(" "));
        }
    }
}
