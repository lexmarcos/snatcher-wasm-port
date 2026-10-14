//! Script subcommands: `script-disasm`, `script-disasm-all`, `script-strings`.
//! Return None if the command is not handled here.
use sncore::script::{self as sc, Node, Operand};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    match args[0].as_str() {
        "script-disasm" if args.len() >= 2 => Some(disasm_one(&args[1], args.get(2).map(|s| s.as_str()))),
        "script-disasm-all" if args.len() == 3 => Some(disasm_all(&args[1], &args[2])),
        "script-strings" if args.len() >= 2 => Some(strings(&args[1])),
        "script-run" if args.len() >= 2 => Some(run(&args[1], args.get(2).map(|s| s.as_str()).unwrap_or(""), args.get(3).and_then(|s| s.parse().ok()).unwrap_or(200), args.get(4).map(|s| s.as_str()))),
        _ => None,
    }
}

fn file_id_from_name(p: &str) -> Option<u16> {
    let n = Path::new(p).file_name()?.to_str()?.to_ascii_uppercase();
    let n = n.strip_suffix(".BIN").unwrap_or(&n).to_string();
    sc::FILE_NAMES.iter().position(|f| *f == n).map(|i| i as u16)
}

fn file_name(id: u16) -> String {
    sc::FILE_NAMES.get(id as usize).map(|s| s.to_string()).unwrap_or(format!("FILE_{id:02X}"))
}

/// Labels inside one file: link targets and label operands that refer to this file.
fn collect_labels(s: &sc::Script, own: Option<u16>, extra: &BTreeSet<u16>) -> BTreeMap<usize, String> {
    let mut l: BTreeMap<usize, String> = BTreeMap::new();
    let add = |o: u16, why: &str, l: &mut BTreeMap<usize, String>| {
        let e = l.entry(o as usize).or_insert_with(|| format!("L_{o:04x}"));
        let _ = why;
        let _ = e;
    };
    for n in &s.nodes {
        sc::walk(n, &mut |x: &Node| {
            if let Some(p) = x.link { add(p, "link", &mut l) }
            // JUMP / CALL: tuple (file, label)
            if matches!(x.op, 0x0B | 0x2C) {
                if let Some((f, lab)) = file_label(x) {
                    if Some(f) == own { add(lab, "jump", &mut l) }
                }
                return;
            }
            if x.op == 0x21 { return } // tuples (file,label) handled by parent
            for o in &x.operands {
                if let Operand::Label(v) = o { add(*v, "label", &mut l) }
            }
        });
    }
    for &e in extra { add(e, "extern", &mut l) }
    l
}

fn file_label(x: &Node) -> Option<(u16, u16)> {
    if let Some(Operand::Node(t)) = x.operands.first() {
        if let (Some(Operand::Res(f)), Some(Operand::Label(lab))) = (t.operands.first(), t.operands.get(1)) {
            return Some((*f, *lab));
        }
    }
    None
}

struct Ctx<'a> {
    speakers: &'a [(u8, String)],
    file: &'a [u8],
    own: Option<u16>,
    labels: &'a BTreeMap<usize, String>,
    out: String,
}

fn is_expr(op: u8) -> bool {
    matches!(op, 0x01..=0x03 | 0x06..=0x09 | 0x10..=0x12 | 0x15..=0x17 | 0x28..=0x2A | 0x2E | 0x2F
        | 0x31..=0x33 | 0x3B | 0x3C)
}

/// True if the subtree is made only of LIST tuples with scalar operands (rendered inline).
fn is_tuple(n: &Node) -> bool {
    let i = sc::op_info(n.op).unwrap();
    i.handler == sc::HANDLER_LIST && n.link.is_none() && n.op != 0x25 && n.op != 0x27 && n.op != 0x26
        && n.operands.iter().all(|o| match o { Operand::Node(c) => is_tuple(c), _ => true })
}

