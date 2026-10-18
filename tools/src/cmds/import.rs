//! Disc importer commands (the same code the browser runs in a worker).
//!   snt import-disc <image.cue> <outdir> [--no-cd]   run the importer natively into a directory of streams (see core/src/import.rs)
//!   snt split-cue <image.cue> <outdir> <name>        split a single-bin image into one .bin per track + a Redump-style cue (for tests)
use sncore::disc::{self, Cue};
use sncore::import::{self, native::{DirSink, FsIo}};
use std::fs;
use std::path::Path;
use std::time::Instant;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "import-disc" if args.len() >= 3 => Some(import_disc(Path::new(&args[1]), Path::new(&args[2]), args.iter().any(|a| a == "--no-cd"))),
        "split-cue" if args.len() == 4 => Some(split_cue(Path::new(&args[1]), Path::new(&args[2]), &args[3])),
        _ => None,
    }
}

fn import_disc(cue_path: &Path, out: &Path, no_cd: bool) -> Result<(), String> {
    let text = fs::read_to_string(cue_path).map_err(|e| e.to_string())?;
    let cue = Cue::parse(&text)?;
    let dir = cue_path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let (io, sizes) = FsIo::open(dir, &cue.files)?;
    let mut sink = DirSink::new(out);
    let t = Instant::now();
    let r = import::import(&text, &sizes, io, &mut sink, import::Options { copy_cdda: !no_cd }).map_err(|e| e.to_string())?;
    println!("imported in {:.1} s, digest {}", t.elapsed().as_secs_f64(), r.digest);
    for (phase, _, total) in &sink.log { println!("  phase {phase} (total {total})"); }
    Ok(())
}

/// One file per track: track N's file holds sectors [INDEX 00 (or INDEX 01), next track's INDEX 00 (or INDEX 01)), so the pregap travels with the track
/// as in a Redump dump, and the cue says INDEX 00 00:00:00 / INDEX 01 <pregap>.
fn split_cue(cue_path: &Path, out: &Path, name: &str) -> Result<(), String> {
    let text = fs::read_to_string(cue_path).map_err(|e| e.to_string())?;
    let cue = Cue::parse(&text)?;
    if cue.files.len() != 1 { return Err("split-cue needs a single-bin cue".into()); }
    let dir = cue_path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let bin = fs::File::open(dir.join(&cue.files[0])).map_err(|e| e.to_string())?;
    let total = bin.metadata().map_err(|e| e.to_string())?.len() / disc::RAW_SECTOR as u64;
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    use std::io::{Read, Seek, SeekFrom, Write};
    let mut bin = bin;
    let (mut names, mut pre) = (Vec::new(), Vec::new());
    for (i, t) in cue.tracks.iter().enumerate() {
        let start = t.index00.unwrap_or(t.index01) as u64;
        let end = cue.tracks.get(i + 1).map(|n| n.index00.unwrap_or(n.index01) as u64).unwrap_or(total);
        let fname = format!("{name} (Track {:02}).bin", t.number);
        bin.seek(SeekFrom::Start(start * disc::RAW_SECTOR as u64)).map_err(|e| e.to_string())?;
        let mut w = fs::File::create(out.join(&fname)).map_err(|e| e.to_string())?;
        let mut left = (end - start) * disc::RAW_SECTOR as u64;
        let mut buf = vec![0u8; 1 << 20];
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            bin.read_exact(&mut buf[..n]).map_err(|e| e.to_string())?;
            w.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            left -= n as u64;
        }
        names.push(fname);
        pre.push(t.index01 - start as u32);
    }
    fs::write(out.join(format!("{name}.cue")), cue.redump_text(&names, &pre)).map_err(|e| e.to_string())?;
    println!("wrote {} files and {name}.cue to {}", names.len(), out.display());
    Ok(())
}
