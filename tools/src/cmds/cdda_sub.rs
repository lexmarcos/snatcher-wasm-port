//! Subtitles of the speech on CD-DA tracks (docs/voice-subtitles.md "CD-DA speech"): `<lang>/cdda.json`, the importer and the validator.
//!
//!   {"lang": "pt-BR", "tracks": {"3": {"12340": {"e": "15200", "h": "<hash of the English transcript>", "s": "<speaker, optional>", "x": "<markup>"}}}}
//!
//! Keyed by track number, then by the start of the segment in milliseconds from the start of the track (the engine's CD clock); `e` = end of the segment, `h` = FNV-1a of the English text of the segment,
//! `x` = the pt-BR text (stored unwrapped, wrapped and paged at run time like the voice subtitles). English transcripts stay in `work/cdda/transcripts/en.tsv`
//! (gitignored, written by `scripts/cdda-transcribe.py`; override with `CDDA_EN`): `track<TAB>start_ms<TAB>end_ms<TAB>speech 1/0<TAB>flag<TAB>text`.
use super::i18n::{font_widths, json_parse, jstr, lang_path, spk_names, Report, J};
use super::voice_sub::{clean_text, en_hash, speaker_arg};
use sncore::audio::CD_TRACK_SECTORS;
use sncore::i18n as i;
use sncore::vwrap as vw;
use std::collections::{BTreeMap, HashMap};
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub struct CEntry { pub start: u32, pub end: u32, pub h: u32, pub s: u8, pub t: String }
pub type CFile = BTreeMap<u8, Vec<CEntry>>;

pub fn cdda_path(lang: &str) -> String { lang_path(lang, "cdda") }

pub fn read_cdda(path: &str) -> Result<CFile, String> {
    let Ok(txt) = fs::read_to_string(path) else { return Ok(CFile::new()) };
    let J::Obj(top) = json_parse(&txt).map_err(|e| format!("{path}: {e}"))? else { return Err(format!("{path}: not an object")) };
    let mut out = CFile::new();
    for (k, v) in top {
        if k != "tracks" { continue; }
        let J::Obj(ts) = v else { return Err(format!("{path}: tracks is not an object")) };
        for (kt, list) in ts {
            let track: u8 = kt.parse().map_err(|_| format!("{path}: bad track {kt:?}"))?;
            // the minimal JSON reader has objects only: a track is an object whose keys are the start times (ms) and values the entries
            let J::Obj(es) = list else { return Err(format!("{path}: track {kt} is not an object")) };
            let v = out.entry(track).or_default();
            for (ks, e) in es {
                let start: u32 = ks.parse().map_err(|_| format!("{path}: track {kt}: bad start {ks:?}"))?;
                let J::Obj(f) = e else { return Err(format!("{path}: track {kt} @{ks} is not an object")) };
                let (mut end, mut h, mut s, mut t) = (None, None, 0u8, None);
                for (fk, fv) in f {
                    match (fk.as_str(), fv) {
                        ("e", J::Str(x)) => end = x.parse().ok(),
                        ("h", J::Str(x)) => h = u32::from_str_radix(&x, 16).ok(),
                        ("s", J::Str(x)) => s = x.parse().map_err(|_| format!("{path}: track {kt} @{ks}: bad speaker"))?,
                        ("x", J::Str(x)) => t = Some(x),
                        _ => {}
                    }
                }
                let at = format!("{path}: track {kt} @{ks}");
                v.push(CEntry { start, end: end.ok_or(format!("{at}: no end"))?, h: h.ok_or(format!("{at}: no hash"))?, s, t: t.ok_or(format!("{at}: no text"))? });
            }
            v.sort_by_key(|e| e.start);
        }
    }
    Ok(out)
}

pub fn write_cdda(path: &str, c: &CFile) -> Result<(), String> {
    let mut s = String::from("{\n  \"lang\": \"pt-BR\",\n  \"tracks\": {\n");
    let nt = c.len();
    for (ti, (track, es)) in c.iter().enumerate() {
        s += &format!("    \"{track}\": {{\n");
        for (k, e) in es.iter().enumerate() {
            let sp = if e.s > 0 { format!(", \"s\": \"{}\"", e.s) } else { String::new() };
            s += &format!("      \"{}\": {{\"e\": \"{}\", \"h\": \"{:08x}\"{sp}, \"x\": {}}}{}\n", e.start, e.end, e.h, jstr(&e.t), if k + 1 < es.len() { "," } else { "" });
        }
        s += &format!("    }}{}\n", if ti + 1 < nt { "," } else { "" });
    }
    s += "  }\n}\n";
    fs::write(path, s).map_err(|e| format!("{path}: {e}"))
}

/// English segments by (track, start ms): (end ms, speech, flag, text).
pub struct CEn { pub end: u32, pub speech: bool, pub flag: String, pub text: String }

