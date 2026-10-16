//! Voice inventory (docs/voice-inventory.md): every place where a voice clip is started, whether a message box
//! string is shown with it, and how long the clip is.
//!
//!   snt voice-inventory <files-dir> [--csv triggers.csv] [--clips-csv clips.csv] [--scenes-csv scenes.csv] [--md tables.md]
//!       [--probes list.txt] [--probe-results file ...] [--icons docs/voice-icons.tsv] [--graphics-text docs/voice-graphics-text.tsv]
//!   snt voice-export <files-dir> <out-dir> [--clips-csv work/voice/voice-clips.csv] [--rate 16000] [--all]
//!       WAV (16-bit mono) of every script-started clip + <out-dir>/index.json (id -> seconds, samples, rate); for the Whisper pass
//!   snt voice-import <lang-dir> <files-dir> <patch.tsv>   merge `id<TAB>speaker<TAB>text` lines into <lang-dir>/voice.json (`~text` = wrap + paginate; hash and duration filled in)
//!   snt voice-check  <lang-dir> <files-dir>               validate <lang-dir>/voice.json (also run by `i18n-check ... all`): characters, widths, lines per page, reading speed
//!   (`--probes` writes the entry points for engine/examples/voice_probe.rs, `--probe-results` reads its output; see docs/voice-inventory.md)
//!
//! Static analysis of the 39 script files (SPnn.BIN): each `CMD0D #id` node (script op 0x04, the only path that starts a
//! voice: sequencer command 0x0D -> pcm_play_voice S:0x154F8; ids 1..35 select a PCM bank and are silent) is followed
//! through the script (branches, GOSUB, GOTO/JUMP/CALL, link continuations) up to the first SAY or the next voice.
//! Durations come from PCMLT_01/PCMLD_01 (samples until the 0xFF end marker at 16276 Hz, docs/re/audio.md section 1).
//! No game text is printed; strings are referred to by offset.
use sncore::audio::{decode_sm8, fd_to_hz, parse_voice_table, VOICE_FD};
use sncore::script::{self as sc, Node, Operand};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;

use super::voice_sub as sub;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "voice-inventory" if args.len() >= 2 => Some(inventory(&args[1], &args[2..])),
        "voice-export" if args.len() >= 3 => Some(export(&args[1], &args[2], &args[3..])),
        "voice-import" if args.len() == 4 => Some(sub::import(&args[1], &args[2], &args[3])),
        "voice-check" if args.len() == 3 => Some(sub::check_cmd(&args[1], &args[2])),
        _ => None,
    }
}

// ---------------------------------------------------------------- export for transcription

/// Linear resampling of 16-bit mono (the clips are 16276 Hz; Whisper wants 16 kHz).
fn resample(s: &[i16], from: f64, to: f64) -> Vec<i16> {
    if (from - to).abs() < 0.5 || s.is_empty() { return s.to_vec(); }
    let n = (s.len() as f64 * to / from).floor() as usize;
    (0..n).map(|i| {
        let t = i as f64 * from / to;
        let k = t.floor() as usize;
        let f = t - k as f64;
        let a = s[k] as f64;
        let b = s[(k + 1).min(s.len() - 1)] as f64;
        (a + (b - a) * f).round() as i16
    }).collect()
}

/// Ids of the clips that a script starts: the rows with `triggers > 0` of `voice-clips.csv` (written by `voice-inventory`).
pub fn started_clips(csv: &str) -> Result<Vec<u16>, String> {
    let t = fs::read_to_string(csv).map_err(|e| format!("{csv}: {e} (run snt voice-inventory --clips-csv first, or pass --all)"))?;
    let mut lines = t.lines();
    let head: Vec<&str> = lines.next().ok_or("empty csv")?.split(',').collect();
    let (ci, ti) = (head.iter().position(|h| *h == "id").ok_or("no id column")?, head.iter().position(|h| *h == "triggers").ok_or("no triggers column")?);
    let mut v = vec![];
    for l in lines {
        let f: Vec<&str> = l.split(',').collect();
        let (Some(id), Some(tr)) = (f.get(ci).and_then(|x| x.parse::<u16>().ok()), f.get(ti).and_then(|x| x.parse::<u32>().ok())) else { continue };
        if tr > 0 && id >= 36 { v.push(id); }
    }
    v.sort();
    Ok(v)
}

fn export(dir: &str, out: &str, opts: &[String]) -> Result<(), String> {
    let mut csv = "work/voice/voice-clips.csv".to_string();
    let mut rate = 16000u32;
    let mut all = false;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match o.as_str() {
            "--clips-csv" => csv = it.next().ok_or("--clips-csv needs a path")?.clone(),
            "--rate" => rate = it.next().and_then(|x| x.parse().ok()).ok_or("--rate needs a number")?,
            "--all" => all = true,
            x => return Err(format!("unknown option {x}")),
        }
    }
    let rd = |n: &str| fs::read(format!("{dir}/{n}")).map_err(|e| format!("{dir}/{n}: {e}"));
    let lt = rd("PCMLT_01.BIN")?;
    let ld = rd("PCMLD_01.BIN")?;
    let vrate = fd_to_hz(VOICE_FD);
    let table = parse_voice_table(&lt);
    let want: Option<BTreeSet<u16>> = if all { None } else { Some(started_clips(&csv)?.into_iter().collect()) };
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut idx = String::from("{\n");
    let mut n = 0;
    let mut total = 0.0;
    for v in &table {
        if v.kind == 1 || v.id < 36 { continue; }
        if let Some(w) = &want { if !w.contains(&v.id) { continue; } }
        let blk = &ld[v.sector as usize * 2048..(v.sector as usize + v.sectors as usize) * 2048];
        let s = decode_sm8(blk);
        let secs = s.len() as f64 / vrate;
        let r = resample(&s, vrate, rate as f64);
        fs::write(format!("{out}/v{:04}.wav", v.id), sncore::audio::wav_bytes(&r, rate, 1)).map_err(|e| e.to_string())?;
        if n > 0 { idx.push_str(",\n"); }
        idx.push_str(&format!("  \"{}\": {{\"seconds\": {:.3}, \"samples\": {}, \"src_samples\": {}, \"rate\": {}}}", v.id, secs, r.len(), s.len(), rate));
        n += 1;
        total += secs;
    }
    idx.push_str("\n}\n");
    fs::write(format!("{out}/index.json"), idx).map_err(|e| e.to_string())?;
    println!("exported {n} clips, {:.1} min, {rate} Hz mono, to {out}", total / 60.0);
    Ok(())
}

// ---------------------------------------------------------------- event trees

