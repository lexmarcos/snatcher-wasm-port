//! Differential check of KDriver vs. a recorded ORIGINAL-driver log (re/oracle/audio_ym.py / audio_inject.py).
//! cargo run --release -p sncore --example fmverify -- <raw-dir> <log.txt> [max-mismatches]
use sncore::audio::verify::replay;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let raw = std::path::Path::new(&a[1]);
    let mut ip = std::fs::read(raw.join("header.bin")).unwrap()[0x200..0x800].to_vec();
    ip.extend_from_slice(&std::fs::read(raw.join("ip.bin")).unwrap());
    let fm = std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap();
    let log = std::fs::read_to_string(&a[2]).unwrap();
    let n = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    let rep = replay(ip, fm, &log, n);
    println!("{rep}");
    let mut ids = rep.starts.clone(); ids.sort(); ids.dedup();
    println!("ticks {} (with volume word != FFFF: {}), distinct S ids {}", rep.ticks, rep.volume_ticks, ids.len());
    let cmds: Vec<String> = rep.cmd_stats.iter().enumerate().map(|(i, n)| format!("{:02X}:{}", 0xE0 + i, n)).collect();
    println!("sequence commands executed: {}", cmds.join(" "));
}
