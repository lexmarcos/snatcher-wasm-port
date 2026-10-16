//! Voice subtitle files (docs/voice-subtitles.md): `<lang>/voice.json`, the importer and the validator.
//!
//!   {"lang": "pt-BR", "clips": {"77": {"h": "<hash of the English transcript>", "d": "<seconds>", "s": "<speaker number, optional>", "t": "<markup>"}}}
//!
//! Only pt-BR text, clip ids, durations and hashes are committed: the English transcripts stay in `work/voice/transcripts/` (gitignored). The tools read
//! the flat copy `work/voice/transcripts/en.tsv` (written by `scripts/voice-transcribe.py`; the path can be set with the environment variable `VOICE_EN`)
//! to fill and check the hash; without it the hash check is skipped.
use super::i18n::{json_parse, jstr, lang_path, spk_names, font_widths, Report, J};
use sncore::audio::{decode_sm8, fd_to_hz, parse_voice_table, VOICE_FD};
use sncore::i18n as i;
use sncore::vwrap as vw;
use std::collections::{BTreeMap, HashMap};
use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub struct VEntry { pub h: u32, pub d: String, pub s: u8, pub t: String }

pub type VFile = BTreeMap<u16, VEntry>;

pub fn voice_path(lang: &str) -> String { lang_path(lang, "voice") }

pub fn read_voice(path: &str) -> Result<VFile, String> {
    let Ok(txt) = fs::read_to_string(path) else { return Ok(VFile::new()) };
    let J::Obj(top) = json_parse(&txt).map_err(|e| format!("{path}: {e}"))? else { return Err(format!("{path}: not an object")) };
    let mut out = VFile::new();
    for (k, v) in top {
        if k != "clips" { continue; }
        let J::Obj(es) = v else { return Err(format!("{path}: clips is not an object")) };
        for (ko, e) in es {
            let id: u16 = ko.parse().map_err(|_| format!("{path}: bad clip id {ko:?}"))?;
            let J::Obj(f) = e else { return Err(format!("{path}: clip {ko} is not an object")) };
            let (mut h, mut d, mut s, mut t) = (None, None, 0u8, None);
            for (fk, fv) in f {
                match (fk.as_str(), fv) {
                    ("h", J::Str(x)) => h = u32::from_str_radix(&x, 16).ok(),
                    ("d", J::Str(x)) => d = Some(x),
                    ("s", J::Str(x)) => s = x.parse().map_err(|_| format!("{path}: clip {ko}: bad speaker"))?,
                    ("t", J::Str(x)) => t = Some(x),
                    _ => {}
                }
            }
            out.insert(id, VEntry { h: h.ok_or(format!("{path}: clip {ko} has no hash"))?, d: d.ok_or(format!("{path}: clip {ko} has no duration"))?, s, t: t.ok_or(format!("{path}: clip {ko} has no text"))? });
        }
    }
    Ok(out)
}

pub fn write_voice(path: &str, v: &VFile) -> Result<(), String> {
    let mut s = String::from("{\n  \"lang\": \"pt-BR\",\n  \"clips\": {\n");
    let n = v.len();
    for (k, (id, e)) in v.iter().enumerate() {
        let sp = if e.s > 0 { format!(", \"s\": \"{}\"", e.s) } else { String::new() };
        s += &format!("    \"{id}\": {{\"h\": \"{:08x}\", \"d\": \"{}\"{sp}, \"t\": {}}}{}\n", e.h, e.d, jstr(&e.t), if k + 1 < n { "," } else { "" });
    }
    s += "  }\n}\n";
    fs::write(path, s).map_err(|e| format!("{path}: {e}"))
}

/// English transcript data of the clips (from the gitignored work files).
pub struct En { pub speech: bool, pub flag: String, pub text: String }

pub fn read_en() -> HashMap<u16, En> {
    let path = std::env::var("VOICE_EN").unwrap_or("work/voice/transcripts/en.tsv".into());
    let Ok(t) = fs::read_to_string(&path) else { return HashMap::new() };
    t.lines().filter_map(|l| {
        let f: Vec<&str> = l.splitn(5, '\t').collect();
        if f.len() < 5 { return None; }
        Some((f[0].parse().ok()?, En { speech: f[2] == "1", flag: f[3].to_string(), text: f[4].to_string() }))
    }).collect()
}