pub fn read_en() -> HashMap<(u8, u32), CEn> {
    let path = std::env::var("CDDA_EN").unwrap_or("work/cdda/transcripts/en.tsv".into());
    let Ok(t) = fs::read_to_string(&path) else { return HashMap::new() };
    t.lines().filter_map(|l| {
        let f: Vec<&str> = l.splitn(6, '\t').collect();
        if f.len() < 6 { return None; }
        Some(((f[0].parse().ok()?, f[1].parse().ok()?), CEn { end: f[2].parse().ok()?, speech: f[3] == "1", flag: f[4].to_string(), text: f[5].to_string() }))
    }).collect()
}

/// Length of a track in milliseconds.
pub fn track_ms(track: u8) -> Option<u32> { CD_TRACK_SECTORS.iter().find(|t| t.0 as u8 == track).map(|t| (t.1 as u64 * 1000 / 75) as u32) }

/// `track<TAB>start_ms<TAB>speaker<TAB>text` lines -> `<lang>/cdda.json` (end and hash come from the English transcript).
pub fn import(lang: &str, dir: &str, patch: &str) -> Result<(), String> {
    fs::create_dir_all(lang).map_err(|e| e.to_string())?;
    let txt = fs::read_to_string(patch).map_err(|e| format!("{patch}: {e}"))?;
    let names = spk_names(dir);
    let en = read_en();
    let path = cdda_path(lang);
    let mut c = read_cdda(&path)?;
    let mut count = 0;
    for (ln, line) in txt.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let f: Vec<&str> = line.splitn(4, '\t').collect();
        if f.len() != 4 { return Err(format!("{patch}:{}: want track<TAB>start_ms<TAB>speaker<TAB>text", ln + 1)); }
        let track: u8 = f[0].trim().parse().map_err(|_| format!("{patch}:{}: bad track", ln + 1))?;
        let start: u32 = f[1].trim().parse().map_err(|_| format!("{patch}:{}: bad start (milliseconds)", ln + 1))?;
        let s = speaker_arg(f[2], &names).map_err(|e| format!("{patch}:{}: {e}", ln + 1))?;
        let e = en.get(&(track, start));
        let (end, h) = match e {
            Some(e) => (e.end, en_hash(&e.text)),
            None => {
                let old = c.get(&track).and_then(|v| v.iter().find(|x| x.start == start)).ok_or(format!("{patch}:{}: no English segment {track} @{start} (work/cdda/transcripts/en.tsv)", ln + 1))?;
                (old.end, old.h)
            }
        };
        let v = c.entry(track).or_default();
        v.retain(|x| x.start != start);
        v.push(CEntry { start, end, h, s, t: clean_text(f[3]) });
        v.sort_by_key(|x| x.start);
        count += 1;
    }
    write_cdda(&path, &c)?;
    println!("imported {count} CD-DA subtitles ({} in {path})", c.values().map(|v| v.len()).sum::<usize>());
    Ok(())
}

/// CD-DA section of the pack.
pub fn pack_cdda(lang: &str) -> Result<BTreeMap<u8, Vec<i::CdSub>>, String> {
    let mut m = BTreeMap::new();
    for (track, es) in read_cdda(&cdda_path(lang))? {
        let mut v = vec![];
        for e in es { v.push(i::CdSub { start_ms: e.start, end_ms: e.end, speaker: e.s, text: i::markup_to_bytes(&e.t).map_err(|x| format!("cdda {track} @{}: {x}", e.start))? }); }
        m.insert(track, v);
    }
    Ok(m)
}

