//! Disc importer tests.
//!  * synthetic discs (always run): wrong disc / region / edition, truncated or malformed images give distinct, clear errors
//!  * real disc (skipped when `work/snatcher.cue` or the reference extraction `assets/` is absent, so CI without data stays green):
//!      - game files (files.pack entries), header.bin, ip.bin equal `assets/raw` byte for byte
//!      - CD-DA equals `assets/raw/audio/trackNN.wav` sample data
//!      - voices, banks, drum kit equal the WAVs of `snt audio-pcm` (exact), manifests equal `pcm.json` / `fm.json`
//!      - FM audio equals the WAVs of `snt audio-fm` (exact; a subset, all 122 with SNT_FULL=1); the dry run equals the full run
//!      - the Redump-style split copy (`work/split`, made by `snt split-cue`) imports to the same bytes as the single bin
//! Run the real-disc tests with `cargo test --release -p sncore --test import` (debug builds work but are slow).
use sncore::audio;
use sncore::import::{self, native::{DirSink, FsIo}, DiscIo, ImportError, Options, Sink};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
fn root() -> PathBuf { Path::new(ROOT).join("..") }

// ---------------------------------------------------------------- synthetic discs

struct MemIo(Vec<Vec<u8>>);
impl DiscIo for MemIo {
    fn read(&mut self, file: usize, offset: u64, buf: &mut [u8]) -> Result<(), String> {
        let f = &self.0[file];
        let end = offset as usize + buf.len();
        if end > f.len() { return Err("past the end of the file".into()); }
        buf.copy_from_slice(&f[offset as usize..end]);
        Ok(())
    }
}
struct NullSink;
impl Sink for NullSink {
    fn write(&mut self, _: &str, _: &[u8]) -> Result<(), String> { Ok(()) }
    fn finish(&mut self, _: &str) -> Result<(), String> { Ok(()) }
    fn progress(&mut self, _: &str, _: u64, _: u64) {}
}

/// n MODE1/2352 sectors; `user` supplies the 2048 bytes of a sector.
fn mode1(n: usize, user: impl Fn(usize) -> Vec<u8>) -> Vec<u8> {
    let mut v = Vec::new();
    for s in 0..n {
        let mut sec = vec![0u8; 2352];
        sec[1..11].fill(0xFF);
        sec[15] = 1;
        let u = user(s);
        sec[16..16 + u.len()].copy_from_slice(&u);
        v.extend_from_slice(&sec);
    }
    v
}
fn sega_sector(title: &str, serial: &str, region: &str) -> Vec<u8> {
    let mut s = vec![b' '; 2048];
    s[0..16].copy_from_slice(b"SEGADISCSYSTEM  ");
    s[0x120..0x120 + title.len()].copy_from_slice(title.as_bytes());
    s[0x150..0x150 + title.len()].copy_from_slice(title.as_bytes());
    s[0x180..0x180 + serial.len()].copy_from_slice(serial.as_bytes());
    s[0x1F0..0x1F0 + region.len()].copy_from_slice(region.as_bytes());
    s[0x30..0x34].copy_from_slice(&0x800u32.to_be_bytes());
    s[0x34..0x38].copy_from_slice(&0x600u32.to_be_bytes());
    s
}
const CUE: &str = "FILE \"d.bin\" BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\n";
fn run(cue: &str, files: Vec<Vec<u8>>) -> Result<import::Summary, ImportError> {
    let sizes: Vec<u64> = files.iter().map(|f| f.len() as u64).collect();
    import::import(cue, &sizes, MemIo(files), &mut NullSink, Options { copy_cdda: false })
}
fn code(r: Result<import::Summary, ImportError>) -> &'static str { r.err().expect("must fail").code }

