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
    /// PCM drum commands (COMCMD5..6 triples, M:0xFF4DDE) of the oracle / ours / equal
    pub drum_mismatches: Vec<(u32, Vec<[u8; 3]>, Vec<[u8; 3]>)>,
    /// requests where the oracle's accept/reject decision differs from our `enqueue` (must stay empty)
    pub queue_rejects: Vec<(u32, u8)>,
    /// requests rejected by the priority gate in the oracle
    pub queue_gate_rejected: usize,
    pub drums_oracle: usize,
    pub drums_ours: usize,
    pub drums_equal: usize,
    /// executed sequence commands 0xE0..=0xFF in our driver (coverage of the replay)
    pub cmd_stats: [u32; 32],
}

impl Report {
    pub fn summary(&self) -> String {
        format!("segments {}/{} equal, writes oracle {} ours {} (positionally equal {}), drum commands oracle {} ours {} equal {}",
            self.segments_equal, self.segments, self.writes_oracle, self.writes_ours, self.writes_equal,
            self.drums_oracle, self.drums_ours, self.drums_equal)
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
        for (fr, o, u) in &self.drum_mismatches { writeln!(f, "frame {fr} drums: oracle {o:02x?} ours {u:02x?}")?; }
        Ok(())
    }
}

/// Replay `log` (see module doc) with a fresh driver. `max_mismatch` limits the report.
pub fn replay(ip: Vec<u8>, fmwr: Vec<u8>, log: &str, max_mismatch: usize) -> Report {
    replay_mode(ip, fmwr, log, max_mismatch, false)
}