impl<'a> Ctx<'a> {
    fn lab(&self, v: u16) -> String {
        self.labels.get(&(v as usize)).cloned().unwrap_or(format!("L_{v:04x}"))
    }
    fn text(&self, off: u16) -> String {
        match sc::text_at(self.file, off) {
            Some(t) => format!("@{off:04x} \"{}\"", sc::decode_text(t).replace('\n', "\\n").replace('"', "\\\"")),
            None => format!("@{off:04x} <no text>"),
        }
    }
    fn scalar(&self, o: &Operand) -> String {
        match o {
            Operand::Var(v) => sc::var_name(*v),
            Operand::Text(t) => self.text(*t),
            Operand::Imm(v) => if *v < 10 { format!("{v}") } else { format!("0x{v:x}") },
            Operand::Label(v) => self.lab(*v),
            Operand::Res(v) => format!("#{v:04x}"),
            Operand::Node(n) => self.inline(n),
        }
    }
    fn tuple(&self, n: &Node) -> String {
        let parts: Vec<String> = n.operands.iter().map(|o| self.scalar(o)).collect();
        format!("({})", parts.join(", "))
    }
    /// Inline rendering for expressions / tuples.
    fn inline(&self, n: &Node) -> String {
        let a = |i: usize| n.operands.get(i).map(|o| self.scalar(o)).unwrap_or_default();
        let bin = |sym: &str| format!("({} {sym} {})", a(0), a(1));
        match n.op {
            0x01..=0x03 => bin("+"),
            0x06..=0x09 => bin("&"),
            0x31..=0x33 => bin("|"),
            0x10..=0x12 => bin("=="),
            0x16 => bin(">"),
            0x17 => bin(">="),
            0x28 | 0x29 => bin("<"),
            0x2A => bin("<="),
            0x2E | 0x2F => format!("!{}", a(0)),
            0x15 => format!("history({})", a(0)),
            0x3B => format!("in_gosub({})", a(0)),
            0x3C => format!("gosub_pc({})", a(0)),
            0x42 | 0x41 => self.sys(n),
            _ if is_tuple(n) => self.tuple(n),
            _ => format!("<{:02x}@{:04x}>", n.op, n.offset),
        }
    }
    fn sys(&self, n: &Node) -> String {
        // 41: fn ; 42: tuple(fn, args...)
        let (f, rest): (u16, Vec<String>) = match n.operands.first() {
            Some(Operand::Res(f)) => (*f, vec![]),
            Some(Operand::Node(t)) => {
                let f = match t.operands.first() { Some(Operand::Res(v)) | Some(Operand::Imm(v)) => *v, _ => 0xFFFF };
                (f, t.operands.iter().skip(1).map(|o| self.scalar(o)).collect())
            }
            _ => (0xFFFF, vec![]),
        };
        let name = sc::SYS_NAMES.get(f as usize).copied().unwrap_or("?");
        format!("sys.{name}({})", rest.join(", "))
    }
    fn line(&mut self, off: usize, depth: usize, s: &str) {
        if depth == 0 || self.labels.contains_key(&off) {
            if let Some(l) = self.labels.get(&off) { self.out += &format!("\n{l}:\n") }
        }
        self.out += &format!("{off:04x}  {}{s}\n", "  ".repeat(depth));
    }
    fn node(&mut self, n: &Node, depth: usize) {
        let info = sc::op_info(n.op).unwrap();
        let link = n.link.map(|p| format!(" ->{}", self.lab(p))).unwrap_or_default();
        let m = info.mnemonic;
        // statement forms with custom rendering
        let head: Option<String> = match n.op {
            _ if is_expr(n.op) || n.op == 0x42 || n.op == 0x41 => Some(self.inline(n)),
            _ if is_tuple(n) => Some(format!("TUPLE {}", self.tuple(n))),
            0x2B => Some(format!("GOTO {}", self.scalar(&n.operands[0]))),
            0x2C | 0x0B => Some(match file_label(n) {
                Some((f, l)) if Some(f) == self.own => format!("{m} {}", self.lab(l)),
                Some((f, l)) => format!("{m} {}:L_{l:04x}", file_name(f)),
                None => format!("{m} ?"),
            }),
            0x0C => Some(format!("GOSUB {}", self.scalar(&n.operands[0]))),
            0x3D => Some(match n.operands[0] { Operand::Res(v) => format!("SCENE {} img {}", file_name(v >> 8), v & 0xFF), _ => "SCENE ?".into() }),
            0x43 => Some(match n.operands.first() {
                Some(Operand::Node(t)) if t.operands.len() == 2 => {
                    let spk = match t.operands[0] { Operand::Imm(v) | Operand::Res(v) => v, _ => 0 };
                    let who = match self.speakers.get((spk as usize).wrapping_sub(1)) {
                        Some((_, n)) if !n.is_empty() => n.clone(),
                        _ => format!("spk={spk}"),
                    };
                    format!("SAY {who}: {}", self.scalar(&t.operands[1]))
                }
                _ => format!("SAY {}", self.scalar(&n.operands[0])),
            }),
            0x44 => Some(format!("SAY {}", self.scalar(&n.operands[0]))),
            0x00 => Some(format!("ANIM obj,mode={}", self.scalar(&n.operands[0]))),
            0x1C | 0x1D if matches!(n.operands.get(1), Some(Operand::Node(c)) if !is_expr(c.op) && c.op != 0x42 && !is_tuple(c)) => None,
            0x1C | 0x1D => Some(format!("{} = {}", self.scalar(&n.operands[0]), self.scalar(&n.operands[1]))),
            0x1A | 0x1B | 0x18 | 0x19 if matches!(n.operands.first(), Some(Operand::Var(_))) || matches!(n.operands.first(), Some(Operand::Node(c)) if is_expr(c.op) || c.op == 0x42) => {
                let cond = self.scalar(&n.operands[0]);
                let kw = if n.op >= 0x1A { "WHEN" } else { "IF" };
                let s = if n.op <= 0x19 { format!("{kw} {cond}{link} (else)") } else { format!("{kw} {cond}") };
                self.line(n.offset, depth, &s);
                for o in &n.operands[1..] { self.operand(o, depth + 1) }
                return;
            }
            _ => None,
        };
        if let Some(h) = head {
            self.line(n.offset, depth, &h);
            return;
        }
        // cons chains (25 inline cdr, 26 linked rest, 27 last) are flattened into a block
        if matches!(n.op, 0x25..=0x27) {
            let hdr = match n.op { 0x26 => format!("SEQ{link}"), _ => "SEQ".to_string() };
            self.line(n.offset, depth, &hdr);
            self.flatten(n, depth + 1);
            return;
        }
        // generic: mnemonic + scalar operands on the line, nested nodes below
        let mut parts = vec![];
        for o in &n.operands {
            if !matches!(o, Operand::Node(_)) { parts.push(self.scalar(o)) }
        }
        let s = format!("{m}{link} {}", parts.join(", ")).trim_end().to_string();
        self.line(n.offset, depth, &s);
        for o in &n.operands {
            if let Operand::Node(c) = o { self.node(c, depth + 1) }
        }
    }
    /// Print the elements of a cons chain at the same depth.
    fn flatten(&mut self, n: &Node, depth: usize) {
        let mut cur = n;
        loop {
            match cur.op {
                0x25 | 0x26 | 0x27 if cur.operands.first().map(|o| matches!(o, Operand::Node(_))).unwrap_or(false) => {
                    if !std::ptr::eq(cur, n) {
                        if let Some(l) = self.labels.get(&cur.offset) { self.out += &format!("{}; {l}: (cons cell {:04x})\n", "  ".repeat(depth + 3), cur.offset) }
                    }
                    if let Operand::Node(car) = &cur.operands[0] { self.node(car, depth) }
                    match cur.op {
                        0x26 => { self.out += &format!("      {}...continues at {}\n", "  ".repeat(depth), self.lab(cur.link.unwrap())); return }
                        0x27 => return,
                        _ => match cur.operands.get(1) {
                            Some(Operand::Node(cdr)) => cur = cdr,
                            _ => return,
                        },
                    }
                }
                _ => { self.node(cur, depth); return }
            }
        }
    }
    fn operand(&mut self, o: &Operand, depth: usize) {
        match o {
            Operand::Node(c) => self.node(c, depth),
            x => { let s = self.scalar(x); self.out += &format!("      {}{s}\n", "  ".repeat(depth)) }
        }
    }
}