#[test]
fn synthetic_wrong_discs_give_distinct_errors() {
    let disc = |title: &str, serial: &str, region: &str| mode1(40, |s| if s == 0 { sega_sector(title, serial, region) } else { vec![0; 2048] });
    // not a Sega CD disc at all
    assert_eq!(code(run(CUE, vec![mode1(40, |_| vec![0; 2048])])), "notsega");
    // too short to hold a Sega header
    assert_eq!(code(run(CUE, vec![mode1(3, |_| vec![0; 2048])])), "notsega");
    // another Sega CD game
    assert_eq!(code(run(CUE, vec![disc("SONIC CD", "GM MK-4407 -00", "U")])), "notsnatcher");
    // Snatcher, other edition (other serial)
    let e = run(CUE, vec![disc("SNATCHER", "GM T-95015 -00", "J")]).err().unwrap();
    assert_eq!(e.code, "edition");
    assert!(e.detail.contains("T-95015"), "{}", e.detail);
    // right serial, other region code
    assert_eq!(code(run(CUE, vec![disc("SNATCHER", "GM T-95035 -00", "U")])), "region");
    // right header, but no ISO9660 volume behind it
    let e = run(CUE, vec![disc("SNATCHER", "GM T-95035 -00", "J")]).err().unwrap();
    assert_eq!(e.code, "notsnatcher", "{}", e.detail);
}

#[test]
fn synthetic_bad_images() {
    // raw image without sync patterns (an ISO or a damaged file)
    let plain: Vec<u8> = (0..2352 * 40).map(|i| (i % 251) as u8).collect();
    assert_eq!(code(run(CUE, vec![plain])), "format");
    // size not a multiple of 2352
    assert_eq!(code(run(CUE, vec![vec![0u8; 2352 * 40 + 10]])), "cue");
    // cue / file count mismatch
    let two = "FILE \"a.bin\" BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\nFILE \"b.bin\" BINARY\n TRACK 02 AUDIO\n  INDEX 01 00:00:00\n";
    assert_eq!(code(run(two, vec![mode1(40, |_| vec![0; 2048])])), "cue");
    // not a cue
    assert_eq!(code(run("hello", vec![mode1(40, |_| vec![0; 2048])])), "cue");
    // MODE2 data track
    let m2 = "FILE \"d.bin\" BINARY\n TRACK 01 MODE2/2352\n  INDEX 01 00:00:00\n";
    assert_eq!(code(run(m2, vec![mode1(40, |_| vec![0; 2048])])), "cue");
}

#[test]
fn error_json_is_valid_json_text() {
    let e = ImportError { code: "edition", detail: "serial \"X\"\n".into() };
    assert_eq!(e.to_json(), "{\"code\":\"edition\",\"detail\":\"serial \\\"X\\\"\\u000a\"}");
}

// ---------------------------------------------------------------- real disc

fn have_disc() -> bool {
    let r = root();
    let ok = r.join("work/snatcher.cue").exists() && r.join("work/snatcher.bin").exists() && r.join("assets/raw/files/SP01.BIN").exists() && r.join("assets/audio/web/pcm.json").exists();
    if !ok { eprintln!("skipped: local disc or reference extraction not present"); }
    ok
}

fn import_to(cue: &Path, out: &Path, cd: bool) -> import::Summary {
    let text = fs::read_to_string(cue).unwrap();
    let c = sncore::disc::Cue::parse(&text).unwrap();
    let (io, sizes) = FsIo::open(cue.parent().unwrap(), &c.files).unwrap();
    let _ = fs::remove_dir_all(out);
    let mut sink = DirSink::new(out);
    import::import(&text, &sizes, io, &mut sink, Options { copy_cdda: cd }).unwrap_or_else(|e| panic!("{e}"))
}

fn same_file(a: &Path, b: &Path) {
    let (mut fa, mut fb) = (fs::File::open(a).unwrap(), fs::File::open(b).unwrap());
    assert_eq!(fa.metadata().unwrap().len(), fb.metadata().unwrap().len(), "size of {}", a.display());
    let (mut x, mut y) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    loop {
        let n = fa.read(&mut x).unwrap();
        if n == 0 { break }
        fb.read_exact(&mut y[..n]).unwrap();
        assert!(x[..n] == y[..n], "{} differs from {}", a.display(), b.display());
    }
}
fn wav_samples(p: &Path) -> Vec<i16> {
    let d = fs::read(p).unwrap();
    d[44..].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}
