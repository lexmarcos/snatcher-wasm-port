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

fn extract(bin: &Path, cue: &Path, out: &Path) -> Result<(), String> {
    let raw = fs::read(bin).map_err(|e| e.to_string())?;
    let cue = fs::read_to_string(cue).map_err(|e| e.to_string())?;
    let tracks = disc::parse_cue(&cue);
    let total = (raw.len() / disc::RAW_SECTOR) as u32;
    let t1_len = tracks.get(1).map(|t| t.2.unwrap_or(t.3)).unwrap_or(total);
    let t = DataTrack::from_raw(&raw, t1_len as usize)?;
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
    // CD-DA tracks: from INDEX 01 to the next track's INDEX 00 (or end of image).
    for (i, tr) in tracks.iter().enumerate() {
        if tr.1 != "AUDIO" { continue }
        let start = tr.3 as usize;
        let end = tracks.get(i + 1).map(|n| n.2.unwrap_or(n.3)).unwrap_or(total) as usize;
        let pcm = &raw[start * disc::RAW_SECTOR..end * disc::RAW_SECTOR];
        let mut w = wav_header(pcm.len() as u32);
        w.extend_from_slice(pcm);
        let p: PathBuf = out.join("audio").join(format!("track{:02}.wav", tr.0));
        fs::write(&p, w).map_err(|e| e.to_string())?;
        report += &format!("track {:02}: lba {}..{} ({:.1} s)\n", tr.0, start, end, (end - start) as f64 / 75.0);
    }
    fs::write(out.join("extract_report.txt"), &report).map_err(|e| e.to_string())?;
    print!("{report}");
    Ok(())
}