#[derive(Debug, Clone)]
pub enum Ev {
    Voice { id: u16, pc: usize },
    Say { spk: u8, text: u16, pc: usize },
    WaitVoice,
    #[allow(dead_code)]
    Wait(u16),
    Scene(u16),
    Goto(u16),
    Jump(u16, u16),
    Call(u16, u16),
    Return,
    /// GOSUB (the target is also expanded inline as a branch)
    Gosub(u16),
    /// alternatives (IF/ELSE, WHEN, RANDOM, CYCLE, SWITCH, GOSUB expansion)
    Branch(Vec<&'static [Ev]>),
    /// a menu item: its body only runs after the player picked it
    Choice(u16, &'static [Ev]),
}

struct SpFile {
    sp: u8,
    script: &'static sc::Script,
    by_off: HashMap<usize, &'static Node>,
    roots: Vec<usize>,
    raw: Vec<u8>,
}

struct Ctx {
    files: Vec<SpFile>,
    trees: std::cell::RefCell<HashMap<(usize, usize, bool), &'static [Ev]>>,
    extra: std::cell::RefCell<HashMap<(usize, usize), &'static Node>>,
    ends: std::cell::RefCell<HashMap<(usize, usize), usize>>,
}

fn file_label(n: &Node) -> Option<(u16, u16)> {
    if let Some(Operand::Node(t)) = n.operands.first() {
        if let (Some(Operand::Res(f)), Some(Operand::Label(l))) = (t.operands.first(), t.operands.get(1)) { return Some((*f, *l)); }
    }
    None
}

impl Ctx {
    fn fi(&self, id: u16) -> Option<usize> { sc::sp_number(id).map(|n| n as usize).filter(|&n| n < self.files.len()) }

    /// events of the node at `off` in file `fi`, including its link continuation
    fn tree(&self, fi: usize, off: usize, gosub: bool) -> &'static [Ev] {
        if let Some(t) = self.trees.borrow().get(&(fi, off, gosub)) { return t; }
        let mut out = Vec::new();
        if let Some(n) = self.files[fi].by_off.get(&off) { self.emit(fi, *n, &mut out, if gosub { 3 } else { 0 }); }
        let t: &'static [Ev] = Box::leak(out.into_boxed_slice());
        self.trees.borrow_mut().insert((fi, off, gosub), t);
        t
    }


    /// node at `off` (parsed positions first; nodes the sweep parsed at a wrong position are parsed on demand)
    fn get(&self, fi: usize, off: usize) -> Option<&'static Node> {
        if let Some(n) = self.files[fi].by_off.get(&off) { return Some(*n); }
        if let Some(n) = self.extra.borrow().get(&(fi, off)) { return Some(*n); }
        let n = sc::parse_node(&self.files[fi].raw, off).ok()?;
        let n: &'static Node = Box::leak(Box::new(n));
        self.extra.borrow_mut().insert((fi, off), n);
        Some(n)
    }

    /// Position after the node as the VM's skip routine S:0x1699A computes it: a node with a link is skipped by skipping its
    /// link target (so the end of a list is the end of its link chain), inline children are skipped recursively.
    fn vm_end(&self, fi: usize, n: &'static Node) -> usize {
        if let Some(e) = self.ends.borrow().get(&(fi, n.offset)) { return *e; }
        let r = if let Some(l) = n.link {
            match self.get(fi, l as usize) { Some(t) if t.offset != n.offset => self.vm_end(fi, t), _ => n.end }
        } else {
            let info = sc::op_info(n.op).unwrap();
            let mut s = n.offset + 1;
            for t in info.types() {
                match t {
                    sc::ty::NONE | sc::ty::END | sc::ty::LINK => {}
                    sc::ty::NODE => match self.get(fi, s) { Some(c) => s = self.vm_end(fi, c), None => return n.end },
                    _ => s += 2,
                }
            }
            s
        };
        self.ends.borrow_mut().insert((fi, n.offset), r);
        r
    }

