//! Differential check of KDriver vs. a recorded ORIGINAL-driver log (re/oracle/audio_ym.py / audio_inject.py).
//! cargo run --release -p sncore --example fmverify -- <raw-dir> <log.txt> [max-mismatches]
use sncore::audio::verify::{replay, replay_mode, verify_drum_pcm};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let raw = std::path::Path::new(&a[1]);
    let mut ip = std::fs::read(raw.join("header.bin")).unwrap()[0x200..0x800].to_vec();
    ip.extend_from_slice(&std::fs::read(raw.join("ip.bin")).unwrap());
    let fm = std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap();
    let log = std::fs::read_to_string(&a[2]).unwrap();
    let n = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(5);
    let rep = replay(ip, fm, &log, n);
    if let Ok(lt) = std::fs::read(raw.join("files/PCMLT_01.BIN")) {
        let dr = verify_drum_pcm(&log, &sncore::audio::parse_sfx_table(&lt), &std::fs::read(raw.join("files/PCMDRMDT.BIN")).unwrap_or_default());
        println!("PCM drum key-ons (sub driver): expected {} observed {} equal {} {:?}", dr.expected, dr.observed, dr.equal, dr.mismatches);
    }
    println!("{rep}");
    if log.contains("\nI ") {
        let q = replay_mode(std::fs::read(raw.join("header.bin")).unwrap()[0x200..0x800].iter().copied().chain(std::fs::read(raw.join("ip.bin")).unwrap()).collect(),
                            std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap(), &log, n, true);
        println!("via request queue (I -> enqueue, update per frame): {q}  rejects {:?}", q.queue_rejects);
    }
    let mut ids = rep.starts.clone(); ids.sort(); ids.dedup();
    println!("ticks {} (with volume word != FFFF: {}), distinct S ids {}", rep.ticks, rep.volume_ticks, ids.len());
    let cmds: Vec<String> = rep.cmd_stats.iter().enumerate().map(|(i, n)| format!("{:02X}:{}", 0xE0 + i, n)).collect();
    println!("sequence commands executed: {}", cmds.join(" "));
}
