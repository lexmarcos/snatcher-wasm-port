//! Graphics subcommands (owner: graphics workstream). See docs/re/graphics.md.
//!
//!   snt gfx-lzkn-verify <assets/raw> <lzkn-ref-binary>   compare Rust LZKN1 with the reference tool
//!   snt gfx-scenes <assets/raw> <DATA_xx>                list scene chunk records
//!   snt gfx-export <assets/raw> <outdir> [DATA_xx …]     render scenes/planes to PNG + index.json
//!   snt gfx-anim <assets/raw> <DATA_xx> <scene[,scene…]> <out.png> <frames> <step>   frame strip of the runtime
//!   snt gfx-font <assets/raw> <outdir>                   font sheet (font.png, 16×7 glyphs) + font.json widths
use sncore::gfx::{self, anim::Runtime, Chunk, SubMem, Vdp};
use sncore::lzkn;
use std::fs;
use std::path::{Path, PathBuf};

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "gfx-lzkn-verify" if args.len() == 3 => Some(lzkn_verify(Path::new(&args[1]), Path::new(&args[2]))),
        "gfx-scenes" if args.len() == 3 => Some(scenes(Path::new(&args[1]), &args[2])),
        "gfx-export" if args.len() >= 3 => Some(export(Path::new(&args[1]), Path::new(&args[2]), &args[3..])),
        "gfx-anim" if args.len() == 7 => Some(anim_strip(Path::new(&args[1]), &args[2], &args[3], Path::new(&args[4]), &args[5], &args[6])),
        "gfx-font" if args.len() == 3 => Some(font(Path::new(&args[1]), Path::new(&args[2]))),
        _ => None,
    }
}

fn data_files(raw: &Path) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = fs::read_dir(raw.join("files")).map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("DATA_")).unwrap_or(false))
        .collect();
    v.sort();
    Ok(v)
}

/// Sub-CPU memory with SUBCODE and one DATA file mapped.
pub fn sub_mem(raw: &Path, data: Option<&Path>) -> Result<SubMem, String> {
    let mut m = SubMem::new();
    let sc = fs::read(raw.join("files/SUBCODE.BIN")).map_err(|e| e.to_string())?;
    m.map(gfx::SUBCODE_BASE, sc);
    if let Some(d) = data { m.map(gfx::DATA_BASE, fs::read(d).map_err(|e| e.to_string())?); }
    Ok(m)
}

/// Every LZKN1 blob the game references: VRAM-list entries of every scene of every DATA file
/// (scene chunk type 1, S:0xF6E8) plus the main-CPU blobs passed to M:0xFF1902.
fn all_blobs(raw: &Path) -> Result<Vec<(String, Vec<u8>, u32)>, String> {
    let mut out = vec![];
    for f in data_files(raw)? {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&f).map_err(|e| e.to_string())?;
        let mem = sub_mem(raw, Some(&f))?;
        let hdr = gfx::data_header(&bytes).ok_or(format!("{name}: bad header"))?;
        let mut seen = std::collections::BTreeSet::new();
        for n in 0..gfx::scene_count(&mem, hdr.scene_table) {
            let rec = gfx::scene_addr(&mem, hdr.scene_table, n);
            for c in gfx::scene_chunks(&mem, rec) {
                if let Chunk::VramList { items, .. } = c {
                    for u in items {
                        if u.src >= gfx::DATA_BASE && seen.insert(u.src) {
                            let off = (u.src - gfx::DATA_BASE) as usize;
                            if off < bytes.len() { out.push((name.clone(), bytes[off..].to_vec(), u.src)); }
                        }
                    }
                } else if let Chunk::Objects(objs) = c {
                    // Tile-stream channels started by anim op 0x8A (S:0xFA66 -> subfn_15):
                    // kinds 0/2/3 decompress entry sources with LZKN1 (S:0xE918, 0xE9AC, 0xEA04).
                    for o in objs {
                        for k in 0..512u32 {
                            let e = o.script + 4 * k;
                            let b0 = mem.u8(e);
                            if b0 == 0x80 || b0 == 0x81 || mem.slice_at(e).is_none() { break; }
                            if b0 != 0x8A { continue; }
                            let d = o.script.wrapping_add(mem.i16(e + 2) as i32 as u32);
                            let kind = mem.u8(d) & 0xF;
                            if !(kind == 0 || kind == 2 || kind == 3) { continue; }
                            let mut p = d + 2;
                            for _ in 0..64 {
                                let src = mem.u32(p) & 0xFF_FFFF;
                                if src >= gfx::DATA_BASE && seen.insert(src) {
                                    let off = (src - gfx::DATA_BASE) as usize;
                                    if off + 2 < bytes.len() { out.push((format!("{name}/tilech"), bytes[off..].to_vec(), src)); }
                                }
                                if kind != 0 || mem.u16(p + 10) == 0xFFFF { break; }
                                p += 10;
                            }
                        }
                    }
                }
            }
        }
    }
    // Main-CPU blobs (IP image, loaded at M:0xFF0000): sources at M:0xFF2BC6, 0xFF2C86, 0xFF2DA2, 0xFF2DC6.
    let ip: Vec<u8> = {
        let h = fs::read(raw.join("header.bin")).map_err(|e| e.to_string())?;
        let mut v = h[0x200..0x800].to_vec();
        v.extend(fs::read(raw.join("ip.bin")).map_err(|e| e.to_string())?);
        v
    };
    for a in [0xFF1B9Eu32, 0xFF286E, 0xFF25C8, 0xFF24B4] {
        out.push(("IP".into(), ip[(a - 0xFF0000) as usize..].to_vec(), a));
    }
    Ok(out)
}