    /// The node-typed operand slots of `n`, positioned the way the VM walks them (the sweep's own `operands` can be wrong for
    /// the second child when the first child has a link).
    fn kids(&self, fi: usize, n: &'static Node) -> Vec<&'static Node> {
        let info = sc::op_info(n.op).unwrap();
        let mut s = n.offset + 1 + if info.has_link() { 2 } else { 0 };
        let mut v = vec![];
        for t in info.types() {
            match t {
                sc::ty::NONE | sc::ty::END | sc::ty::LINK => {}
                sc::ty::NODE => match self.get(fi, s) { Some(c) => { v.push(c); s = self.vm_end(fi, c); } None => break },
                _ => s += 2,
            }
        }
        v
    }

    fn sub(&self, fi: usize, n: &'static Node, gd: u8) -> &'static [Ev] {
        let mut v = Vec::new();
        self.emit(fi, n, &mut v, gd);
        Box::leak(v.into_boxed_slice())
    }

    fn link_tree(&self, fi: usize, l: u16, gd: u8) -> &'static [Ev] {
        match self.files[fi].by_off.get(&(l as usize)) { Some(n) => self.sub(fi, *n, gd), None => &[] }
    }

    /// elements of an exclusive list (RANDOM / CYCLE / SWITCH): cons cells 0x25 (car, cdr), 0x26 (car, link), 0x27 (car)
    fn elems(&self, fi: usize, n: &'static Node, out: &mut Vec<&'static Node>) {
        match n.op {
            0x25 | 0x26 | 0x27 => {
                if let Some(c) = self.kids(fi, n).first() { out.push(*c); }
                match n.op {
                    0x25 => if let Some(r) = self.kids(fi, n).get(1) { self.elems(fi, *r, out) },
                    0x26 => if let Some(l) = n.link { if let Some(r) = self.files[fi].by_off.get(&(l as usize)) { self.elems(fi, *r, out) } },
                    _ => {}
                }
            }
            _ => out.push(n),
        }
    }

    fn emit(&self, fi: usize, n: &'static Node, out: &mut Vec<Ev>, gd: u8) {
        let nodes = |o: &'static Operand| -> Option<&'static Node> { if let Operand::Node(c) = o { Some(&**c) } else { None } };
        match n.op {
            0x04 => if let Some(Operand::Res(v)) = n.operands.first() { out.push(Ev::Voice { id: *v, pc: n.offset }); },
            0x46 => out.push(Ev::WaitVoice),
            0x4A => if let Some(Operand::Imm(v)) = n.operands.first() { out.push(Ev::Wait(*v)); },
            0x43 | 0x44 => {
                let (spk, text) = match n.operands.first() {
                    Some(Operand::Node(t)) if t.op != 0x44 => {
                        let s = match t.operands.first() { Some(Operand::Imm(v)) | Some(Operand::Res(v)) => *v as u8, _ => 0 };
                        (s, t.operands.get(1).cloned())
                    }
                    Some(o) => (0, Some(o.clone())),
                    None => (0, None),
                };
                if let Some(Operand::Text(t)) = text { out.push(Ev::Say { spk, text: t, pc: n.offset }); }
            }
            0x3D => if let Some(Operand::Res(v)) = n.operands.first() { out.push(Ev::Scene(*v)); },
            0x2B => if let Some(Operand::Label(l)) = n.operands.first() { out.push(Ev::Goto(*l)); },
            0x2C => if let Some((f, l)) = file_label(n) { out.push(Ev::Jump(f, l)); },
            0x0B => if let Some((f, l)) = file_label(n) { out.push(Ev::Call(f, l)); },
            0x3A => out.push(Ev::Return),
            0x0C => {
                if let Some(Operand::Label(l)) = n.operands.first() {
                    out.push(Ev::Gosub(*l));
                    if gd < 3 { out.push(Ev::Branch(vec![self.tree(fi, *l as usize, true)])); }
                }
            }
            0x18 | 0x19 => {
                let then = self.kids(fi, n).last().map(|c| self.sub(fi, *c, gd)).unwrap_or(&[]);
                let els = n.link.map(|l| self.link_tree(fi, l, gd)).unwrap_or(&[]);
                out.push(Ev::Branch(vec![then, els]));
            }
            0x1A | 0x1B => {
                let body = self.kids(fi, n).last().map(|c| self.sub(fi, *c, gd)).unwrap_or(&[]);
                out.push(Ev::Branch(vec![body, &[]]));
            }
            0x3E | 0x3F | 0x40 => {
                if let Some(l) = self.kids(fi, n).first().copied() {
                    let mut es = Vec::new();
                    self.elems(fi, l, &mut es);
                    let alts: Vec<&'static [Ev]> = es.into_iter().map(|e| self.sub(fi, e, gd)).collect();
                    out.push(Ev::Branch(alts));
                }
            }
            0x34 => if let Some(c) = n.operands.get(1).and_then(nodes) { self.emit(fi, c, out, gd) },
            0x36 => if let Some(c) = n.operands.iter().find_map(nodes) { self.emit(fi, c, out, gd) },
            0x38 => {
                if let (Some(Operand::Text(t)), body) = (n.operands.first(), n.operands.get(1).and_then(nodes)) {
                    let b = body.map(|c| self.sub(fi, c, gd)).unwrap_or(&[]);
                    out.push(Ev::Choice(*t, b));
                }
            }
            0x25 => { for c in self.kids(fi, n) { self.emit(fi, c, out, gd); } }
            0x26 => {
                if let Some(c) = self.kids(fi, n).first() { self.emit(fi, *c, out, gd); }
                if let Some(l) = n.link { if let Some(r) = self.files[fi].by_off.get(&(l as usize)) { self.emit(fi, *r, out, gd); } }
            }
            0x27 | 0x30 => { if let Some(c) = n.operands.first().and_then(nodes) { self.emit(fi, c, out, gd); } }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- scanning forward from a voice

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Outcome {
    /// a SAY follows; `waited` = a CMD0A (wait for the voice to end) comes first, `hops` = GOTO/JUMP/CALL taken on the way
    Text { text: u16, spk: u8, pc: usize, file: u8, waited: bool, hops: u8 },
    /// another voice starts first
    Voice { id: u16, waited: bool },
    /// a menu is shown
    Menu,
    /// the script block ends (RETURN, end of a menu item body, end of the tree, unresolved jump)
    End,
}

struct Scan<'a> {
    cx: &'a Ctx,
    outs: BTreeSet<Outcome>,
    vis: HashSet<(usize, usize, bool)>,
    budget: usize,
}

impl<'a> Scan<'a> {
    fn run(&mut self, fi: usize, seq: &'static [Ev], i: usize, waited: bool, stack: Vec<(&'static [Ev], usize)>, hops: u8) {
        let (mut seq, mut i, mut waited, mut stack) = (seq, i, waited, stack);
        loop {
            if self.budget == 0 { self.outs.insert(Outcome::End); return; }
            self.budget -= 1;
            if i >= seq.len() {
                match stack.pop() { Some((ps, pi)) => { seq = ps; i = pi; continue; } None => { self.outs.insert(Outcome::End); return; } }
            }
            if !self.vis.insert((seq.as_ptr() as usize ^ (fi << 40), i, waited)) { return; }
            match &seq[i] {
                Ev::Voice { id, .. } => {
                    if *id >= 36 { self.outs.insert(Outcome::Voice { id: *id, waited }); return; }
                    i += 1;
                }
                Ev::WaitVoice => { waited = true; i += 1; }
                Ev::Say { spk, text, pc } => { self.outs.insert(Outcome::Text { text: *text, spk: *spk, pc: *pc, file: self.cx.files[fi].sp, waited, hops }); return; }
                Ev::Goto(l) => { self.jump(fi, fi, *l, waited, hops); return; }
                Ev::Jump(f, l) | Ev::Call(f, l) => {
                    match self.cx.fi(*f) { Some(tf) => self.jump(fi, tf, *l, waited, hops), None => { self.outs.insert(Outcome::End); } }
                    return;
                }
                Ev::Return => { self.outs.insert(Outcome::End); return; }
                Ev::Choice(..) => { self.outs.insert(Outcome::Menu); return; }
                Ev::Branch(alts) => {
                    let mut st = stack.clone();
                    st.push((seq, i + 1));
                    for a in alts { self.run(fi, a, 0, waited, st.clone(), hops); }
                    return;
                }
                Ev::Wait(_) | Ev::Scene(_) | Ev::Gosub(_) => { i += 1; }
            }
        }
    }

    fn jump(&mut self, _from: usize, tf: usize, l: u16, waited: bool, hops: u8) {
        if hops >= 12 { self.outs.insert(Outcome::End); return; }
        let t = self.cx.tree(tf, l as usize, false);
        if t.is_empty() { self.outs.insert(Outcome::End); return; }
        self.run(tf, t, 0, waited, vec![], hops + 1);
    }
}

// ---------------------------------------------------------------- scene tracking

/// Walk a tree in order collecting, for each voice (pc) the set of scenes that can be active, and the scenes active at
/// every GOTO/JUMP/CALL (to propagate the entry scene to the target).
fn scene_walk(seq: &[Ev], cur: &mut BTreeSet<String>, at_voice: &mut dyn FnMut(usize, &BTreeSet<String>), at_jump: &mut dyn FnMut(Option<u16>, u16, &BTreeSet<String>), depth: u8) {
    for e in seq {
        match e {
            Ev::Voice { pc, .. } => at_voice(*pc, cur),
            Ev::Scene(r) => { *cur = BTreeSet::from([scene_name(*r)]); }
            Ev::Goto(l) | Ev::Gosub(l) => at_jump(None, *l, cur),
            Ev::Jump(f, l) | Ev::Call(f, l) => at_jump(Some(*f), *l, cur),
            Ev::Branch(alts) => {
                // each alternative starts from the same state; afterwards any of them may have changed the scene
                let start = cur.clone();
                let mut end = BTreeSet::new();
                for a in alts {
                    let mut c = start.clone();
                    scene_walk(a, &mut c, at_voice, at_jump, depth + 1);
                    end.extend(c);
                }
                if !alts.is_empty() { *cur = end; }
            }
            Ev::Choice(_, b) => { let mut c = cur.clone(); scene_walk(b, &mut c, at_voice, at_jump, depth + 1); }
            _ => {}
        }
    }
}

fn scene_name(r: u16) -> String { format!("{}#{}", sc::FILE_NAMES.get((r >> 8) as usize).copied().unwrap_or("?"), r & 0xFF) }

// ---------------------------------------------------------------- the inventory

#[derive(Debug, Clone)]
pub struct Trigger {
    pub file: u8,
    pub pc: usize,
    pub label: usize,
    pub id: u16,
    pub scene: String,
    pub outcomes: BTreeSet<Outcome>,
    /// SAY (text offset, speaker) directly before the voice in the same sequence
    pub prev_say: Option<(u16, u8)>,
    /// CHOICE labels (text offsets) enclosing the voice inside its root
    pub path: Vec<u16>,
    /// speakers of the SAYs next to the voice in the same sequence (up to 2 before, 2 after)
    pub near: Vec<u8>,
    /// the root is reachable from the new-game entry (static over-approximation)
    pub reachable: bool,
}

pub fn act_of(sp: u8) -> &'static str {
    match sp { 0..=5 => "sys", 6..=19 => "1", 20..=35 => "2", _ => "3" }
}

fn inventory(dir: &str, opts: &[String]) -> Result<(), String> {
    let mut csv: Option<&str> = None;
    let mut md: Option<&str> = None;
    let mut probes: Option<&str> = None;
    let mut probe_files: Vec<String> = vec![];
    let mut clips_csv: Option<&str> = None;
    let mut icons_file: Option<&str> = None;
    let mut gfx_file: Option<&str> = None;
    let mut scenes_csv: Option<&str> = None;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match o.as_str() {
            "--csv" => csv = it.next().map(|s| s.as_str()),
            "--md" => md = it.next().map(|s| s.as_str()),
            "--probes" => probes = it.next().map(|s| s.as_str()),
            "--probe-results" => { if let Some(f) = it.next() { probe_files.push(f.clone()); } }
            "--graphics-text" => gfx_file = it.next().map(|s| s.as_str()),
            "--icons" => icons_file = it.next().map(|s| s.as_str()),
            "--clips-csv" => clips_csv = it.next().map(|s| s.as_str()),
            "--scenes-csv" => scenes_csv = it.next().map(|s| s.as_str()),
            x => return Err(format!("unknown option {x}")),
        }
    }
    let rd = |n: &str| fs::read(format!("{dir}/{n}")).map_err(|e| format!("{dir}/{n}: {e}"));
    // ---- voice table and durations
    let lt = rd("PCMLT_01.BIN")?;
    let ld = rd("PCMLD_01.BIN")?;
    let vrate = fd_to_hz(VOICE_FD).round();
    let table = parse_voice_table(&lt);
    let mut dur: BTreeMap<u16, (f64, u8, u8, u8)> = BTreeMap::new();
    for v in &table {
        if v.kind == 1 { continue; }
        let blk = &ld[v.sector as usize * 2048..(v.sector as usize + v.sectors as usize) * 2048];
        dur.insert(v.id, (decode_sm8(blk).len() as f64 / vrate, v.kind, v.env, v.pan));
    }
    let subcode = rd("SUBCODE.BIN")?;
    let names: Vec<String> = sncore::actions::speaker_names(&subcode).into_iter().map(|(_, n)| String::from_utf8_lossy(&n).to_string()).collect();