/// Hash of an English transcript as stored in `h`: FNV-1a (the hash of the SP files) over the whitespace-normalised text.
pub fn en_hash(text: &str) -> u32 { i::hash32(text.split_whitespace().collect::<Vec<_>>().join(" ").as_bytes()) }

/// Clip durations in seconds (samples until the 0xFF marker at the voice rate), by id, for ids that are voices (>= 36).
pub fn durations(dir: &str) -> Result<BTreeMap<u16, f64>, String> {
    let rd = |n: &str| fs::read(format!("{dir}/{n}")).map_err(|e| format!("{dir}/{n}: {e}"));
    let (lt, ld) = (rd("PCMLT_01.BIN")?, rd("PCMLD_01.BIN")?);
    let vrate = fd_to_hz(VOICE_FD).round();
    let mut m = BTreeMap::new();
    for v in parse_voice_table(&lt) {
        if v.kind == 1 || v.id < 36 { continue; }
        let blk = &ld[v.sector as usize * 2048..(v.sector as usize + v.sectors as usize) * 2048];
        m.insert(v.id, decode_sm8(blk).len() as f64 / vrate);
    }
    Ok(m)
}

fn speaker_arg(s: &str, names: &[String]) -> Result<u8, String> {
    let s = s.trim();
    if s.is_empty() || s == "-" || s == "0" { return Ok(0); }
    if let Ok(n) = s.parse::<u8>() { return Ok(n); }
    names.iter().position(|n| n.eq_ignore_ascii_case(s)).map(|p| p as u8 + 1).ok_or(format!("unknown speaker {s:?} (see snt i18n-names)"))
}

/// Text of a patch line: stored unwrapped. The engine wraps and pages at run time for the layout of the screen (`sncore::vwrap`), so a `{nl}` or
/// `{page}` written by the author is kept as a hint (a forced line / page break) and nothing else is added. `|` is a forced line break (`{nl}`); a
/// leading `~` (the old "wrap this for me") is accepted and ignored. Line breaks of the source file's text are not allowed.
pub fn clean_text(t: &str) -> String {
    let t = t.strip_prefix('~').unwrap_or(t);
    t.split_whitespace().collect::<Vec<_>>().join(" ").replace(" | ", "{nl}").replace('|', "{nl}")
}

pub fn import(lang: &str, dir: &str, patch: &str) -> Result<(), String> {
    fs::create_dir_all(lang).map_err(|e| e.to_string())?;
    let txt = fs::read_to_string(patch).map_err(|e| format!("{patch}: {e}"))?;

    let names = spk_names(dir);
    let dur = durations(dir)?;
    let en = read_en();
    let path = voice_path(lang);
    let mut v = read_voice(&path)?;
    let mut count = 0;
    for (ln, line) in txt.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let f: Vec<&str> = line.splitn(3, '\t').collect();
        if f.len() != 3 { return Err(format!("{patch}:{}: want id<TAB>speaker<TAB>text", ln + 1)); }
        let id: u16 = f[0].trim().parse().map_err(|_| format!("{patch}:{}: bad clip id", ln + 1))?;
        let s = speaker_arg(f[1], &names).map_err(|e| format!("{patch}:{}: {e}", ln + 1))?;
        let d = dur.get(&id).ok_or(format!("{patch}:{}: clip {id} is not a voice clip", ln + 1))?;
        let t = clean_text(f[2]);
        let h = en.get(&id).map(|e| en_hash(&e.text)).or_else(|| v.get(&id).map(|x| x.h)).ok_or(format!("{patch}:{}: no English transcript for clip {id} (work/voice/transcripts/en.tsv)", ln + 1))?;
        v.insert(id, VEntry { h, d: format!("{d:.3}"), s, t });
        count += 1;
    }
    write_voice(&path, &v)?;
    println!("imported {count} subtitles ({} in {path})", v.len());
    Ok(())
}