fn lzkn_verify(raw: &Path, refbin: &Path) -> Result<(), String> {
    let blobs = all_blobs(raw)?;
    let tmp = std::env::temp_dir().join(format!("snt-lzkn-{}", std::process::id()));
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let (mut ok, mut bad, mut rejected) = (0, 0, 0);
    let mut total = 0usize;
    for (i, (name, data, addr)) in blobs.iter().enumerate() {
        let (mine, used) = match lzkn::decompress(data) {
            Ok(v) => v,
            // Candidates found by scanning anim scripts for op 0x8A are heuristic: a
            // descriptor that is not a real LZKN1 source is reported, not fatal.
            Err(e) if name.ends_with("/tilech") => { eprintln!("rejected candidate {name}@{addr:06x}: {e:?}"); rejected += 1; continue; }
            Err(e) => return Err(format!("{name}@{addr:06x}: {e:?}")),
        };
        let header = u16::from_be_bytes([data[0], data[1]]) as usize;
        // The reference tool decompresses a whole file: give it exactly the consumed bytes.
        let inp = tmp.join(format!("b{i}.lz"));
        let outp = tmp.join(format!("b{i}.unc"));
        fs::write(&inp, &data[..used]).map_err(|e| e.to_string())?;
        let st = std::process::Command::new(refbin).arg("-d").arg(&inp).arg(&outp).status().map_err(|e| e.to_string())?;
        let refd = if st.success() { fs::read(&outp).unwrap_or_default() } else { vec![] };
        if refd == mine && header == mine.len() { ok += 1; } else {
            bad += 1;
            eprintln!("MISMATCH {name}@{addr:06x}: rust {} ref {} header {header}", mine.len(), refd.len());
        }
        total += mine.len();
        let _ = fs::remove_file(&inp);
        let _ = fs::remove_file(&outp);
    }
    let _ = fs::remove_dir_all(&tmp);
    println!("lzkn verify: {ok} identical, {bad} mismatching, {rejected} rejected heuristic candidates, {} blobs, {total} bytes decoded", blobs.len());
    if bad == 0 { Ok(()) } else { Err("mismatches".into()) }
}

fn scenes(raw: &Path, name: &str) -> Result<(), String> {
    let f = raw.join("files").join(format!("{}.BIN", name.trim_end_matches(".BIN")));
    let bytes = fs::read(&f).map_err(|e| e.to_string())?;
    let mem = sub_mem(raw, Some(&f))?;
    let hdr = gfx::data_header(&bytes).ok_or("bad header")?;
    println!("{:?}", hdr);
    for n in 0..gfx::scene_count(&mem, hdr.scene_table) {
        let rec = gfx::scene_addr(&mem, hdr.scene_table, n);
        println!("scene {n} @ {rec:06x}");
        for c in gfx::scene_chunks(&mem, rec) { println!("    {c:?}"); }
    }
    Ok(())
}