    // ---- scripts
    let mut files = Vec::new();
    for n in 0..=38u8 {
        let raw = rd(&format!("SP{n:02}.BIN"))?;
        let script = sc::parse_script(&raw)?;
        let script: &'static sc::Script = Box::leak(Box::new(script));
        let mut by_off: HashMap<usize, &'static Node> = HashMap::new();
        for r in &script.nodes { sc::walk(r, &mut |x: &'static Node| { by_off.insert(x.offset, x); }); }
        let mut targets = HashSet::new();
        for r in &script.nodes { sc::walk(r, &mut |x: &Node| { if let Some(l) = x.link { targets.insert(l as usize); } }); }
        let roots = script.nodes.iter().map(|r| r.offset).collect();
        files.push(SpFile { sp: n, script, by_off, roots, raw });
        let _ = targets;
    }
    let cx = Ctx { files, trees: Default::default(), extra: Default::default(), ends: Default::default() };

    // ---- scene propagation (entry scenes of roots), a few rounds
    let mut entry: HashMap<(usize, usize), BTreeSet<String>> = HashMap::new();
    entry.entry((6, 0)).or_default().insert("(new game)".into());
    let mut voice_scene: HashMap<(usize, usize), BTreeSet<String>> = HashMap::new();
    for _round in 0..8 {
        let mut changed = false;
        for fi in 0..cx.files.len() {
            for &r in &cx.files[fi].roots {
                let mut cur = entry.get(&(fi, r)).cloned().unwrap_or_else(|| BTreeSet::from(["?".to_string()]));
                let t = cx.tree(fi, r, false);
                let mut jumps: Vec<(Option<u16>, u16, BTreeSet<String>)> = vec![];
                let mut vs: Vec<(usize, BTreeSet<String>)> = vec![];
                scene_walk(t, &mut cur, &mut |pc, s| vs.push((pc, s.clone())), &mut |f, l, s| jumps.push((f, l, s.clone())), 0);
                for (pc, s) in vs { voice_scene.entry((fi, pc)).or_default().extend(s); }
                for (f, l, s) in jumps {
                    let tf = match f { Some(f) => match cx.fi(f) { Some(x) => x, None => continue }, None => fi };
                    let e = entry.entry((tf, l as usize)).or_default();
                    for x in s { if x != "?" || e.is_empty() { changed |= e.insert(x); } }
                }
            }
        }
        if !changed { break; }
    }

    // ---- static reachability of roots from the new-game entry SP06:0000
    fn labels_of(seq: &[Ev], out: &mut Vec<(Option<u16>, u16)>) {
        for e in seq {
            match e {
                Ev::Goto(l) | Ev::Gosub(l) => out.push((None, *l)),
                Ev::Jump(f, l) | Ev::Call(f, l) => out.push((Some(*f), *l)),
                Ev::Branch(alts) => for a in alts { labels_of(a, out); },
                Ev::Choice(_, b) => labels_of(b, out),
                _ => {}
            }
        }
    }
    let root_of = |fi: usize, off: usize| -> Option<usize> {
        let f = &cx.files[fi];
        f.roots.iter().rev().copied().filter(|&r| r <= off).find(|&r| off < cx.vm_end(fi, f.by_off[&r]))
    };
    let mut reach: HashSet<(usize, usize)> = HashSet::new();
    let mut work: Vec<(usize, usize)> = vec![(6, 0)];
    while let Some((fi, r)) = work.pop() {
        if !reach.insert((fi, r)) { continue; }
        let f = &cx.files[fi];
        let end = cx.vm_end(fi, f.by_off[&r]);
        // every root inside the span (list continuations) runs as part of this one
        for &r2 in &f.roots { if r2 > r && r2 < end { work.push((fi, r2)); } }
        let mut ls = vec![];
        labels_of(cx.tree(fi, r, false), &mut ls);
        for (tf, l) in ls {
            let tfi = match tf { Some(t) => match cx.fi(t) { Some(x) => x, None => continue }, None => fi };
            if let Some(t) = root_of(tfi, l as usize) { work.push((tfi, t)); }
        }
    }
    let reached = |fi: usize, pc: usize| -> bool { reach.iter().any(|&(f, r)| f == fi && r <= pc && pc < cx.vm_end(f, cx.files[f].by_off[&r])) };
    if std::env::var("VOICE_DEBUG_REACH").is_ok() { let mut c: BTreeMap<usize, usize> = BTreeMap::new(); for (f, _) in &reach { *c.entry(*f).or_default() += 1; } eprintln!("reach per file: {c:?}"); if let Ok(v) = std::env::var("VOICE_DEBUG_REACH") { if let Ok(n) = v.parse::<usize>() { let un: Vec<String> = cx.files[n].roots.iter().filter(|r| !reach.contains(&(n, **r))).map(|r| format!("{r:04x}")).collect(); eprintln!("unreached in SP{n:02}: {}", un.join(" ")); } } }
    // ---- triggers
    let mut trig: Vec<Trigger> = Vec::new();
    for fi in 0..cx.files.len() {
        let f = &cx.files[fi];
        let mut seen_pc = HashSet::new();
        for &r in &f.roots {
            let t = cx.tree(fi, r, false);
            let rend = cx.vm_end(fi, f.by_off[&r]);
            // find the voice events owned by this root and scan from each
            fn visit(cx: &Ctx, fi: usize, seq: &'static [Ev], stack: &mut Vec<(&'static [Ev], usize)>, path: &mut Vec<u16>, own: &dyn Fn(usize) -> bool, found: &mut Vec<(usize, u16, BTreeSet<Outcome>, Option<(u16, u8)>, Vec<u16>, Vec<u8>)>) {
                for (i, e) in seq.iter().enumerate() {
                    match e {
                        Ev::Voice { id, pc } if own(*pc) => {
                            let mut sc = Scan { cx, outs: BTreeSet::new(), vis: HashSet::new(), budget: 20000 };
                            sc.run(fi, seq, i + 1, false, stack.clone(), 0);
                            // nearest preceding SAY in the same sequence with nothing but waits / scene changes in between
                            let prev = seq[..i].iter().rev().find(|e| !matches!(e, Ev::Wait(_) | Ev::WaitVoice | Ev::Scene(_))).and_then(|e| if let Ev::Say { text, spk, .. } = e { Some((*text, *spk)) } else { None });
                            let mut near: Vec<u8> = seq[..i].iter().rev().filter_map(|e| if let Ev::Say { spk, .. } = e { Some(*spk) } else { None }).take(2).collect();
                            near.extend(seq[i + 1..].iter().filter_map(|e| if let Ev::Say { spk, .. } = e { Some(*spk) } else { None }).take(2));
                            found.push((*pc, *id, sc.outs, prev, path.clone(), near));
                        }
                        Ev::Branch(alts) => {
                            stack.push((seq, i + 1));
                            for a in alts { visit(cx, fi, a, stack, path, own, found); }
                            stack.pop();
                        }
                        Ev::Choice(t, b) => {
                            stack.push((seq, i + 1));
                            path.push(*t);
                            visit(cx, fi, b, stack, path, own, found);
                            path.pop();
                            stack.pop();
                        }
                        _ => {}
                    }
                }
            }
            let mut found = vec![];
            visit(&cx, fi, t, &mut vec![], &mut vec![], &|pc| pc >= r && pc < rend, &mut found);
            for (pc, id, outs, prev, path, near) in found {
                if !seen_pc.insert(pc) { continue; }
                let sc = voice_scene.get(&(fi, pc)).map(|s| s.iter().cloned().collect::<Vec<_>>().join(" | ")).unwrap_or_default();
                trig.push(Trigger { file: f.sp, pc, label: r, id, scene: sc, outcomes: outs, prev_say: prev, path, near, reachable: reached(fi, pc) });
            }
        }
    }
    trig.sort_by_key(|t| (t.file, t.pc));
    // every CMD0D node of the files must be found exactly once (nested nodes under a root's rend included)
    let mut total_nodes = 0;
    for f in &cx.files { for r in &f.script.nodes { sc::walk(r, &mut |x: &Node| { if x.op == 0x04 { total_nodes += 1; } }); } }
    eprintln!("CMD0D nodes in the scripts: {total_nodes}, triggers analysed: {}", trig.len());
    if trig.len() != total_nodes { return Err(format!("voice triggers ({}) differ from the CMD0D nodes of the scripts ({total_nodes})", trig.len())); }

    // ---- report
    // dynamic probe results (engine/examples/voice_probe.rs output), id -> most common screen class
    let mut probe: HashMap<u16, BTreeMap<String, usize>> = HashMap::new();
    let mut probe_icons: HashMap<u16, BTreeMap<String, usize>> = HashMap::new();
    let mut icon_names: HashMap<String, String> = HashMap::new();
    if let Some(f) = icons_file { for l in fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?.lines() { if let Some((h, n)) = l.split_once('\t') { icon_names.insert(h.trim().to_string(), n.trim().to_string()); } } }
    for pf in &probe_files {
        for line in fs::read_to_string(pf).map_err(|e| format!("{pf}: {e}"))?.lines() {
            if !line.starts_with("PROBE ") { continue; }
            let id = line.split("id=").nth(1).and_then(|x| x.split_whitespace().next()).and_then(|x| x.parse::<u16>().ok());
            let cls = line.split("box=").nth(1).and_then(|x| x.split_whitespace().next());
            if let (Some(id), Some(c)) = (id, cls) {
                *probe.entry(id).or_default().entry(c.to_string()).or_default() += 1;
                let icons = line.split("icons=").nth(1).and_then(|x| x.split_whitespace().next()).unwrap_or("");
                let set: Vec<String> = icons.split(',').filter(|x| !x.is_empty()).map(|x| x.split(':').next().unwrap_or("").to_string()).collect();
                *probe_icons.entry(id).or_default().entry(set.join("+")).or_default() += 1;
            }
        }
    }
    let probe_cls = |id: u16| -> String {
        probe.get(&id).and_then(|m| m.iter().max_by_key(|(k, v)| (**v, std::cmp::Reverse((*k).clone()))).map(|(k, _)| k.clone())).unwrap_or_default()
    };
    // portraits shown in the dialogue strip while the clip plays (most common set over the probe runs), as names when `--icons` knows them
    let icon_list = |id: u16| -> Vec<String> {
        probe_icons.get(&id).and_then(|m| m.iter().max_by_key(|(k, v)| (**v, std::cmp::Reverse((*k).clone()))).map(|(k, _)| k.clone())).unwrap_or_default()
            .split('+').filter(|x| !x.is_empty()).map(|h| icon_names.get(h).cloned().unwrap_or_else(|| h.to_string())).collect()
    };
    // clips whose picture carries text (reviewed by eye on the probe screenshots): id <TAB> class (with-text | partial) <TAB> note
    let mut gfx_text: HashMap<u16, String> = HashMap::new();
    if let Some(f) = gfx_file {
        for l in fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?.lines() {
            let c: Vec<&str> = l.split('\t').collect();
            if c.len() >= 2 && !l.starts_with('#') { if let Ok(id) = c[0].trim().parse::<u16>() { gfx_text.insert(id, c[1].trim().to_string()); } }
        }
    }
    let spk_name = |s: u8| -> String { if s >= 2 { names.get(s as usize - 1).cloned().unwrap_or_default() } else { String::new() } };
    struct Row<'a> { t: &'a Trigger, class: &'static str, secs: f64, kind: u8, env: u8, pan: u8, text: Option<(u16, u8)>, rel: &'static str, via_jump: bool, next_voice: bool, menu: bool, ctx: String, dynbox: String, present: String }
    let mut rows: Vec<Row> = Vec::new();
    for t in &trig {
        let (secs, kind, env, pan) = dur.get(&t.id).copied().unwrap_or((0.0, 0, 0, 0));
        let bank = t.id < 36;
        let simul = t.outcomes.iter().find_map(|o| if let Outcome::Text { text, spk, hops: 0, waited: false, .. } = o { Some((*text, *spk)) } else { None });
        let after = t.outcomes.iter().find_map(|o| if let Outcome::Text { text, spk, hops: 0, waited: true, .. } = o { Some((*text, *spk)) } else { None });
        let via_jump = t.outcomes.iter().any(|o| matches!(o, Outcome::Text { hops, .. } if *hops > 0));
        let (class, rel, text) = if bank { ("bank", "none", None) }
            else if let Some(c) = gfx_text.get(&t.id) { (if c == "with-text" { "with-text" } else { "partial" }, "text-in-picture", None) }
            else if simul.is_some() { ("with-text", "simultaneous", simul) }
            else if after.is_some() { ("unknown", "text-after-wait", after) }
            else if t.prev_say.is_some() { ("unknown", "text-before", t.prev_say) }
            else { ("voice-only", "none", None) };
        let mut cnt: BTreeMap<String, usize> = BTreeMap::new();
        for &sp in &t.near { let n = spk_name(sp); if !n.is_empty() { *cnt.entry(n).or_default() += 1; } }
        let ctx = cnt.into_iter().map(|(k, _)| k).collect::<Vec<_>>().join("/");
        rows.push(Row { t, class, secs, kind, env, pan, text, rel, via_jump,
            next_voice: t.outcomes.iter().any(|o| matches!(o, Outcome::Voice { .. })), menu: t.outcomes.contains(&Outcome::Menu), ctx, dynbox: if bank { String::new() } else { probe_cls(t.id) }, present: if bank { String::new() } else { icon_list(t.id).join("+") } });
    }
    // clips the probes did not reach: infer the screen class from the scene resource of the trigger when probed clips share it
    let mut scene_cls: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
    for r in rows.iter().filter(|r| !r.dynbox.is_empty() && !r.t.scene.contains(" | ") && r.t.scene != "?" && !r.t.scene.starts_with('(')) {
        *scene_cls.entry(r.t.scene.clone()).or_default().entry(r.dynbox.clone()).or_default() += 1;
    }
    let mut inferred = 0;
    for r in rows.iter_mut().filter(|r| r.dynbox.is_empty() && r.class != "bank") {
        let sc = r.t.scene.split(" | ").filter(|x| *x != "?" && !x.starts_with('(')).collect::<Vec<_>>();
        let mut m: BTreeMap<String, usize> = BTreeMap::new();
        for x in &sc { if let Some(c) = scene_cls.get(*x) { for (k, v) in c { *m.entry(k.clone()).or_default() += v; } } }
        if let Some((k, _)) = m.iter().max_by_key(|(k, v)| (**v, std::cmp::Reverse((*k).clone()))) { r.dynbox = format!("{k} (inferred)"); inferred += 1; }
    }
    eprintln!("screen class: probed {} voice triggers, inferred from the scene {inferred}", rows.iter().filter(|r| r.class != "bank" && !r.dynbox.is_empty() && !r.dynbox.contains("inferred")).count());
    let csv_esc = |s: &str| s.replace(',', ";");
    let mut out = String::from("id,kind,seconds,file,act,pc,label,scene,class,text_rel,text_off,text_spk,speaker,context_speakers,text_via_jump,next_voice,menu_next,reachable,screen,portraits,env,pan\n");
    for r in &rows {
        let t = r.t;
        out += &format!("{},{},{:.3},SP{:02},{},{:04x},L_{:04x},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n", t.id, r.kind, r.secs, t.file, act_of(t.file), t.pc, t.label,
            csv_esc(&t.scene), r.class, r.rel, r.text.map(|x| format!("{:04x}", x.0)).unwrap_or_default(), r.text.map(|x| x.1 as i32).unwrap_or(-1),
            r.text.map(|x| spk_name(x.1)).unwrap_or_default(), r.ctx, r.via_jump, r.next_voice, r.menu, t.reachable, r.dynbox, r.present, r.env, r.pan);
    }
    if let Some(p) = csv { fs::write(p, &out).map_err(|e| format!("{p}: {e}"))?; }
    if let Ok(d) = std::env::var("VOICE_DEBUG") {
        for t in &trig { if format!("SP{:02}:{:04x}", t.file, t.pc) == d { eprintln!("{t:#?}"); } }
    }
    if let Some(p) = probes {
        // entry points for `voice_probe`: SPnn:root[:choice,choice] (one per root and enclosing menu path)
        let mut set: BTreeSet<String> = BTreeSet::new();
        for t in trig.iter().filter(|t| t.id >= 36) {
            // the scene that is active at the voice (only when the analysis found exactly one), so the probe can show it first
            let scenes: Vec<&str> = t.scene.split(" | ").filter(|x| *x != "?" && !x.starts_with('(')).collect();
            let res = if scenes.len() == 1 { scenes[0].split_once('#').and_then(|(f, i)| Some(((sc::FILE_NAMES.iter().position(|n| *n == f)? as u16) << 8 | i.parse::<u16>().ok()?, ()))).map(|(r, _)| format!("{r:04x}")).unwrap_or_default() } else { String::new() };
            set.insert(format!("SP{:02}:{:04x}:{}:{}", t.file, t.label, t.path.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(","), res));
        }
        fs::write(p, set.into_iter().collect::<Vec<_>>().join("\n") + "\n").map_err(|e| format!("{p}: {e}"))?;
    }