/// `via_queue`: once the first injected request (`I id 1`, see audio_inject.py) is seen, the
/// oracle's own `S`/`C` entry events are ignored and our driver is driven only by the same
/// requests: `I` -> `KDriver::enqueue` (M:0xFF5058), `KDriver::update` (M:0xFF5106) wherever the oracle dequeued
/// `update` runs wherever the oracle's fm_update dequeued something. This also checks the request queue, the priority gate and the dequeue order.
pub fn replay_mode(ip: Vec<u8>, fmwr: Vec<u8>, log: &str, max_mismatch: usize, via_queue: bool) -> Report {
    let mut queue_mode = false;
    let mut d = KDriver::new(ip, fmwr);
    d.ym.log = Some(Vec::new());
    let mut rep = Report::default();
    let mut frame = 0u32;
    let mut cur_ev = String::from("start");
    let mut oracle: Vec<(u8, u8, u8)> = Vec::new();
    let mut odr: [u8; 3] = [0; 3];
    let mut odrums: Vec<[u8; 3]> = Vec::new();
    let flush = |d: &mut KDriver, rep: &mut Report, oracle: &mut Vec<(u8, u8, u8)>, odrums: &mut Vec<[u8; 3]>, frame: u32, ev: &str| {
        let ours_dr: Vec<[u8; 3]> = d.drums.drain(..).map(|e| [e.b0, e.b1, e.b2]).collect();
        rep.drums_oracle += odrums.len();
        rep.drums_ours += ours_dr.len();
        if *odrums == ours_dr { rep.drums_equal += ours_dr.len(); } else if rep.drum_mismatches.len() < 8 {
            rep.drum_mismatches.push((frame, odrums.clone(), ours_dr));
        }
        odrums.clear();
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
            "F" => {
                frame = it.next().and_then(|s| s.parse().ok()).unwrap_or(frame);

            }
            "I" if via_queue => {
                let id = hex(it.next()) as u8; let acc = it.next() == Some("1");
                if acc && !queue_mode { queue_mode = true; flush(&mut d, &mut rep, &mut oracle, &mut odrums, frame, &cur_ev); }
                if queue_mode {
                    // the injector replicates the gate of M:0xFF5058: accepted <=> our enqueue accepts
                    if d.enqueue(id) != acc { rep.queue_rejects.push((frame, id)); }
                    if !acc { rep.queue_gate_rejected += 1; }
                }
            }
            // the oracle's own dequeue (fm_update M:0xFF5106 found a request) marks where our
            // `update` runs; which id it starts comes from OUR queue. (Within a frame the
            // original runs fm_update before or after fm_tick, so the position cannot be fixed.)
            "S" | "C" if queue_mode => {
                if k == "S" { rep.starts.push(hex(it.next()) as u8); }
                flush(&mut d, &mut rep, &mut oracle, &mut odrums, frame, &cur_ev);
                d.update();
                cur_ev = format!("update ({k})");
            }
            "W" => {
                let port = it.next().and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
                let reg = hex(it.next()) as u8; let val = hex(it.next()) as u8; let ppc = hex(it.next());
                if (0xFF4000..0xFF6142).contains(&ppc) { oracle.push((port, reg, val)); }
            }
            "D" => {
                let n = it.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0);
                odr[n.min(2)] = hex(it.next()) as u8;
                if n == 2 { odrums.push(odr); }
            }
            "V" => { let w = hex(it.next()) as u16; if w != 0xFFFF && rep.odd_volume.len() < 16 { rep.odd_volume.push((frame, w)); } }
            "S" | "C" | "T" => {
                flush(&mut d, &mut rep, &mut oracle, &mut odrums, frame, &cur_ev);
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
    flush(&mut d, &mut rep, &mut oracle, &mut odrums, frame, &cur_ev);
    rep.cmd_stats = d.cmd_stats;
    rep
}

/// Result of [`verify_drum_pcm`].
#[derive(Debug, Default)]
pub struct DrumPcmReport {
    /// key-ons predicted from the drum commands (COMCMD5..6 triples via `DrumEvent::decode` and
    /// the PCM sfx table) / key-on pairs observed on RF5C164 channels 4..7 / predictions that
    /// equal the observation in order (channel, wave-RAM start page, FD, ENV, PAN)
    pub expected: usize,
    pub observed: usize,
    pub equal: usize,
    /// first mismatches: (index, expected (ch, start page, fd, env, pan), observed)
    pub mismatches: Vec<(usize, (u8, u8, u16, u8, u8), Option<(u8, u8, u16, u8, u8)>)>,
}

/// Check the sub-CPU side of the PCM drums (S:0x160BA decode + S:0x161F4 play): every drum command
/// triple the main CPU wrote (`D` lines) must produce, in order, an RF5C164 key-on pair (`P`
/// lines, channels 4/5 for logical channel 2 and 6/7 for 3) with FD, ENV and PAN from the sfx
/// table entry of the note: ENV = table ENV * (vel << 3) >> 8, or the table ENV for vel 0
/// (S:0x16260); notes 6, 8, 9, 10 are skipped (S:0x161F4).
pub fn verify_drum_pcm(log: &str, sfx: &[super::PcmSfxEntry], drmdt: &[u8]) -> DrumPcmReport {
    use super::kdriver::DrumEvent;
    let mut rep = DrumPcmReport::default();
    // wave-RAM start pages of the drum samples as laid out by pcm_drum_preload S:0x16128: notes
    // 1..12 except 6, 8, 9, 10 consecutively from 0x8000; each sample is copied from the
    // segment's data (header + 0x10) up to the first 0xFF, followed by 4 marker bytes, and the
    // next one starts on the following 256-byte page (S:0x161D6..0x161E0)
    let mut page = [0u8; 13];
    let mut d5 = 0x8000u32;
    for n in 1..13u8 {
        if [6, 8, 9, 10].contains(&n) { continue }
        let Some(e) = sfx.iter().find(|e| e.id == n) else { continue };
        let off = (e.addr.wrapping_sub(super::DRUM_BASE) + 0x10) as usize;
        let Some(len) = drmdt.get(off..).and_then(|d| d.iter().position(|&b| b == 0xFF)) else { continue };
        page[n as usize] = (d5 >> 8) as u8;
        d5 += len as u32 + 4;
        if d5 & 0xFF != 0 { d5 = (d5 + 0xFF) & !0xFF; }
    }
    let mut exp: Vec<(u8, u8, u16, u8, u8)> = Vec::new();
    let mut obs: Vec<(u8, u8, u16, u8, u8)> = Vec::new();
    let mut odr = [0u8; 3];
    let mut since_f1 = false;
    let push = |odr: &[u8; 3], exp: &mut Vec<(u8, u8, u16, u8, u8)>| {
        for (note, vel, chan) in (DrumEvent { b0: odr[0], b1: odr[1], b2: odr[2] }).decode() {
            if [6, 8, 9, 10].contains(&note) { continue }
            let Some(e) = sfx.iter().find(|e| e.id == note) else { continue };
            let env = if vel == 0 { e.env } else { ((e.env as u32 * vel as u32) >> 8) as u8 };
            exp.push((chan * 2, page[note as usize], e.fd, env, e.pan));
        }
    };
    let mut pend: Option<(u8, u8, u16, u8, u8)> = None;
    for line in log.lines() {
        let mut it = line.split_whitespace();
        let h = |s: Option<&str>| u32::from_str_radix(s.unwrap_or("0"), 16).unwrap_or(0);
        match it.next() {
            Some("D") => {
                let n = it.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or(0).min(2);
                odr[n] = h(it.next()) as u8;
                if n == 2 { push(&odr, &mut exp); since_f1 = true; }
            }
            // F1 (stop all, M:0xFF52AC) clears the drum command counter A1201D (M:0xFF52C2); the
            // sub CPU sees a changed counter and plays the stale COMCMD5..6 command once more
            Some("C") if it.next() == Some("f1") => {
                if since_f1 { push(&odr, &mut exp); since_f1 = false; }
            }
            Some("P") => {
                let ch = it.next().and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
                let st = h(it.next()) as u8; let fd = h(it.next()) as u16; let _ls = h(it.next());
                let env = h(it.next()) as u8; let pan = h(it.next()) as u8;
                if (4..8).contains(&ch) {
                    if ch & 1 == 0 { pend = Some((ch, st, fd, env, pan)); }
                    else if let Some(p) = pend.take() { if p == (ch - 1, st, fd, env, pan) { obs.push(p); } }
                }
            }
            _ => {}
        }
    }
    rep.expected = exp.len();
    rep.observed = obs.len();
    for (k, e) in exp.iter().enumerate() {
        if obs.get(k) == Some(e) { rep.equal += 1; }
        else if rep.mismatches.len() < 8 { rep.mismatches.push((k, *e, obs.get(k).copied())); }
    }
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
        let sfx = std::fs::read(raw.join("files/PCMLT_01.BIN")).ok().map(|lt| crate::audio::parse_sfx_table(&lt));
        let drmdt = std::fs::read(raw.join("files/PCMDRMDT.BIN")).unwrap_or_default();
        let mut n = 0;
        for e in dir.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("txt") { continue }
            let log = std::fs::read_to_string(&path).unwrap();
            let rep = replay(ip.clone(), fm.clone(), &log, 5);
            eprintln!("{}: {}", path.file_name().unwrap().to_string_lossy(), rep.summary());
            assert!(rep.segments > 1000, "{}: too few segments: {}", path.display(), rep.summary());
            assert_eq!(rep.segments, rep.segments_equal, "{}: {rep}", path.display());
            if log.contains("\nI ") {
                let q = replay_mode(ip.clone(), fm.clone(), &log, 5, true);
                eprintln!("  via request queue: {}", q.summary());
                assert_eq!(q.segments, q.segments_equal, "{} (queue mode): {q}", path.display());
                assert!(q.queue_rejects.is_empty(), "{:?}", q.queue_rejects);
            }
            assert_eq!(rep.drums_oracle, rep.drums_equal, "{}: {rep}", path.display());
            assert_eq!(rep.drums_ours, rep.drums_equal, "{}: {rep}", path.display());
            if let Some(sfx) = &sfx {
                let dr = verify_drum_pcm(&log, sfx, &drmdt);
                eprintln!("  drum key-ons: expected {} observed {} equal {}", dr.expected, dr.observed, dr.equal);
                assert_eq!(dr.expected, dr.equal, "{}: {:?}", path.display(), dr);
                assert_eq!(dr.expected, dr.observed, "{}: {:?}", path.display(), dr);
            }
            n += 1;
        }
        if n == 0 { eprintln!("no reference logs, skipped"); }
    }
}