fn write_png(path: &Path, w: usize, h: usize, rgba: &[u8]) -> Result<(), String> {
    let f = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

/// Render every scene of the selected DATA files: `<DATA>_<n>.png` = full 256×224 screen after
/// running the scene runtime until its VRAM list finished (+30 frames), `<DATA>_<n>_planes.png`
/// = planes A and B (64×32 cells each, A on top, B below) with the final palette.
fn export(raw: &Path, out: &Path, only: &[String]) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut index = String::from("[\n");
    let mut first = true;
    for f in data_files(raw)? {
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();
        if !only.is_empty() && !only.iter().any(|o| o.trim_end_matches(".BIN") == stem) { continue; }
        let bytes = fs::read(&f).map_err(|e| e.to_string())?;
        let mem = sub_mem(raw, Some(&f))?;
        let Some(hdr) = gfx::data_header(&bytes) else { continue };
        let count = gfx::scene_count(&mem, hdr.scene_table);
        for n in 0..count {
            let rec = gfx::scene_addr(&mem, hdr.scene_table, n);
            let chunks = gfx::scene_chunks(&mem, rec);
            let has_vram = chunks.iter().any(|c| matches!(c, Chunk::VramList { .. }));
            let nobj: usize = chunks.iter().map(|c| if let Chunk::Objects(v) = c { v.len() } else { 0 }).sum();
            let mut vdp = Vdp::default();
            let mut rt = Runtime::new();
            rt.load_scene(&mem, rec);
            let mut frames = 0;
            while (rt.vram_list.is_some() && frames < 600) || frames < 30 {
                rt.tick(&mem, &mut vdp);
                frames += 1;
            }
            let mut fb = vec![0u8; gfx::SCREEN_W * gfx::SCREEN_H * 4];
            vdp.render(&mut fb);
            let png1 = format!("{stem}_{n:03}.png");
            write_png(&out.join(&png1), gfx::SCREEN_W, gfx::SCREEN_H, &fb)?;
            // planes side by side: A (left) | B (right), each 32×64 cells = 256×512
            let pa = vdp.render_plane(vdp.plane_a, 32, 64);
            let pb = vdp.render_plane(vdp.plane_b, 32, 64);
            let mut planes = vec![0u8; 512 * 512 * 4];
            for y in 0..512 {
                planes[y * 2048..y * 2048 + 1024].copy_from_slice(&pa[y * 1024..(y + 1) * 1024]);
                planes[y * 2048 + 1024..(y + 1) * 2048].copy_from_slice(&pb[y * 1024..(y + 1) * 1024]);
            }
            let png2 = format!("{stem}_{n:03}_planes.png");
            write_png(&out.join(&png2), 512, 512, &planes)?;
            let pal: Vec<String> = rt.shadow.iter().map(|c| format!("{c}")).collect();
            if !first { index += ",\n"; }
            first = false;
            index += &format!(
                "{{\"file\":\"{stem}\",\"scene\":{n},\"addr\":{rec},\"vram\":{has_vram},\"objects\":{nobj},\"screen\":\"{png1}\",\"planes\":\"{png2}\",\"palette\":[{}],\"chunks\":{}}}",
                pal.join(","), chunks.len());
        }
        eprintln!("{stem}: {count} scenes");
    }
    index += "\n]\n";
    fs::write(out.join("index.json"), index).map_err(|e| e.to_string())?;
    Ok(())
}

fn font(raw: &Path, out: &Path) -> Result<(), String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mem = sub_mem(raw, None)?;
    let (cols, rows) = (16usize, gfx::FONT_GLYPHS.div_ceil(16));
    let (w, h) = (cols * 10, rows * 10);
    let mut img = vec![0u8; w * h * 4];
    let mut widths = vec![];
    for i in 0..gfx::FONT_GLYPHS {
        let (g, adv) = gfx::font_glyph(&mem, 0x20 + i as u8);
        widths.push(adv.to_string());
        let (ox, oy) = ((i % cols) * 10 + 1, (i / cols) * 10 + 1);
        for (r, bits) in g.iter().enumerate() {
            for x in 0..8 {
                let on = bits & (0x80 >> x) != 0;
                let o = ((oy + r) * w + ox + x) * 4;
                let v = if on { [255, 255, 255, 255] } else if x < adv as usize { [40, 40, 90, 255] } else { [0, 0, 0, 255] };
                img[o..o + 4].copy_from_slice(&v);
            }
        }
    }
    write_png(&out.join("font.png"), w, h, &img)?;
    fs::write(out.join("font.json"), format!("{{\"first\":32,\"count\":{},\"widths\":[{}]}}\n", gfx::FONT_GLYPHS, widths.join(","))).map_err(|e| e.to_string())?;
    println!("font: {} glyphs", gfx::FONT_GLYPHS);
    Ok(())
}