/// Voice section of the pack: id -> subtitle (speaker, markup bytes).
pub fn pack_voice(lang: &str) -> Result<HashMap<u16, i::VoiceSub>, String> {
    let mut m = HashMap::new();
    for (id, e) in read_voice(&voice_path(lang))? {
        m.insert(id, i::VoiceSub { speaker: e.s, text: i::markup_to_bytes(&e.t).map_err(|x| format!("voice {id}: {x}"))? });
    }
    Ok(m)
}

/// Validate `<lang>/voice.json`; returns the number of subtitles checked. Failures and warnings go to `rep`.
pub fn check_into(lang: &str, dir: &str, rep: &mut Report) -> Result<usize, String> {
    let v = read_voice(&voice_path(lang))?;
    if v.is_empty() { return Ok(0); }
    let widths = font_widths(dir)?;
    let names = spk_names(dir);
    let dur = durations(dir)?;
    let en = read_en();
    if en.is_empty() { rep.warns.push("voice: work/voice/transcripts/en.tsv not found, the English hashes are not checked".into()); }
    let (mut hold_n, mut max_cps) = (0usize, 0.0f64);
    let mut layout_stats: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (id, e) in &v {
        rep.checked += 1;
        let t = format!("voice {id}");
        let Some(&real) = dur.get(id) else { rep.fails.push(format!("{t}: not a voice clip of the disc")); continue };
        match e.d.parse::<f64>() { Ok(d) if (d - real).abs() < 0.002 => {}, _ => rep.fails.push(format!("{t}: duration {} does not match the clip ({real:.3} s)", e.d)) }
        if let Some(x) = en.get(id) {
            if en_hash(&x.text) != e.h { rep.fails.push(format!("{t}: the English transcript changed (hash {:08x}, entry {:08x}): translate again", en_hash(&x.text), e.h)); }
            if !x.speech { rep.fails.push(format!("{t}: the clip is flagged as not speech ({}), it gets no subtitle", x.flag)); }
        }
        if e.s == 1 || (e.s >= 2 && names.get(e.s as usize - 1).map_or(true, |n| n.is_empty())) { rep.fails.push(format!("{t}: speaker {} is not a name (see snt i18n-names)", e.s)); }
        let bytes = match i::markup_to_bytes(&e.t) { Ok(b) => b, Err(m) => { rep.fails.push(format!("{t}: {m}")); continue } };
        if bytes.is_empty() { rep.fails.push(format!("{t}: empty subtitle")); continue; }
        if bytes.iter().any(|&c| c == i::AUTO || matches!(c, 0xEC | 0xEE | 0xF0) || c == 0xF4 || (c < 0x20)) { rep.fails.push(format!("{t}: control tokens other than {{nl}}, {{page}} and colours")); }
        // the subtitle is wrapped at run time for the layout of the screen (`sncore::vwrap`): check every layout a clip can get
        let name = if e.s >= 2 { names.get(e.s as usize - 1).filter(|n| !n.is_empty()) } else { None };
        let prefix_px = name.map_or(0, |n| { let nb = i::markup_to_bytes(n).unwrap_or_default(); vw::text_width(&widths, &nb) + vw::text_width(&widths, b": ") });
        let chars: usize = bytes.split(|&c| c == i::PAGE).map(|p| i::voice_glyph_count(p)).sum();
        let play = (real * 60.0).ceil() as u32 + 6;
        // the reading time counts from the first frame the text is on screen (`SETTLE` after the start, when the clip loads a portrait scene)
        let eff = play.saturating_sub(vw::SETTLE);
        let (_, hold) = i::voice_display_frames(eff, chars);
        let secs = (eff + hold) as f64 / 60.0;
        let cps = chars as f64 / secs;
        let need = chars as f64 / i::VOICE_READ_CPS;
        if hold as f64 > i::VOICE_HOLD_MIN as f64 && need > eff as f64 / 60.0 { hold_n += 1; }
        max_cps = max_cps.max(cps);
        if need > secs + 0.02 {
            rep.fails.push(format!("{t}: reading speed {cps:.1} chars/s over the {secs:.1} s of the subtitle ({chars} chars, clip {real:.1} s + {:.1} s hold), limit {:.0}: shorten it", hold as f64 / 60.0, i::VOICE_READ_CPS));
        }
        for (label, lay) in vw::all_layouts(name.is_some()) {
            let pp = if lay.name_inline { prefix_px } else { 0 };
            let pages = vw::paginate(&bytes, &widths, &lay, pp);
            if pages.is_empty() { rep.fails.push(format!("{t}: {label}: no text")); continue; }
            for (k, pg) in pages.iter().enumerate() {
                if pg.len() > lay.max { rep.fails.push(format!("{t}: {label}: page {} has {} lines (limit {})", k + 1, pg.len(), lay.max)); }
                for (n, line) in pg.iter().enumerate() {
                    let w = vw::text_width(&widths, line) + if n == 0 { pp } else { 0 };
                    if w > lay.width() { rep.fails.push(format!("{t}: {label}: line is {w} px wide, limit {}: \"{}\"", lay.width(), i::bytes_to_markup(line))); }
                }
            }
            let st = layout_stats.entry(label).or_default();
            st.0 += pages.len();
            st.1 = st.1.max(pages.len());
            // a page must stay long enough on its own: the pages share the display time in proportion to their text
            let pb: Vec<Vec<u8>> = pages.iter().map(|p| p.join(&i::NL)).collect();
            let starts = i::voice_page_starts(&pb, eff + hold);
            for (k, p) in pb.iter().enumerate() {
                let end = starts.get(k + 1).copied().unwrap_or(eff + hold);
                let ps = (end - starts[k]) as f64 / 60.0;
                let pc = i::voice_glyph_count(p) as f64;
                if pc / ps > i::VOICE_READ_CPS + 0.5 { rep.fails.push(format!("{t}: {label}: page {} shows {pc} chars for {ps:.1} s", k + 1)); }
            }
        }
    }
    let ls: Vec<String> = layout_stats.iter().map(|(l, (n, m))| format!("{l} {n} pages (longest clip {m})")).collect();
    println!("voice-check layouts: {}", ls.join(", "));
    println!("voice-check: {} subtitles, {hold_n} need the hold, fastest {max_cps:.1} chars/s", v.len());
    Ok(v.len())
}

