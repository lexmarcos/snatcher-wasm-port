//! Differential test of the FM driver translation (`kdriver`) against the ORIGINAL driver
//! running in the BIOS-free oracle (re/oracle). The oracle side is recorded by
//! `re/oracle/audio_ym.py` as a text event log:
//!   `F n` frame, `S id` fm_start (M:0xFF5306), `C id` command dispatch (M:0xFF518C),
//!   `T` fm_tick (M:0xFF408A), `V w` volume word M:$FF7F5E at the tick,
//!   `W port reg val ppc` YM2612 data write.
//! `replay` feeds the same calls into a fresh `KDriver` and compares the YM write streams
//! segment by segment (a segment = the writes between two driver entry events).
use super::kdriver::KDriver;

#[derive(Debug, Default)]
pub struct Report {
    pub segments: usize,
    pub segments_equal: usize,
    pub writes_oracle: usize,
    pub writes_ours: usize,
    pub writes_equal: usize,
    /// first few mismatching segments: (frame, event, oracle writes, our writes)
    pub mismatches: Vec<(u32, String, Vec<(u8, u8, u8)>, Vec<(u8, u8, u8)>)>,
    /// volume word values seen at ticks other than 0xFFFF (the replay assumes 0xFFFF)
    pub odd_volume: Vec<(u32, u16)>,
    /// number of volume words != 0xFFFF applied at ticks
    pub volume_ticks: usize,
    /// ticks replayed, `S` starts and `C` commands replayed
    pub ticks: usize,
    pub starts: Vec<u8>,
    /// executed sequence commands 0xE0..=0xFF in our driver (coverage of the replay)
    pub cmd_stats: [u32; 32],
}

impl Report {
    pub fn summary(&self) -> String {
        format!("segments {}/{} equal, writes oracle {} ours {} (positionally equal {})",
            self.segments_equal, self.segments, self.writes_oracle, self.writes_ours, self.writes_equal)
    }
}

fn fmt(v: &[(u8, u8, u8)]) -> String {
    v.iter().map(|(p, r, d)| format!("{p}:{r:02x}={d:02x}")).collect::<Vec<_>>().join(" ")
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", self.summary())?;
        for (fr, ev, o, u) in &self.mismatches {
            writeln!(f, "frame {fr} after {ev}:\n  oracle {}\n  ours   {}", fmt(o), fmt(u))?;
        }
        Ok(())
    }
}

/// Replay `log` (see module doc) with a fresh driver. `max_mismatch` limits the report.
pub fn replay(ip: Vec<u8>, fmwr: Vec<u8>, log: &str, max_mismatch: usize) -> Report {
    let mut d = KDriver::new(ip, fmwr);
    d.ym.log = Some(Vec::new());
    let mut rep = Report::default();
    let mut frame = 0u32;
    let mut cur_ev = String::from("start");
    let mut oracle: Vec<(u8, u8, u8)> = Vec::new();
    let flush = |d: &mut KDriver, rep: &mut Report, oracle: &mut Vec<(u8, u8, u8)>, frame: u32, ev: &str| {
        let ours = d.ym.log.replace(Vec::new()).unwrap_or_default();
        if oracle.is_empty() && ours.is_empty() { return; }
        rep.segments += 1;
        rep.writes_oracle += oracle.len();
        rep.writes_ours += ours.len();
        rep.writes_equal += oracle.iter().zip(ours.iter()).filter(|(a, b)| a == b).count();
        if *oracle == ours { rep.segments_equal += 1; } else if rep.mismatches.len() < max_mismatch {
            rep.mismatches.push((frame, ev.to_string(), oracle.clone(), ours));
        }
        oracle.clear();
    };
    let lines: Vec<&str> = log.lines().collect();
    for (li, line) in lines.iter().enumerate() {
        let mut it = line.split_whitespace();
        let Some(k) = it.next() else { continue };
        let hex = |s: Option<&str>| u32::from_str_radix(s.unwrap_or("0"), 16).unwrap_or(0);
        match k {
            "F" => { frame = it.next().and_then(|s| s.parse().ok()).unwrap_or(frame); }
            "W" => {
                let port = it.next().and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
                let reg = hex(it.next()) as u8; let val = hex(it.next()) as u8; let ppc = hex(it.next());
                if (0xFF4000..0xFF6142).contains(&ppc) { oracle.push((port, reg, val)); }
            }
            "V" => { let w = hex(it.next()) as u16; if w != 0xFFFF && rep.odd_volume.len() < 16 { rep.odd_volume.push((frame, w)); } }
            "S" | "C" | "T" => {
                flush(&mut d, &mut rep, &mut oracle, frame, &cur_ev);
                match k {
                    "S" => { let id = hex(it.next()) as u8; rep.starts.push(id); d.start(id); cur_ev = format!("S {id:02x}"); }
                    "C" => { let id = hex(it.next()) as u8; d.request(id); cur_ev = format!("C {id:02x}"); }
                    _ => {
                        // the volume word M:$FF7F5E is written by other main-CPU code (fades,
                        // boot): apply the value the original had at this tick (next `V` line)
                        if let Some(v) = lines.get(li + 1).and_then(|l| l.strip_prefix("V ")) {
                            let w = u16::from_str_radix(v.trim(), 16).unwrap_or(0xFFFF);
                            if w != 0xFFFF { rep.volume_ticks += 1; }
                            d.set_volume_word(w);
                        }
                        rep.ticks += 1;
                        d.tick(); cur_ev = "T".into();
                    }
                }
            }
            _ => {}
        }
    }
    flush(&mut d, &mut rep, &mut oracle, frame, &cur_ev);
    rep.cmd_stats = d.cmd_stats;
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs assets/raw (extraction) and the reference logs in assets/audio/ref/*.txt, which
    /// `re/oracle/audio_ref.py` records from the ORIGINAL driver (see docs/re/audio.md,
    /// "Verification against the oracle"). Skips when either is absent. Every log must replay
    /// with every segment (the YM2612 writes between two driver entry events) identical.
    #[test]
    fn fm_driver_matches_oracle_refs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let raw = root.join("assets/raw");
        let Ok(dir) = std::fs::read_dir(root.join("assets/audio/ref")) else {
            eprintln!("oracle reference logs missing, skipped"); return
        };
        let Ok(hdr) = std::fs::read(raw.join("header.bin")) else { eprintln!("assets missing, skipped"); return };
        let mut ip = hdr[0x200..0x800].to_vec();
        ip.extend_from_slice(&std::fs::read(raw.join("ip.bin")).unwrap());
        let fm = std::fs::read(raw.join("files/FMWR_1.BIN")).unwrap();
        let mut n = 0;
        for e in dir.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("txt") { continue }
            let log = std::fs::read_to_string(&path).unwrap();
            let rep = replay(ip.clone(), fm.clone(), &log, 5);
            eprintln!("{}: {}", path.file_name().unwrap().to_string_lossy(), rep.summary());
            assert!(rep.segments > 1000, "{}: too few segments: {}", path.display(), rep.summary());
            assert_eq!(rep.segments, rep.segments_equal, "{}: {rep}", path.display());
            n += 1;
        }
        if n == 0 { eprintln!("no reference logs, skipped"); }
    }
}