/// Frame strip through the full [`gfx::Display`] pipeline. `scenes` is a comma list of
/// tokens: `N` = load scene N, `fxN` = request screen effect N (scroll_mode), `txt` = draw a
/// test text into the text bitmap (tiles 0-254 + nametable 0xEC80, as S:0x13D4E), `wN` = wait N frames.
fn anim_strip(raw: &Path, name: &str, scenes: &str, out: &Path, frames: &str, step: &str) -> Result<(), String> {
    let f = raw.join("files").join(format!("{}.BIN", name.trim_end_matches(".BIN")));
    let mem = sub_mem(raw, Some(&f))?;
    let frames: usize = frames.parse().map_err(|_| "frames")?;
    let step: usize = step.parse().map_err(|_| "step")?;
    let mut d = gfx::Display::new();
    for tok in scenes.split(',') {
        if let Some(n) = tok.strip_prefix("fx") { d.sub.scroll_mode(n.parse().map_err(|_| "fx")?); }
        else if tok == "txt" { test_text(&mem, &mut d.vdp); d.sub.shadow[15] = 0x0EEE; d.sub.shadow[64 + 15] = 0x0EEE; }
        else if let Some(n) = tok.strip_prefix('w') { for _ in 0..n.parse::<usize>().map_err(|_| "w")? { d.frame(&mem); } }
        else if let Some(n) = tok.strip_prefix("split") { d.sub.split_req = n.parse().map_err(|_| "split")?; }
        else { d.load_scene(&mem, tok.parse().map_err(|_| "scene")?); }
    }
    let shots = frames / step;
    let cols = 4usize;
    let rows = shots.div_ceil(cols);
    let (w, h) = (cols * gfx::SCREEN_W, rows * gfx::SCREEN_H);
    let mut img = vec![0u8; w * h * 4];
    let mut fb = vec![0u8; gfx::SCREEN_W * gfx::SCREEN_H * 4];
    for k in 0..shots {
        for _ in 0..step { d.frame(&mem); }
        d.render(&mut fb);
        let (ox, oy) = ((k % cols) * gfx::SCREEN_W, (k / cols) * gfx::SCREEN_H);
        for y in 0..gfx::SCREEN_H {
            let o = ((oy + y) * w + ox) * 4;
            img[o..o + gfx::SCREEN_W * 4].copy_from_slice(&fb[y * gfx::SCREEN_W * 4..(y + 1) * gfx::SCREEN_W * 4]);
        }
    }
    for e in d.sub.events.iter().take(20) { eprintln!("event {e:?}"); }
    write_png(out, w, h, &img)
}

/// Draw a test string into the 4bpp text bitmap like S:0x13E9C (colour 0xF, tiles row-major,
/// 32 tiles per row) and write the S:0x13D4E nametable (0x8000+i) at VRAM 0xEC80.
fn test_text(mem: &SubMem, vdp: &mut Vdp) {
    for i in 0..256u16 {
        let e = 0x8000u16 + if i < 255 { i } else { 0 };
        vdp.write_vram(0xEC80 + 2 * i, &e.to_be_bytes());
    }
    let lines = ["THE QUICK BROWN FOX JUMPS OVER", "the lazy dog. Snatcher text test", "Line three: 0123456789 !?"];
    let (mut x, mut y) = (0x18usize, 2usize);
    for l in lines {
        for ch in l.bytes() {
            let (g, adv) = gfx::font_glyph(mem, ch);
            for (r, bits) in g.iter().enumerate() {
                for c in 0..8 {
                    if bits & (0x80 >> c) == 0 { continue; }
                    let (px, py) = (x + c, y + r);
                    let tile = (py / 8) * 32 + px / 8;
                    let a = tile * 32 + (py % 8) * 4 + (px % 8) / 2;
                    if a >= 0x1FE0 { continue; }
                    let b = &mut vdp.vram[a];
                    *b = if px % 2 == 0 { (*b & 0x0F) | 0xF0 } else { (*b & 0xF0) | 0x0F };
                }
            }
            x += adv as usize;
        }
        x = 0x18; y += 9;
    }
}
