//! Actions-workstream subcommands (owner: actions workstream). See docs/re/actions.md.
//!
//!   snt act-names <SUBCODE.BIN>                                  list speaker names
//!   snt act-message <SUBCODE.BIN> <SPxx.BIN> <text_off_hex> <spk_hex> <out_prefix>
//!        run the message box (sequencer cmd 08) to completion, pressing confirm on every
//!        page, and write one PNG per page (256x64 text window, default text palette)
use sncore::actions::{speaker_names, Font, MessageBox, MsgEvent, TextWindow, WIN_H, WIN_W};
use std::fs;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "act-names" if args.len() == 2 => Some(names(&args[1])),
        "act-message" if args.len() == 6 => Some(message(&args[1..])),
        _ => None,
    }
}

fn names(sc: &str) -> Result<(), String> {
    let sc = fs::read(sc).map_err(|e| e.to_string())?;
    for (i, (c, n)) in speaker_names(&sc).iter().enumerate() {
        println!("{i:2} colour {c} {}", String::from_utf8_lossy(n));
    }
    Ok(())
}

/// Default text palette written by subfn_17 (S:0xF600): CRAM words for indices 0..7.
const CRAM: [u16; 8] = [0x000, 0xEEE, 0x0EE, 0x86E, 0xEE0, 0xE42, 0x4E0, 0x000];

fn rgb(w: u16) -> [u8; 3] {
    let c = |v: u16| ((v & 7) * 36) as u8;
    [c(w >> 1), c(w >> 5), c(w >> 9)]
}

fn save_png(path: &str, win: &TextWindow) -> Result<(), String> {
    let f = fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(f, WIN_W as u32, WIN_H as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let data: Vec<u8> = win.pix.iter().flat_map(|&p| rgb(CRAM[(p & 7) as usize])).collect();
    w.write_image_data(&data).map_err(|e| e.to_string())
}

fn message(a: &[String]) -> Result<(), String> {
    let sc = fs::read(&a[0]).map_err(|e| e.to_string())?;
    let sp = fs::read(&a[1]).map_err(|e| e.to_string())?;
    let off = u16::from_str_radix(&a[2], 16).map_err(|e| e.to_string())?;
    let spk = u16::from_str_radix(&a[3], 16).map_err(|e| e.to_string())?;
    let text = sp.get(0x3800..).ok_or("script file too short")?;
    let font = Font::from_subcode(&sc);
    let names = speaker_names(&sc);
    let mut m = MessageBox::new(spk, off, 0);
    let mut win = TextWindow::new();
    let mut page = 0;
    for frame in 0..100_000 {
        let confirm = m.state == 5;
        if confirm {
            save_png(&format!("{}_p{}.png", a[4], page), &win)?;
            println!("page {page} at frame {frame}, next text offset {:#x}", m.pos);
            page += 1;
        }
        let ev = m.tick(&mut win, text, &names, confirm);
        win.tick(&font);
        if ev == MsgEvent::Done { return Ok(()); }
    }
    Err("message did not finish".into())
}
