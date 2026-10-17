//! CD-DA speech subtitles (docs/voice-subtitles.md "CD-DA speech"): a function of the CD clock (frames since the engine started the track), drawn as an overlay
//! on the intro screens, in pt-BR only with the option on; no game state or timing changes; English byte-identical. Synthetic subtitles (no game text).
//! Needs assets/raw (skipped otherwise).
use snengine::game::{Engine, Top, FILE_NAMES};
use sncore::i18n::{self as i, CdSub, Translation};

fn engine(pack: Option<&Translation>, pt: bool, on: bool) -> Option<Engine> {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw");
    if !base.join("files/SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(base.join("files").join(n)) { e.load_file(n, d); }
    }
    for (n, f) in [("HEADER.BIN", "header.bin"), ("IP.BIN", "ip.bin")] {
        if let Ok(d) = std::fs::read(base.join(f)) { e.load_file(n, d); }
    }
    if let Some(p) = pack { e.load_translation(&p.build()).unwrap(); }
    e.set_language(pt);
    e.vsub.on = on;
    e.boot();
    Some(e)
}

fn pack() -> Translation {
    let mut t = Translation::default();
    t.cdda.entry(3).or_default().push(CdSub { start_ms: 4000, end_ms: 7000, speaker: 0, text: i::markup_to_bytes("Moscou, seis de junho.{nl}Algo mudou.").unwrap() });
    t.cdda.entry(3).or_default().push(CdSub { start_ms: 9000, end_ms: 12000, speaker: 0, text: i::markup_to_bytes("Segunda frase.").unwrap() });
    t
}

/// frame, pad of the START presses that open the options and start the intro (title_to_intro in engine/src/modules/title.rs)
fn pad(f: u64) -> u8 { if f == 1301 || f == 1612 { 0x80 } else { 0 } }

#[test]
fn the_subtitle_follows_the_cd_clock_and_changes_nothing_else() {
    let t = pack();
    let (Some(mut off), Some(mut on), Some(mut en)) = (engine(Some(&t), true, false), engine(Some(&t), true, true), engine(Some(&t), false, true)) else { return };
    let mut start = None;
    let mut seen: Vec<(i64, bool)> = vec![];
    for f in 1..=2400u64 {
        off.tick(pad(f)); on.tick(pad(f)); en.tick(pad(f));
        assert_eq!(off.debug_info(), on.debug_info(), "game state differs at frame {f}");
        assert_eq!(en.fb, off.fb, "English frame {f} differs");
        if start.is_none() && on.cd_start.is_some() && on.top == Top::Intro { start = on.cd_start; }
        if let Some(s) = start {
            let pos = (f - s) as i64;
            let active = on.vsub_active().is_some();
            seen.push((pos, active));
            if !active { assert_eq!(on.fb, off.fb, "pixels differ without a subtitle at {f}"); }
            else {
                let r = on.vsub_info().unwrap().rect;
                assert!(r.2 > r.0, "a subtitle without a rectangle");
                // every changed pixel is inside the reported rectangle
                for y in 0..snengine::game::H { for x in 0..snengine::game::W {
                    let k = (y * snengine::game::W + x) * 4;
                    if on.fb[k..k + 4] != off.fb[k..k + 4] { assert!(x >= r.0 && x < r.2 && y >= r.1 && y < r.3, "pixel ({x},{y}) outside {r:?}"); }
                } }
            }
        }
    }
    let s = start.expect("the intro starts track 3");
    let on_at = |sec: f64| seen.iter().find(|p| p.0 == (sec * 60.0) as i64).map(|p| p.1).unwrap_or(false);
    assert!(!on_at(3.0), "before the first segment");
    assert!(on_at(4.5) && on_at(6.9), "during the first segment");
    assert!(!on_at(8.5), "between the segments (after the hold)");
    assert!(on_at(9.5) && on_at(11.5), "during the second");
    assert!(!on_at(20.0), "after the last one");
    let _ = s;
}
