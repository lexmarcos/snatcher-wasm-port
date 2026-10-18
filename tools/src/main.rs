//! `snt` – Snatcher reverse-engineering / asset tool.
use sncore::disc::{self, DataTrack, SegaHeader};
use std::fs;
use std::path::{Path, PathBuf};

mod cmds;

fn usage() -> ! {
    eprintln!("usage:
  snt extract <image.bin> <image.cue> <outdir>     carve IP/SP, dump ISO9660 files and CD-DA tracks (WAV)
  snt lzkn-d <in> <out> [offset]                    LZKN1 decompress (from offset)
  snt <other subcommands: see tools/src/cmds.rs>");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage() }
    let r = match args[0].as_str() {
        "extract" if args.len() == 4 => extract(Path::new(&args[1]), Path::new(&args[2]), Path::new(&args[3])),
        _ => cmds::dispatch(&args),
    };
    if let Err(e) = r { eprintln!("error: {e}"); std::process::exit(1) }
}

fn wav_header(data_len: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&2u16.to_le_bytes()); // stereo
    h.extend_from_slice(&44100u32.to_le_bytes());
    h.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    h
}

fn extract(bin: &Path, cue_path: &Path, out: &Path) -> Result<(), String> {
    // `bin` may be any of the files of the cue: the cue names the files, they are looked up next to it (a multi-bin Redump cue works)
    let cue_text = fs::read_to_string(cue_path).map_err(|e| e.to_string())?;
    let cue = disc::Cue::parse(&cue_text)?;
    let dir = cue_path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let names: Vec<String> = if cue.files.len() == 1 && !dir.join(&cue.files[0]).exists() { vec![bin.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()] } else { cue.files.clone() };
    let bin_dir = if cue.files.len() == 1 && !dir.join(&cue.files[0]).exists() { bin.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new(".")) } else { dir };
    let (mut io, sizes) = sncore::import::native::FsIo::open(bin_dir, &names)?;
    let tracks = cue.layout(&sizes)?;
    let t1 = tracks.first().ok_or("no tracks")?;
    if t1.mode != disc::TrackMode::Mode1Raw { return Err("track 1 must be MODE1/2352".into()); }
    let t1_len = t1.sectors;
    let mut raw = vec![0u8; t1_len as usize * disc::RAW_SECTOR];
    use sncore::import::DiscIo;
    io.read(t1.file, t1.start, &mut raw)?;
    let t = DataTrack::from_raw(&raw, t1_len as usize)?;
    drop(raw);
    let h = SegaHeader::parse(t.sector(0))?;
    fs::create_dir_all(out.join("files")).map_err(|e| e.to_string())?;
    fs::create_dir_all(out.join("audio")).map_err(|e| e.to_string())?;
    let mut report = String::new();
    report += &format!("{h:#?}\ntrack1 sectors: {t1_len}\n");
    // IP and SP live at byte offsets inside the first sectors of track 1.
    let ip = &t.data[h.ip_offset as usize..(h.ip_offset + h.ip_size) as usize];
    let sp = &t.data[h.sp_offset as usize..(h.sp_offset + h.sp_size) as usize];
    fs::write(out.join("ip.bin"), ip).map_err(|e| e.to_string())?;
    fs::write(out.join("sp.bin"), sp).map_err(|e| e.to_string())?;
    fs::write(out.join("header.bin"), &t.data[..0x800]).map_err(|e| e.to_string())?;
    report += &format!("IP: offset {:#x} size {:#x} -> ip.bin ({} bytes)\n", h.ip_offset, h.ip_size, ip.len());
    report += &format!("SP: offset {:#x} size {:#x} -> sp.bin ({} bytes)\n", h.sp_offset, h.sp_size, sp.len());
    let files = disc::iso_root(&t)?;
    let mut list = String::from("name\tlba\tsize\tsectors\n");
    for f in &files {
        if f.is_dir { continue }
        let end = f.lba as u64 * 2048 + f.size as u64;
        if f.lba as u32 >= t1_len || end > t.data.len() as u64 {
            return Err(format!("{}: extent out of track ({} + {})", f.name, f.lba, f.size));
        }
        fs::write(out.join("files").join(&f.name), t.read(f.lba, f.size)).map_err(|e| e.to_string())?;
        list += &format!("{}\t{}\t{}\t{}\n", f.name, f.lba, f.size, (f.size + 2047) / 2048);
    }
    fs::write(out.join("filelist.tsv"), &list).map_err(|e| e.to_string())?;
    report += &format!("ISO9660 files: {}\n", files.len());
    // CD-DA tracks: from INDEX 01 to the next track's INDEX 00 (or the end of its file).
    for tr in tracks.iter().filter(|t| t.mode == disc::TrackMode::Audio) {
        let mut pcm = vec![0u8; tr.sectors as usize * disc::RAW_SECTOR];
        io.read(tr.file, tr.start, &mut pcm)?;
        let mut w = wav_header(pcm.len() as u32);
        w.extend_from_slice(&pcm);
        let p: PathBuf = out.join("audio").join(format!("track{:02}.wav", tr.number));
        fs::write(&p, w).map_err(|e| e.to_string())?;
        let first = tr.start / disc::RAW_SECTOR as u64; // sector inside its file (the absolute LBA for a single-bin image)
        report += &format!("track {:02}: lba {}..{} ({:.1} s)\n", tr.number, first, first + tr.sectors as u64, tr.sectors as f64 / 75.0);
    }
    fs::write(out.join("extract_report.txt"), &report).map_err(|e| e.to_string())?;
    print!("{report}");
    Ok(())
}
