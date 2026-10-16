//! pt-BR picture patches (docs/i18n-graphics.md): `<lang>/gfx.tsv` -> the `GfxPatch` inside the pack, and the checker.
//!
//!   snt i18n-gfx-check <lang-dir> <assets/raw>   every blob / raw run of gfx.tsv still matches the user's game data (hash, length, original bytes)
//!   snt i18n-gfx-info  <lang-dir>                 what the patch changes (blobs, runs, bytes)
use sncore::gfxpatch::{fnv64, FileRun, GfxPatch};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "i18n-gfx-check" if args.len() == 3 => Some(check(&args[1], Path::new(&args[2]))),
        "i18n-gfx-info" if args.len() == 2 => Some(info(&args[1])),
        _ => None,
    }
}

/// One `blob` line of gfx.tsv.
pub struct BlobLine { pub file: String, pub addr: u32, pub hash: u64, pub len: u32, pub off: u32, pub bytes: Vec<u8> }
pub struct FileLine { pub file: String, pub off: u32, pub orig: Vec<u8>, pub new: Vec<u8> }

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 { return Err(format!("odd hex length: {s}")); }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())).collect()
}

pub fn read_tsv(lang: &str) -> Result<(Vec<BlobLine>, Vec<FileLine>), String> {
    let path = format!("{lang}/gfx.tsv");
    let Ok(txt) = fs::read_to_string(&path) else { return Ok((vec![], vec![])) };
    let (mut blobs, mut files) = (vec![], vec![]);
    for (n, l) in txt.lines().enumerate() {
        let l = l.trim_end();
        if l.is_empty() || l.starts_with('#') { continue; }
        let f: Vec<&str> = l.split('\t').collect();
        let err = |m: &str| format!("{path}:{}: {m}", n + 1);
        match f.first().copied() {
            Some("blob") if f.len() == 7 => blobs.push(BlobLine {
                file: f[1].to_string(),
                addr: u32::from_str_radix(f[2].trim_start_matches("0x"), 16).map_err(|e| err(&e.to_string()))?,
                hash: u64::from_str_radix(f[3], 16).map_err(|e| err(&e.to_string()))?,
                len: f[4].parse().map_err(|_| err("length"))?,
                off: f[5].parse().map_err(|_| err("offset"))?,
                bytes: unhex(f[6]).map_err(|e| err(&e))?,
            }),
            Some("file") if f.len() == 5 => {
                let (orig, new) = (unhex(f[3]).map_err(|e| err(&e))?, unhex(f[4]).map_err(|e| err(&e))?);
                if orig.len() != new.len() { return Err(err("orig and new differ in length")); }
                files.push(FileLine { file: f[1].to_string(), off: f[2].parse().map_err(|_| err("offset"))?, orig, new });
            }
            _ => return Err(err("unknown line")),
        }
    }
    Ok((blobs, files))
}

/// The patch the engine reads (inside the pack).
pub fn patch_for(lang: &str) -> Result<GfxPatch, String> {
    let (blobs, files) = read_tsv(lang)?;
    let mut g = GfxPatch::default();
    for b in blobs {
        let runs = g.blobs.entry((b.hash, b.len)).or_default();
        if (b.off as usize) + b.bytes.len() > b.len as usize { return Err(format!("{}: run beyond the blob", b.file)); }
        runs.push((b.off, b.bytes));
    }
    for f in files { g.files.entry(f.file).or_default().push(FileRun { off: f.off, orig: f.orig, new: f.new }); }
    Ok(g)
}

fn info(lang: &str) -> Result<(), String> {
    let (blobs, files) = read_tsv(lang)?;
    let mut per: BTreeMap<(String, u32, u64), (usize, usize)> = BTreeMap::new();
    for b in &blobs { let e = per.entry((b.file.clone(), b.addr, b.hash)).or_default(); e.0 += 1; e.1 += b.bytes.len(); }
    for ((f, a, h), (r, n)) in &per { println!("blob {f} {a:#x} hash {h:016x}: {r} runs, {n} bytes"); }
    for f in &files { println!("file {} @{}: {} bytes", f.file, f.off, f.new.len()); }
    Ok(())
}

fn check(lang: &str, raw: &Path) -> Result<(), String> {
    let (blobs, files) = read_tsv(lang)?;
    let mut bad = 0;
    let mut seen: BTreeMap<(String, u32), (u64, u32)> = BTreeMap::new();
    for b in &blobs {
        let key = (b.file.clone(), b.addr);
        if !seen.contains_key(&key) {
            let data = fs::read(raw.join("files").join(&b.file)).map_err(|e| format!("{}: {e}", b.file))?;
            let off = b.addr.checked_sub(sncore::gfx::DATA_BASE).ok_or("address below the DATA base")? as usize;
            let (out, _) = sncore::lzkn::decompress(data.get(off..).ok_or("address beyond the file")?).map_err(|e| format!("{} {:#x}: {e:?}", b.file, b.addr))?;
            seen.insert(key.clone(), (fnv64(&out), out.len() as u32));
        }
        let (h, l) = seen[&key];
        if h != b.hash || l != b.len { println!("FAIL blob {} {:#x}: decoded original is {h:016x}/{l}, patch expects {:016x}/{}", b.file, b.addr, b.hash, b.len); bad += 1; }
        if b.off as usize + b.bytes.len() > b.len as usize { println!("FAIL blob {} {:#x}: run beyond the blob", b.file, b.addr); bad += 1; }
    }
    for f in &files {
        let data = fs::read(raw.join("files").join(&f.file)).map_err(|e| format!("{}: {e}", f.file))?;
        let o = f.off as usize;
        if data.get(o..o + f.orig.len()) != Some(&f.orig[..]) { println!("FAIL file {} @{}: the bytes are not the expected ones", f.file, f.off); bad += 1; }
    }
    // the pack round trip
    let g = patch_for(lang)?;
    if GfxPatch::parse(&g.build())? != g { println!("FAIL pack round trip"); bad += 1; }
    println!("gfx patch: {} blob runs in {} blobs, {} raw runs, {} failures", blobs.len(), seen.len(), files.len(), bad);
    if bad > 0 { Err(format!("{bad} failures")) } else { Ok(()) }
}