/// Disassemble one file. Returns (text, stats).
fn disasm_bytes(name: &str, file: &[u8], own: Option<u16>, extern_labels: &BTreeSet<u16>, speakers: &[(u8, String)]) -> Result<(String, Stats), String> {
    let s = sc::parse_script(file).map_err(|e| format!("{name}: {e}"))?;
    let labels = collect_labels(&s, own, extern_labels);
    // every label must be the offset of some node (top-level or nested)
    let mut starts = BTreeSet::new();
    for n in &s.nodes { sc::walk(n, &mut |x: &Node| { starts.insert(x.offset); }) }
    let bad: Vec<usize> = labels.keys().filter(|k| !starts.contains(k)).copied().collect();
    let mut st = Stats { op_hist: vec![0; 256], files: 1, code_bytes: s.code_end, nodes: starts.len(), strings: s.strings.len(), bad_labels: bad.len(), ..Default::default() };
    let mut text_refs = BTreeSet::new();
    for n in &s.nodes {
        sc::walk(n, &mut |x: &Node| {
            st.op_hist[x.op as usize] += 1;
            for o in &x.operands { if let Operand::Text(t) = o { text_refs.insert(*t); } }
        });
    }
    let strings_set: BTreeSet<u16> = s.strings.iter().map(|x| x.0).collect();
    st.text_refs = text_refs.len();
    st.text_refs_ok = text_refs.iter().filter(|t| strings_set.contains(t)).count();
    st.unreferenced_strings = strings_set.difference(&text_refs).count();
    let mut c = Ctx { speakers, file, own, labels: &labels, out: String::new() };
    c.out += &format!("; {name}  file id {}\n", own.map(|v| format!("{v} (0x{v:02x})")).unwrap_or("?".into()));
    c.out += &format!("; code 0000-{:04x}  pad {:04x}-{:04x}  text {:04x}-{:04x} ({} strings)  file size {:04x}\n",
        s.code_end, s.code_end, sc::TEXT_BASE.min(file.len()), sc::TEXT_BASE.min(file.len()), s.text_end, s.strings.len(), file.len());
    if !bad.is_empty() { c.out += &format!("; WARNING labels not at node starts: {bad:x?}\n") }
    for n in &s.nodes { c.node(n, 0) }
    c.out += "\n; ---- strings (offset rel. 0x3800; * = not referenced by an operand in this file) ----\n";
    for (o, raw) in &s.strings {
        let mark = if text_refs.contains(o) { ' ' } else { '*' };
        c.out += &format!("{mark}@{o:04x} \"{}\"\n", sc::decode_text(raw).replace('\n', "\\n"));
    }
    Ok((c.out, st))
}