fn json(p: &Path) -> String { fs::read_to_string(p).unwrap() }
/// (name, offset, size) of the entries of `files` in meta.json
fn pack_index(meta: &str) -> Vec<(String, usize, usize)> {
    let mut v = Vec::new();
    let mut rest = meta;
    while let Some(i) = rest.find("{\"name\": \"") {
        let t = &rest[i + 10..];
        let name = t[..t.find('"').unwrap()].to_string();
        let num = |k: &str| -> usize { let j = t.find(k).unwrap() + k.len(); t[j..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap() };
        v.push((name, num("\"off\": "), num("\"size\": ")));
        rest = &t[1..];
    }
    v
}

#[test]
fn single_bin_import_equals_the_reference_extraction() {
    if !have_disc() { return }
    let r = root();
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("import-single");
    let t = std::time::Instant::now();
    let sum = import_to(&r.join("work/snatcher.cue"), &out, true);
    eprintln!("import (native, with CD copy): {:.1} s", t.elapsed().as_secs_f64());
    assert_eq!(sum.digest, import::SNATCHER_USA.digest);

    // game files + boot code, byte for byte (PCMLD_01.BIN becomes voices.pcm: checked below)
    let raw = r.join("assets/raw");
    let meta0 = json(&out.join("meta.json"));
    let pack = fs::read(out.join(import::PACK_STREAM)).unwrap();
    let index = pack_index(&meta0);
    let mut n = 0;
    for (name, off, size) in &index {
        let want = fs::read(raw.join("files").join(name)).unwrap();
        assert_eq!(want.len(), *size, "{name} size");
        assert!(pack[*off..off + size] == want[..], "{name} differs from assets/raw/files");
        n += 1;
    }
    assert_eq!(n, 99);
    let mut on_disk: Vec<String> = fs::read_dir(raw.join("files")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).filter(|n| n != import::PCMLD).collect();
    let mut listed: Vec<String> = index.iter().map(|x| x.0.clone()).collect();
    on_disk.sort(); listed.sort();
    assert_eq!(listed, on_disk, "same set of files as the extraction (PCMLD_01.BIN aside)");
    assert_eq!(index.iter().map(|x| x.2).sum::<usize>(), pack.len(), "the pack holds nothing else");
    drop(pack);
    same_file(&out.join("header.bin"), &raw.join("header.bin"));
    same_file(&out.join("ip.bin"), &raw.join("ip.bin"));

    // CD-DA: the sector data of each track = the sample data of the reference WAV
    let meta = json(&out.join("meta.json"));
    let cdda = fs::read(out.join(import::CDDA_STREAM)).unwrap();
    let mut tracks = 0;
    for tr in 2..=21u32 {
        let key = format!("\"{tr}\": {{\"file\": 0, \"start\": ");
        let at = meta.find(&key).unwrap_or_else(|| panic!("track {tr} not in meta.json")) + key.len();
        let info = &meta[at..meta[at..].find('}').unwrap() + at];
        let num = |k: &str| -> u64 { let i = info.find(k).unwrap() + k.len(); info[i..].split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap() };
        let (bytes, off) = (num("\"bytes\": "), num("\"off\": "));
        let wav = fs::read(raw.join(format!("audio/track{tr:02}.wav"))).unwrap();
        assert_eq!(bytes as usize, wav.len() - 44, "track {tr} length");
        assert!(cdda[off as usize..(off + bytes) as usize] == wav[44..], "track {tr} samples differ");
        tracks += 1;
    }
    assert_eq!(tracks, 20);
    // the engine's CD table (sector counts per track) agrees with the disc, track by track
    for (tr, sectors) in audio::CD_TRACK_SECTORS {
        let wav_len = fs::metadata(raw.join(format!("audio/track{tr:02}.wav"))).unwrap().len() as usize - 44;
        assert!((wav_len / 2352).abs_diff(sectors as usize) <= 1, "track {tr}: disc {} sectors, engine table {sectors}", wav_len / 2352);
    }
    drop(cdda);

    // voices, banks, drum kit: equal to the WAVs of `snt audio-pcm`
    let voices = fs::read(out.join(import::VOICES_STREAM)).unwrap();
    let sfx = fs::read(out.join(import::SFX_STREAM)).unwrap();
    let mf = json(&out.join("pcm.json"));
    let want = audio_pcm_reference(&r);
    let (mut nv, mut nb) = (0, 0);
    for (file, (stream, off, len)) in locate(&mf) {
        let src = if stream == import::VOICES_STREAM { &voices } else { &sfx };
        let bytes = &src[off..off + len];
        let wavp = r.join("assets/audio").join(file.replace(".s8", ".wav"));
        let s16 = wav_samples(&wavp);
        assert_eq!(s16.len(), bytes.len(), "{file}");
        assert!(s16.iter().zip(bytes).all(|(&a, &b)| a == (b as i8 as i16) << 8), "{file} differs");
        if stream == import::VOICES_STREAM { nv += 1 } else { nb += 1 }
    }
    assert_eq!(nv, want.0);
    assert_eq!(nb, want.1);
    eprintln!("voices {nv}, bank/drum segments {nb} equal");
    // voices straight from the extracted PCMLD_01.BIN
    let ld = fs::read(raw.join("files/PCMLD_01.BIN")).unwrap();
    let lt = fs::read(raw.join("files/PCMLT_01.BIN")).unwrap();
    let mut off = 0;
    for v in audio::parse_voice_table(&lt).iter().filter(|v| v.kind != 1) {
        let s = audio::decode_sm8(&ld[v.sector as usize * 2048..(v.sector as usize + v.sectors as usize) * 2048]);
        assert!(s.iter().zip(&voices[off..]).all(|(&a, &b)| a == (b as i8 as i16) << 8), "voice {}", v.id);
        off += s.len();
    }
    assert_eq!(off, voices.len());

    // manifests: same numbers as the pipeline's (file names and places differ)
    let strip = |s: &str| -> Vec<String> {
        s.lines().filter(|l| !l.contains("\"note\"")).map(|l| {
            let mut l = l.to_string();
            while let Some(i) = l.find("\"file\": \"") { let j = l[i + 9..].find('"').unwrap() + i + 9; let end = if l[j..].starts_with("\", ") { j + 3 } else { j + 1 }; l.replace_range(i..end, ""); }
            while let Some(i) = l.find("\"at\": [") { let j = l[i..].find(']').unwrap() + i; let end = if l[j..].starts_with("], ") { j + 3 } else { j + 1 }; l.replace_range(i..end, ""); }
            l
        }).collect()
    };
    assert_eq!(strip(&mf), strip(&json(&r.join("assets/audio/pcm.json"))), "pcm.json");
    let fm_new = parse_fm(&json(&out.join("fm.json")));
    let fm_old = parse_fm(&json(&r.join("assets/audio/fm.json")));
    assert_eq!(fm_new, fm_old, "fm.json (frames, loop points, ticks, kinds of all 122 sounds)");
    assert_eq!(fm_new.len(), 122);
}

/// (voice count, bank + drum segment count) of the reference manifest
fn audio_pcm_reference(r: &Path) -> (usize, usize) {
    let j = json(&r.join("assets/audio/pcm.json"));
    let v = j.matches("\"file\": \"voice/").count();
    let b = j.matches("\"file\": \"bank/").count() + j.matches("\"file\": \"drum/").count();
    (v, b)
}
/// "file" -> (stream, offset, bytes) for every entry of a manifest with an "at" (several entries may share a line)
fn locate(m: &str) -> Vec<(String, (String, usize, usize))> {
    let mut v = Vec::new();
    let mut rest = m;
    while let Some(i) = rest.find("\"file\": \"") {
        let t = &rest[i + 9..];
        let f = &t[..t.find('"').unwrap()];
        let a = t.find("\"at\": [\"").unwrap();
        let u = &t[a + 8..];
        let stream = u[..u.find('"').unwrap()].to_string();
        let nums: Vec<usize> = u[u.find(',').unwrap() + 1..u.find(']').unwrap()].split(',').map(|x| x.trim().parse().unwrap()).collect();
        v.push((f.to_string(), (stream, nums[0], nums[1])));
        rest = &t[a..];
    }
    v
}

/// (id, music, frames, loop_start, loop_end, ticks, loop_kind) per sound, from the one-line-per-sound layout of fm.json
type Fm = Vec<(u32, bool, u64, Option<u64>, u64, u64, Option<String>)>;
fn parse_fm(s: &str) -> Fm {
    let field = |l: &str, k: &str| -> Option<String> {
        let i = l.find(&format!("\"{k}\": "))? + k.len() + 4;
        let t = &l[i..];
        Some(t[..t.find(|c| c == ',' || c == '}').unwrap()].trim().trim_matches('"').to_string())
    };
    s.lines().filter(|l| l.contains("\"frames\"")).map(|l| {
        let id: u32 = l.trim_start().trim_start_matches('"').split('"').next().unwrap().parse().unwrap();
        let num = |k: &str| field(l, k).unwrap().parse::<u64>().unwrap();
        let opt = |k: &str| field(l, k).filter(|x| x != "null");
        (id, field(l, "music").unwrap() == "true", num("frames"), opt("loop_start").map(|x| x.parse().unwrap()), num("loop_end"), num("ticks"), opt("loop_kind"))
    }).collect()
}

#[test]
fn multi_bin_import_equals_single_bin() {
    if !have_disc() { return }
    let r = root();
    let cue = r.join("work/split/Snatcher (USA).cue");
    if !cue.exists() { eprintln!("skipped: no work/split (make it with `snt split-cue work/snatcher.cue work/split \"Snatcher (USA)\"`)"); return }
    let single = Path::new(env!("CARGO_TARGET_TMPDIR")).join("import-single");
    if !single.join("meta.json").exists() { import_to(&r.join("work/snatcher.cue"), &single, true); }
    let multi = Path::new(env!("CARGO_TARGET_TMPDIR")).join("import-multi");
    let sum = import_to(&cue, &multi, true);
    assert_eq!(sum.digest, import::SNATCHER_USA.digest);
    for e in fs::read_dir(&single).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        if name == "meta.json" { continue } // the track table names different files
        same_file(&e.path(), &multi.join(&name));
    }
    // the table of tracks points to files 1..20, one per track, each starting after its 150-sector pregap
    let m = json(&multi.join("meta.json"));
    assert!(m.contains("\"21\": {\"file\": 20, \"start\": 352800"), "{}", &m[m.find("\"cdda\"").unwrap()..][..300]);
    // without the CD copy the image files are only referenced
    let nocd = Path::new(env!("CARGO_TARGET_TMPDIR")).join("import-nocd");
    import_to(&cue, &nocd, false);
    assert!(!nocd.join(import::CDDA_STREAM).exists());
    assert!(json(&nocd.join("meta.json")).contains("\"copied\": false"));
    same_file(&nocd.join("voices.pcm"), &single.join("voices.pcm"));
}

