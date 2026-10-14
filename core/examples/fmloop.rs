//! Debug: per-track tick-domain loop detection. cargo run --release -p sncore --example fmloop -- <raw> <id>
use sncore::audio::kdriver::KDriver;
use std::collections::HashMap;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let raw = std::path::Path::new(&a[1]);
    let mut ip = std::fs::read(raw.join("header.bin")).unwrap()[0x200..0x800].to_vec();
    ip.extend_from_slice(&std::fs::read(raw.join("ip.bin")).unwrap());
    let fm = std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap();
    let mut d = KDriver::new(ip, fm);
    let id = u8::from_str_radix(&a[2], 16).unwrap();
    d.request(id);
    let mut hist: Vec<Vec<Vec<u8>>> = vec![Vec::new(); 8];
    let mut last = 0;
    for _ in 0..60 * 60 * 10 {
        d.tick();
        if d.tick0 != last {
            last = d.tick0;
            for t in 0..8 { hist[t].push(d.track_key(t as u32)); }
        }
    }
    println!("cmds: {}", d.cmd_stats.iter().enumerate().filter(|(_, &n)| n > 0).map(|(i, n)| format!("{:02x}:{}", i + 0xE0, n)).collect::<Vec<_>>().join(" "));
    for t in 0..8 {
        let h = &hist[t];
        let mut first: HashMap<&Vec<u8>, usize> = HashMap::new();
        let mut res = None;
        for (i, k) in h.iter().enumerate() {
            if let Some(&g) = first.get(k) { { res = Some((g, i - g)); break } }
            first.entry(k).or_insert(i);
        }
        let ptrs: Vec<String> = h.iter().rev().take(1).map(|k| format!("{:02x?}", &k[k.len()-4..])).collect();
        println!("   ticks {} first-repeat {:?} active-at-end {} ptr {:?}", h.len(), res, h.last().map(|k| k[0]).unwrap_or(0), ptrs);
    }
}