#[derive(Default)]
struct Stats {
    files: usize,
    code_bytes: usize,
    nodes: usize,
    strings: usize,
    text_refs: usize,
    text_refs_ok: usize,
    unreferenced_strings: usize,
    bad_labels: usize,
    op_hist: Vec<usize>,
}

impl Stats {
    fn add(&mut self, o: &Stats) {
        self.files += o.files; self.code_bytes += o.code_bytes; self.nodes += o.nodes; self.strings += o.strings;
        self.text_refs += o.text_refs; self.text_refs_ok += o.text_refs_ok; self.unreferenced_strings += o.unreferenced_strings;
        self.bad_labels += o.bad_labels;
        for i in 0..256 { self.op_hist[i] += o.op_hist[i] }
    }
}

fn disasm_one(path: &str, out: Option<&str>) -> Result<(), String> {
    let f = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let sub = Path::new(path).with_file_name("SUBCODE.BIN");
    let spk = fs::read(sub).map(|b| sc::speaker_names(&b)).unwrap_or_default();
    let (txt, _) = disasm_bytes(path, &f, file_id_from_name(path), &BTreeSet::new(), &spk)?;
    match out {
        Some(o) => fs::write(o, txt).map_err(|e| e.to_string()),
        None => { print!("{txt}"); Ok(()) }
    }
}