/// Validate `<lang>/cdda.json`; returns the number of subtitles checked.
pub fn check_into(lang: &str, dir: &str, rep: &mut Report) -> Result<usize, String> {
    let c = read_cdda(&cdda_path(lang))?;
    if c.is_empty() { return Ok(0); }
    let widths = font_widths(dir)?;
    let names = spk_names(dir);
    let en = read_en();
    if en.is_empty() { rep.warns.push("cdda: work/cdda/transcripts/en.tsv not found, the English hashes are not checked".into()); }
    let (mut n, mut max_cps) = (0usize, 0.0f64);
    for (track, es) in &c {
        let Some(len) = track_ms(*track) else { rep.fails.push(format!("cdda {track}: not a CD-DA track of the disc (2..21)")); continue };
        for (k, e) in es.iter().enumerate() {
            rep.checked += 1; n += 1;
            let t = format!("cdda {track} @{}", e.start);
            if e.end <= e.start { rep.fails.push(format!("{t}: ends at {} ms, before it starts", e.end)); }
            if e.end > len + 500 { rep.fails.push(format!("{t}: ends at {} ms, the track is {len} ms long", e.end)); }
            if k > 0 && es[k - 1].start == e.start { rep.fails.push(format!("{t}: two subtitles start at the same time")); }
            if let Some(x) = en.get(&(*track, e.start)) {
                if en_hash(&x.text) != e.h { rep.fails.push(format!("{t}: the English transcript changed (hash {:08x}, entry {:08x}): translate again", en_hash(&x.text), e.h)); }
                if !x.speech { rep.fails.push(format!("{t}: the segment is flagged as not speech ({}), it gets no subtitle", x.flag)); }
                if x.end != e.end { rep.fails.push(format!("{t}: the English segment ends at {} ms, the entry says {}", x.end, e.end)); }
            } else if !en.is_empty() { rep.fails.push(format!("{t}: no such segment in the English transcript")); }
            if e.s == 1 || (e.s >= 2 && names.get(e.s as usize - 1).map_or(true, |n| n.is_empty())) { rep.fails.push(format!("{t}: speaker {} is not a name (see snt i18n-names)", e.s)); }
            let bytes = match i::markup_to_bytes(&e.t) { Ok(b) => b, Err(m) => { rep.fails.push(format!("{t}: {m}")); continue } };
            if bytes.is_empty() { rep.fails.push(format!("{t}: empty subtitle")); continue; }
            if bytes.iter().any(|&c| c == i::AUTO || matches!(c, 0xEC | 0xEE | 0xF0) || c == 0xF4 || (c < 0x20)) { rep.fails.push(format!("{t}: control tokens other than {{nl}}, {{page}} and colours")); }
            let name = if e.s >= 2 { names.get(e.s as usize - 1).filter(|n| !n.is_empty()) } else { None };
            let prefix_px = name.map_or(0, |n| { let nb = i::markup_to_bytes(n).unwrap_or_default(); vw::text_width(&widths, &nb) + vw::text_width(&widths, b": ") });
            let chars: usize = bytes.split(|&c| c == i::PAGE).map(|p| i::voice_glyph_count(p)).sum();
            let total = i::cdda_display_frames(e.start, e.end, es.get(k + 1).map(|x| x.start), chars);
            let secs = total as f64 / 60.0;
            let cps = chars as f64 / secs;
            max_cps = max_cps.max(cps);
            if chars as f64 / i::VOICE_READ_CPS > secs + 0.02 {
                rep.fails.push(format!("{t}: reading speed {cps:.1} chars/s over the {secs:.1} s it can stay up ({chars} chars, speech {:.1} s, next subtitle {}), limit {:.0}: shorten it or move the start earlier",
                    (e.end - e.start) as f64 / 1000.0, es.get(k + 1).map_or("none".to_string(), |x| format!("at {} ms", x.start)), i::VOICE_READ_CPS));
            }
            for (label, lay) in vw::all_layouts(name.is_some()) {
                let pp = if lay.name_inline { prefix_px } else { 0 };
                let pages = vw::paginate(&bytes, &widths, &lay, pp);
                if pages.is_empty() { rep.fails.push(format!("{t}: {label}: no text")); continue; }
                for (kp, pg) in pages.iter().enumerate() {
                    if pg.len() > lay.max { rep.fails.push(format!("{t}: {label}: page {} has {} lines (limit {})", kp + 1, pg.len(), lay.max)); }
                    for (ln, line) in pg.iter().enumerate() {
                        let w = vw::text_width(&widths, line) + if ln == 0 { pp } else { 0 };
                        if w > lay.width() { rep.fails.push(format!("{t}: {label}: line is {w} px wide, limit {}: \"{}\"", lay.width(), i::bytes_to_markup(line))); }
                    }
                }
                let pb: Vec<Vec<u8>> = pages.iter().map(|p| p.join(&i::NL)).collect();
                let starts = i::voice_page_starts(&pb, total);
                for (kp, p) in pb.iter().enumerate() {
                    let end = starts.get(kp + 1).copied().unwrap_or(total);
                    let ps = (end - starts[kp]) as f64 / 60.0;
                    let pc = i::voice_glyph_count(p) as f64;
                    if pc / ps > i::VOICE_READ_CPS + 0.5 { rep.fails.push(format!("{t}: {label}: page {} shows {pc} chars for {ps:.1} s", kp + 1)); }
                }
            }
        }
    }
    println!("cdda-check: {n} subtitles on {} tracks, fastest {max_cps:.1} chars/s", c.len());
    Ok(n)
}

pub fn check_cmd(lang: &str, dir: &str) -> Result<(), String> {
    let mut rep = Report::default();
    check_into(lang, dir, &mut rep)?;
    for f in &rep.fails { println!("FAIL {f}"); }
    for w in &rep.warns { println!("warn {w}"); }
    println!("cdda-check: {} subtitles checked, {} failures, {} warnings", rep.checked, rep.fails.len(), rep.warns.len());
    if rep.fails.is_empty() { Ok(()) } else { Err(format!("{} failures", rep.fails.len())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdda_json_round_trip_and_track_lengths() {
        let dir = std::env::temp_dir().join(format!("snt_cdda_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("cdda.json").to_string_lossy().to_string();
        let mut c = CFile::new();
        c.entry(3).or_default().push(CEntry { start: 12340, end: 15200, h: en_hash("Moscow."), s: 0, t: "Moscou{nl}\"1996\"".into() });
        c.entry(3).or_default().push(CEntry { start: 20000, end: 21000, h: 1, s: 3, t: "b".into() });
        c.entry(19).or_default().push(CEntry { start: 0, end: 500, h: 2, s: 0, t: "c".into() });
        write_cdda(&p, &c).unwrap();
        assert_eq!(read_cdda(&p).unwrap(), c);
        assert_eq!(track_ms(3), Some(132_946));
        assert_eq!(track_ms(1), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