    // ---- per clip (unique voice id)
    #[derive(Default)]
    struct Clip { secs: f64, kind: u8, n: usize, vo: usize, unk: usize, wt: usize, pa: usize, reach: usize, dbg_only: bool, files: BTreeSet<u8>, first: (u8, usize) }
    let mut clips: BTreeMap<u16, Clip> = BTreeMap::new();
    for r in rows.iter().filter(|r| r.class != "bank") {
        let c = clips.entry(r.t.id).or_default();
        if c.n == 0 { c.first = (r.t.file, r.t.pc); c.secs = r.secs; c.kind = r.kind; }
        c.n += 1;
        match r.class { "voice-only" => c.vo += 1, "unknown" => c.unk += 1, "with-text" => c.wt += 1, "partial" => c.pa += 1, _ => {} }
        if r.t.reachable && r.t.file != 1 { c.reach += 1; }
        c.files.insert(r.t.file);
    }
    for c in clips.values_mut() { c.dbg_only = c.reach == 0; }
    let unused: Vec<u16> = table.iter().filter(|v| v.kind != 1 && !clips.contains_key(&v.id)).map(|v| v.id).collect();
    let clip_class = |c: &Clip| -> &'static str { if c.wt > 0 { "with-text" } else if c.pa > 0 { "partial" } else if c.vo > 0 { "voice-only" } else { "unknown" } };
    let clip_sorted = clips.iter().map(|(id, c)| (*id, c)).collect::<Vec<_>>();
    if let Some(p) = clips_csv {
        let mut o = String::from("id,kind,seconds,class,triggers,voice_only_triggers,adjacent_text_triggers,reachable_in_game,files,first_file,first_pc,screen\n");
        for (id, c) in &clip_sorted {
            o += &format!("{id},{},{:.3},{},{},{},{},{},{},SP{:02},{:04x},{}\n", c.kind, c.secs, clip_class(c), c.n, c.vo, c.unk, c.reach > 0, c.files.iter().map(|f| format!("SP{f:02}")).collect::<Vec<_>>().join("/"), c.first.0, c.first.1, rows.iter().find(|r| r.t.id == *id).map(|r| r.dynbox.clone()).unwrap_or_default());
        }
        for id in &unused {
            let v = table.iter().find(|v| v.id == *id).unwrap();
            o += &format!("{id},{},{:.3},unused,0,0,0,false,,,,\n", v.kind, dur[id].0);
        }
        fs::write(p, o).map_err(|e| format!("{p}: {e}"))?;
    }