fn disasm_all(dir: &str, outdir: &str) -> Result<(), String> {
    fs::create_dir_all(outdir).map_err(|e| e.to_string())?;
    let mut files = vec![];
    for n in 0..=38u8 {
        let p = format!("{dir}/SP{n:02}.BIN");
        if let Ok(b) = fs::read(&p) { files.push((n, p, b)) }
    }
    // cross-file entry points: JUMP/CALL (file,label)
    let mut ext: BTreeMap<u16, BTreeSet<u16>> = BTreeMap::new();
    let mut xref_lines = String::new();
    for (n, p, b) in &files {
        let s = sc::parse_script(b).map_err(|e| format!("{p}: {e}"))?;
        for node in &s.nodes {
            sc::walk(node, &mut |x: &Node| {
                if matches!(x.op, 0x0B | 0x2C) {
                    if let Some((f, l)) = file_label(x) {
                        ext.entry(f).or_default().insert(l);
                        xref_lines += &format!("SP{n:02}:{:04x} {} {}:{l:04x}\n", x.offset, if x.op == 0x0B { "CALL" } else { "JUMP" }, file_name(f));
                    }
                }
            });
        }
    }
    let spk = fs::read(format!("{dir}/SUBCODE.BIN")).map(|b| sc::speaker_names(&b)).unwrap_or_default();
    let mut tot = Stats { op_hist: vec![0; 256], ..Default::default() };
    for (n, p, b) in &files {
        let own = sc::sp_file_id(*n);
        let (txt, st) = disasm_bytes(p, b, Some(own), ext.get(&own).unwrap_or(&BTreeSet::new()), &spk)?;
        fs::write(format!("{outdir}/SP{n:02}.txt"), txt).map_err(|e| e.to_string())?;
        tot.add(&st);
    }
    fs::write(format!("{outdir}/xrefs.txt"), xref_lines).map_err(|e| e.to_string())?;
    // variable cross reference: desc -> (writes, reads) locations
    let mut vars: BTreeMap<u16, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for (n, p, b) in &files {
        let s = sc::parse_script(b).map_err(|e| format!("{p}: {e}"))?;
        for node in &s.nodes {
            sc::walk(node, &mut |x: &Node| {
                for (i, o) in x.operands.iter().enumerate() {
                    if let Operand::Var(d) = o {
                        let w = i == 0 && matches!(x.op, 0x0E | 0x13 | 0x14 | 0x1C | 0x1D | 0x3E | 0x3F);
                        let e = vars.entry(*d).or_default();
                        let loc = format!("SP{n:02}:{:04x}/{}", x.offset, sc::MNEMONICS[x.op as usize]);
                        if w { e.0.push(loc) } else { e.1.push(loc) }
                    }
                }
            });
        }
    }
    let mut vt = String::from("; variable cross reference (desc  name  #writes #reads  first locations)\n");
    for (d, (w, r)) in &vars {
        vt += &format!("{d:04x} {:<14} W{:<4} R{:<4} W:{} R:{}\n", sc::var_name(*d), w.len(), r.len(),
            w.iter().take(4).cloned().collect::<Vec<_>>().join(","), r.iter().take(4).cloned().collect::<Vec<_>>().join(","));
    }
    fs::write(format!("{outdir}/vars.txt"), vt).map_err(|e| e.to_string())?;
    let mut rep = format!(
        "files {}  code bytes {} (100% parsed, padding verified)  nodes {}  strings {}\n\
         text operands {} (resolving to string starts: {})  strings not referenced by any operand in their file: {}\n\
         labels not at node starts: {}\nopcode histogram:\n",
        tot.files, tot.code_bytes, tot.nodes, tot.strings, tot.text_refs, tot.text_refs_ok, tot.unreferenced_strings, tot.bad_labels);
    for i in 0..sc::OP_COUNT {
        rep += &format!("  {i:02x} {:<9} {}\n", sc::MNEMONICS[i], tot.op_hist[i]);
    }
    let unknown: usize = tot.op_hist[sc::OP_COUNT..].iter().sum();
    rep += &format!("unknown opcodes: {unknown}\n");
    fs::write(format!("{outdir}/summary.txt"), &rep).map_err(|e| e.to_string())?;
    print!("{rep}");
    Ok(())
}

fn strings(path: &str) -> Result<(), String> {
    let f = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let s = sc::parse_script(&f)?;
    for (o, raw) in &s.strings {
        println!("@{o:04x} {}", sc::decode_text(raw).replace('\n', "\\n"));
    }
    Ok(())
}