#[test]
fn fm_rendering_equals_the_pipeline_and_the_dry_run() {
    if !have_disc() { return }
    let r = root();
    let raw = r.join("assets/raw");
    let rd = |n: &str| fs::read(raw.join(n)).unwrap();
    let fm = import::FmRenderer::new(&rd("header.bin"), &rd("ip.bin"), &rd("files/FMWR_1.BIN"), &rd("files/PCMLT_01.BIN"), &rd("files/PCMDRMDT.BIN")).unwrap();
    let ids: Vec<u8> = if std::env::var("SNT_FULL").is_ok() { (1..=0x7A).collect() } else { vec![1, 2, 3, 0x38, 0x5A, 0x5F, 0x6E, 0x73] };
    let mut checked = 0;
    for id in ids {
        let (full, music) = fm.render(id);
        let (dry, music2) = fm.dry(id);
        assert_eq!(music, music2);
        assert_eq!((full.out_frames, full.loop_start, full.loop_end, full.frames, full.loop_kind), (dry.out_frames, dry.loop_start, dry.loop_end, dry.frames, dry.loop_kind), "dry run of {id:02x}");
        assert_eq!(full.samples.len() / 2, full.out_frames);
        let wav = r.join(format!("assets/audio/fm/{}_{:02x}.wav", if music { "song" } else { "sfx" }, id));
        let want = wav_samples(&wav);
        assert_eq!(want.len(), full.samples.len(), "{id:02x} length");
        assert!(want == full.samples, "{id:02x}: samples differ from the pipeline's WAV");
        checked += 1;
    }
    eprintln!("{checked} FM sounds equal to assets/audio/fm/*.wav");
}