pub fn check_cmd(lang: &str, dir: &str) -> Result<(), String> {
    let mut rep = Report::default();
    check_into(lang, dir, &mut rep)?;
    for f in &rep.fails { println!("FAIL {f}"); }
    for w in &rep.warns { println!("warn {w}"); }
    println!("voice-check: {} subtitles checked, {} failures, {} warnings", rep.checked, rep.fails.len(), rep.warns.len());
    if rep.fails.is_empty() { Ok(()) } else { Err(format!("{} failures", rep.fails.len())) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_text_is_stored_unwrapped_and_keeps_the_authors_hints() {
        assert_eq!(clean_text("~Um   dois\ttrês."), "Um dois três.");
        assert_eq!(clean_text("Um{page}Dois"), "Um{page}Dois");
        assert_eq!(clean_text("Um|dois"), "Um{nl}dois");
        assert_eq!(clean_text("Um | dois"), "Um{nl}dois");
    }

    #[test]
    fn voice_json_round_trip_and_english_hash_ignores_spacing() {
        let dir = std::env::temp_dir().join(format!("snt_voice_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("voice.json").to_string_lossy().to_string();
        let mut v = VFile::new();
        v.insert(77, VEntry { h: en_hash("Hello  there."), d: "3.501".into(), s: 3, t: "Oi{nl}\"você\"{page}Tchau".into() });
        v.insert(5, VEntry { h: 1, d: "0.500".into(), s: 0, t: "a".into() });
        write_voice(&p, &v).unwrap();
        assert_eq!(read_voice(&p).unwrap(), v);
        assert_eq!(en_hash("Hello  there."), en_hash(" Hello there. "));
        assert_ne!(en_hash("Hello there."), en_hash("Hello there!"));
        let _ = fs::remove_dir_all(&dir);
    }
}