/// Headless run of the reference VM: prints dialogue, scenes, menus.
/// `choices`: comma-separated 1-based menu picks (`c` = cancel); when exhausted, picks 1.
fn run(dir: &str, choices: &str, max_passes: usize, start: Option<&str>) -> Result<(), String> {
    use sc::vm::{Host, PassEnd, Vm, SCRIPT_ADDR};
    use std::collections::HashMap;
    #[derive(Default)]
    struct H { m: HashMap<u32, u16> }
    impl Host for H {
        fn peek(&mut self, a: u32) -> u16 { *self.m.get(&a).unwrap_or(&0) }
        fn poke(&mut self, a: u32, v: u16) { self.m.insert(a, v); }
        fn poke_byte(&mut self, a: u32, v: u8) { self.m.insert(a, v as u16); }
    }
    let load = |id: u16| -> Result<Vec<u8>, String> {
        let p = format!("{dir}/{}.BIN", file_name(id));
        fs::read(&p).map_err(|e| format!("{p}: {e}"))
    };
    let mut rnd: Option<u64> = choices.strip_prefix("random:").and_then(|x| x.parse().ok());
    let quiet = rnd.is_some();
    let mut picks = choices.split(',').filter(|x| !x.is_empty()).map(|x| x.to_string()).collect::<Vec<_>>().into_iter();
    let (mut vm, mut q) = Vm::new_game();
    if let Some(st) = start {
        // "SPnn:hhhh" – start at a script/pc instead of the new-game entry
        let (f, pc) = st.split_once(':').ok_or("start must be SPnn:pc")?;
        let id = sc::FILE_NAMES.iter().position(|n| *n == f).ok_or("unknown script")? as u16;
        vm.script_id = id;
        vm.pc = u16::from_str_radix(pc, 16).map_err(|e| e.to_string())?;
        q = vec![sc::vm::Action { cmd: 0x11, args: vec![1, 0xA800, id] }];
    }
    let mut h = H::default();
    let mut script = Vec::new();
    for pass in 0..max_passes {
        for a in &q {
            if quiet && a.cmd != 0x11 { continue }
            match a.cmd {
                0x11 if (a.args[0] as u32) << 16 | a.args[1] as u32 == SCRIPT_ADDR => {
                    script = load(a.args[2])?;
                    println!("  [load script {}]", file_name(a.args[2]));
                }
                0x11 => println!("  [load {} to {:x}]", file_name(a.args[2]), (a.args[0] as u32) << 16 | a.args[1] as u32),
                0x08 => {
                    let t = sc::text_at(&script, a.args[1]).map(sc::decode_text).unwrap_or_default();
                    println!("  SAY[{}] {}", a.args[0], t.replace('\n', " "));
                }
                0x12 => println!("  [image {}]", a.args[0]),
                0x07 => println!("  [music {:x}]", a.args[0]),
                0x10 => println!("  [wait {}]", a.args[0]),
                c => println!("  [Q {c:02x} {:x?}]", a.args),
            }
        }
        if vm.state != 2 { println!("state -> {} at pass {pass}", vm.state); break }
        match vm.finish_pass() {
            PassEnd::Continue => {}
            PassEnd::Menu(items) => {
                let names: Vec<String> = items.iter().map(|&t| sc::text_at(&script, t).map(sc::decode_text).unwrap_or_default()).collect();
                let sel = if let Some(seed) = rnd.as_mut() {
                    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    let r = (*seed >> 33) as usize;
                    let ok: Vec<usize> = (0..names.len()).filter(|&i| !names[i].contains("SAVE") && !names[i].contains("FINISH")).collect();
                    if r % 8 == 0 || ok.is_empty() { None } else { Some(ok[r % ok.len()]) }
                } else {
                    let p = picks.next();
                    match p.as_deref() { Some("c") => None, Some(n) => n.parse::<usize>().ok().map(|n| n - 1), None => Some(0) }
                };
                if !quiet { println!("MENU {:?} -> {}", names, sel.map(|i| names.get(i).cloned().unwrap_or_default()).unwrap_or("cancel".into())); }
                vm.choose(sel);
            }
        }
        if !quiet { println!("-- pass {pass} pc {} {:04x} path {:x?}", file_name(vm.script_id), vm.pc, vm.path.items); }
        q = vm.run_pass(&script, &mut h);
    }
    Ok(())
}