    // ---- scene groups (voice-only triggers of the game proper, per root label)
    struct Grp { file: u8, label: usize, act: &'static str, n: usize, secs: f64, scenes: BTreeSet<String>, ctx: BTreeMap<String, usize>, boxes: BTreeMap<String, usize>, present: BTreeMap<String, usize>, ids: Vec<u16>, pcs: (usize, usize) }
    let mut grps: BTreeMap<(u8, usize), Grp> = BTreeMap::new();
    let mut seen_ids: HashSet<(u8, usize, u16)> = HashSet::new();
    for r in rows.iter().filter(|r| r.class == "voice-only" && r.t.file != 1) {
        let g = grps.entry((r.t.file, r.t.label)).or_insert_with(|| Grp { file: r.t.file, label: r.t.label, act: act_of(r.t.file), n: 0, secs: 0.0, scenes: BTreeSet::new(), ctx: BTreeMap::new(), boxes: BTreeMap::new(), present: BTreeMap::new(), ids: vec![], pcs: (usize::MAX, 0) });
        g.n += 1;
        if seen_ids.insert((r.t.file, r.t.label, r.t.id)) { g.secs += r.secs; g.ids.push(r.t.id); }
        for sc in r.t.scene.split(" | ") { g.scenes.insert(sc.to_string()); }
        for n in r.ctx.split('/') { if !n.is_empty() { *g.ctx.entry(n.to_string()).or_default() += 1; } }
        if !r.dynbox.is_empty() { *g.boxes.entry(r.dynbox.clone()).or_default() += 1; }
        for n in r.present.split('+') { if !n.is_empty() { *g.present.entry(n.to_string()).or_default() += 1; } }
        g.pcs = (g.pcs.0.min(r.t.pc), g.pcs.1.max(r.t.pc));
    }
    let nice = |k: &str| -> String {
        let (b, inf) = match k.strip_suffix(" (inferred)") { Some(b) => (b, "*"), None => (k, "") };
        format!("{}{inf}", match b { "strip-empty" => "empty strip", "strip-portraits" => "strip + portraits", "black" => "black", "fullscreen" => "no strip", x => x })
    };
    let box_label = |m: &BTreeMap<String, usize>| -> String {
        if m.is_empty() { return "not probed".into(); }
        let total: usize = m.values().sum();
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by_key(|(k, c)| (std::cmp::Reverse(**c), (*k).clone()));
        v.iter().take(2).map(|(k, c)| if v.len() == 1 || **c * 10 >= total * 9 { nice(k) } else { format!("{} {}%", nice(k), **c * 100 / total) }).collect::<Vec<_>>().join(", ")
    };
    let hint = |g: &Grp| -> String {
        let mut parts: Vec<String> = vec![];
        if !g.ctx.is_empty() { parts.push(format!("text: {}", g.ctx.keys().cloned().collect::<Vec<_>>().join("/"))); }
        if !g.present.is_empty() { parts.push(format!("portraits: {}", g.present.keys().cloned().collect::<Vec<_>>().join("+"))); }
        if parts.is_empty() { "?".into() } else { parts.join("; ") }
    };
    let scene_short = |g: &Grp| -> String {
        let v: Vec<&String> = g.scenes.iter().filter(|s| s.as_str() != "?").collect();
        if v.is_empty() { "?".into() } else if v.len() <= 2 { v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ") } else { format!("{} (+{} more)", v[0], v.len() - 1) }
    };
    if let Some(p) = scenes_csv {
        let mut o = String::from("file,label,act,pc_first,pc_last,clips,seconds,scenes,speaker_hint,screen\n");
        for g in grps.values() {
            o += &format!("SP{:02},L_{:04x},{},{:04x},{:04x},{},{:.1},{},{},{}\n", g.file, g.label, g.act, g.pcs.0, g.pcs.1, g.ids.len(), g.secs, csv_esc(&g.scenes.iter().cloned().collect::<Vec<_>>().join(" | ")),
                hint(g), box_label(&g.boxes));
        }
        fs::write(p, o).map_err(|e| format!("{p}: {e}"))?;
    }

    // ---- totals
    let mut tot: BTreeMap<&str, (usize, f64)> = BTreeMap::new();
    for (_, c) in &clip_sorted { let e = tot.entry(clip_class(c)).or_default(); e.0 += 1; e.1 += c.secs; }
    let (total_clips, total_secs) = (table.iter().filter(|v| v.kind != 1).count(), table.iter().filter(|v| v.kind != 1).map(|v| dur[&v.id].0).sum::<f64>());
    let unused_secs: f64 = unused.iter().map(|id| dur[id].0).sum();
    println!("voice clips in the disc table: {total_clips} ({:.1} min); started by a script: {} ({:.1} min); never started: {} ({:.1} min)", total_secs / 60.0, clips.len(), clips.values().map(|c| c.secs).sum::<f64>() / 60.0, unused.len(), unused_secs / 60.0);
    for (k, (n, s)) in &tot { println!("  {k}: {n} clips, {:.1} min", s / 60.0); }
    let dbg: Vec<_> = clip_sorted.iter().filter(|(_, c)| c.dbg_only).collect();
    if !dbg.is_empty() { println!("  clips started only by the SP01 debug menu or unreachable code: {} ({:.1} min)", dbg.len(), dbg.iter().map(|(_, c)| c.secs).sum::<f64>() / 60.0); }

    if let Some(p) = md {
        let mut m = String::new();
        let min = |s: f64| format!("{:.1}", s / 60.0);
        m += "| class | clips | minutes |\n|---|---:|---:|\n";
        for (k, (n, s)) in &tot { m += &format!("| {k} | {n} | {} |\n", min(*s)); }
        m += &format!("| never started by any script | {} | {} |\n", unused.len(), min(unused_secs));
        m += &format!("| **all voice clips of the disc** | **{total_clips}** | **{}** |\n\n", min(total_secs));
        // screen while the clip plays (clip level: the class of its first trigger)
        let mut sc_tab: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
        for (id, c) in &clip_sorted {
            let d = rows.iter().find(|r| r.t.id == *id).map(|r| r.dynbox.clone()).unwrap_or_default();
            let inf = d.contains("inferred");
            let base = if d.is_empty() { "not probed".to_string() } else { d.replace(" (inferred)", "") };
            let e = sc_tab.entry(base).or_default();
            e.0 += 1; if inf { e.1 += 1; } e.2 += c.secs;
        }
        m += "| screen while the clip plays | clips | of which inferred from the scene | minutes |\n|---|---:|---:|---:|\n";
        for (k, (n, inf, secs)) in &sc_tab { m += &format!("| {} | {n} | {inf} | {} |\n", nice(k), min(*secs)); }
        m += "\n";
        // adjacent text
        let adj: Vec<&Row> = rows.iter().filter(|r| r.class == "unknown").collect();
        let (after, before) = (adj.iter().filter(|r| r.rel == "text-after-wait").count(), adj.iter().filter(|r| r.rel == "text-before").count());
        m += &format!("adjacent text (class unknown): {} triggers, {after} with the text after the wait, {before} with the text before the voice\n\n", adj.len());
        // per file
        m += "| file | act | triggers (voice-only / other) | distinct clips | minutes |\n|---|---|---:|---:|---:|\n";
        for fi in 0..39u8 {
            let rs: Vec<&Row> = rows.iter().filter(|r| r.class != "bank" && r.t.file == fi).collect();
            if rs.is_empty() { continue; }
            let ids: BTreeSet<u16> = rs.iter().map(|r| r.t.id).collect();
            let secs: f64 = ids.iter().map(|i| dur[i].0).sum();
            let vo = rs.iter().filter(|r| r.class == "voice-only").count();
            m += &format!("| SP{fi:02} | {} | {} ({} / {}) | {} | {} |\n", act_of(fi), rs.len(), vo, rs.len() - vo, ids.len(), min(secs));
        }
        m += "\n";
        // scene groups
        let mut gv: Vec<&Grp> = grps.values().collect();
        gv.sort_by_key(|g| (g.file, g.label));
        m += "| file | label | act | scene resource(s) | clips | seconds | screen while the clip plays | speaker hint |\n|---|---|---|---|---:|---:|---|---|\n";
        for g in gv {
            m += &format!("| SP{:02} | L_{:04x} | {} | {} | {} | {:.0} | {} | {} |\n", g.file, g.label, g.act, scene_short(g), g.ids.len(), g.secs, box_label(&g.boxes), hint(g));
        }
        m += "\n";
        m += "| id | seconds | class | file | pc | relation | text offset | speaker of that text |\n|---|---:|---|---|---|---|---|---|\n";
        for r in rows.iter().filter(|r| r.class != "bank" && r.class != "voice-only") {
            m += &format!("| {} | {:.1} | {} | SP{:02} | {:04x} | {} | {} | {} |\n", r.t.id, r.secs, r.class, r.t.file, r.t.pc, r.rel, r.text.map(|x| format!("{:04x}", x.0)).unwrap_or_default(), r.text.map(|x| spk_name(x.1)).unwrap_or_default());
        }
        m += "\n";
        m += &format!("unused clip ids: {}\n", unused.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(" "));
        fs::write(p, m).map_err(|e| format!("{p}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs the extracted disc (assets/raw/files); skipped without it.
    #[test]
    fn inventory_covers_every_cmd0d_node() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/raw/files");
        if !std::path::Path::new(dir).join("SP06.BIN").exists() { eprintln!("skip: no extracted files"); return; }
        inventory(dir, &[]).expect("inventory");
    }

    #[test]
    fn acts_by_script_file() {
        assert_eq!(act_of(6), "1");
        assert_eq!(act_of(20), "2");
        assert_eq!(act_of(38), "3");
        assert_eq!(act_of(1), "sys");
    }
}
