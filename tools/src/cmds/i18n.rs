//! Translation tooling (docs/i18n.md): context dumps, import of translations, the validator `i18n-check`,
//! the pack builder for the engine and a few measuring helpers.
//!
//!   snt i18n-ctx    <files-dir> <SPnn> [from-off [to-off]]     context dump of the English text (work file, not committed)
//!   snt i18n-import <lang-dir> <files-dir> <patch.tsv>         merge `SPnn@off<TAB>text` lines into <lang-dir>/SPnn.json (hash filled in)
//!   snt i18n-check  <lang-dir> <files-dir> [SPnn ...|all]      the objective gate: keys, hashes, tokens, characters, box geometry
//!   snt i18n-build  <lang-dir> <pack-out>                      compact translation pack read by the engine
//!   snt i18n-stats  <files-dir>                                widths of the original text (measuring aid)
//!   snt i18n-sheet  <files-dir> <out.png> [text ...]           render the font + pt-BR glyphs and sample strings
//!   snt i18n-preview <lang-dir> <files-dir> <out.png> <SPnn> [off|longest:N|sample:N ...]   draw translated messages / menu items with the real text renderer
//!   snt i18n-apply-menu <lang-dir> <files-dir> [--force]       fill common menu labels from <lang-dir>/menu.json (keyed by hash)
//!   snt i18n-status <lang-dir> <files-dir>                     translated / total per file
use sncore::i18n as i;
use sncore::script::{self as sc, Node, Operand};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::Path;

pub fn dispatch(args: &[String]) -> Option<Result<(), String>> {
    let a = |n: usize| args.get(n).map(|s| s.as_str());
    match args[0].as_str() {
        "i18n-ctx" if args.len() >= 3 => Some(ctx(&args[1], &args[2], a(3), a(4))),
        "i18n-import" if args.len() == 4 => Some(import(&args[1], &args[2], &args[3])),
        "i18n-check" if args.len() >= 3 => Some(check(&args[1], &args[2], &args[3..])),
        "i18n-build" if args.len() == 3 => Some(build(&args[1], &args[2])),
        "i18n-stats" if args.len() == 2 => Some(stats(&args[1])),
        "i18n-sheet" if args.len() >= 3 => Some(sheet(&args[1], &args[2], &args[3..])),
        "i18n-status" if args.len() == 3 => Some(status(&args[1], &args[2])),
        "i18n-names" if args.len() == 2 => Some(names_dump(&args[1])),
        "i18n-preview" if args.len() >= 5 => Some(preview(&args[1], &args[2], &args[3], &args[4], &args[5..])),
        "i18n-labels" if args.len() == 2 => Some(labels(&args[1])),
        "i18n-apply-menu" if args.len() >= 3 => Some(apply_menu(&args[1], &args[2], args.get(3).map(|s| s == "--force").unwrap_or(false))),
        "i18n-learn-menu" if args.len() == 3 => Some(learn_menu(&args[1], &args[2])),
        "i18n-kinds" if args.len() == 2 => Some(kind_counts(&args[1])),
        _ => None,
    }
}

// ---------------------------------------------------------------- minimal JSON (objects of strings / nested objects only)

#[derive(Debug, Clone, PartialEq)]
pub enum J { Str(String), Obj(Vec<(String, J)>) }

struct P<'a> { s: &'a [u8], p: usize }
impl<'a> P<'a> {
    fn ws(&mut self) { while self.p < self.s.len() && (self.s[self.p] as char).is_whitespace() { self.p += 1; } }
    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.s.get(self.p) == Some(&c) { self.p += 1; Ok(()) } else { Err(format!("json: expected '{}' at byte {}", c as char, self.p)) }
    }
    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = *self.s.get(self.p).ok_or("json: unterminated string")?;
            self.p += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.p).ok_or("json: bad escape")?;
                    self.p += 1;
                    match e {
                        b'n' => out.push(b'\n'), b't' => out.push(b'\t'), b'r' => out.push(b'\r'),
                        b'"' => out.push(b'"'), b'\\' => out.push(b'\\'), b'/' => out.push(b'/'),
                        b'u' => {
                            let h = std::str::from_utf8(self.s.get(self.p..self.p + 4).ok_or("json: bad \\u")?).map_err(|e| e.to_string())?;
                            let cp = u32::from_str_radix(h, 16).map_err(|e| e.to_string())?;
                            self.p += 4;
                            let ch = char::from_u32(cp).ok_or("json: bad \\u code")?;
                            let mut b = [0u8; 4]; out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                        }
                        _ => return Err("json: unknown escape".into()),
                    }
                }
                _ => out.push(c),
            }
        }
        String::from_utf8(out).map_err(|e| e.to_string())
    }
    fn value(&mut self) -> Result<J, String> {
        self.ws();
        match self.s.get(self.p) {
            Some(b'"') => Ok(J::Str(self.string()?)),
            Some(b'{') => {
                self.p += 1;
                let mut v = Vec::new();
                self.ws();
                if self.s.get(self.p) == Some(&b'}') { self.p += 1; return Ok(J::Obj(v)); }
                loop {
                    let k = self.string()?;
                    self.eat(b':')?;
                    v.push((k, self.value()?));
                    self.ws();
                    match self.s.get(self.p) { Some(b',') => self.p += 1, Some(b'}') => { self.p += 1; break; } _ => return Err(format!("json: expected , or }} at byte {}", self.p)) }
                }
                Ok(J::Obj(v))
            }
            _ => Err(format!("json: unexpected input at byte {}", self.p)),
        }
    }
}

pub fn json_parse(s: &str) -> Result<J, String> {
    let mut p = P { s: s.as_bytes(), p: 0 };
    let v = p.value()?;
    p.ws();
    if p.p != s.len() { return Err("json: trailing data".into()); }
    Ok(v)
}

fn jstr(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), '\t' => o.push_str("\\t"), c => o.push(c) }
    }
    o.push('"');
    o
}

// ---------------------------------------------------------------- translation files

/// One translation entry: hash of the English string and the markup text.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry { pub hash: u32, pub text: String }

pub type FileTr = BTreeMap<u16, Entry>;

fn lang_path(dir: &str, name: &str) -> String { format!("{dir}/{name}.json") }

fn read_entries(path: &str) -> Result<FileTr, String> {
    let Ok(txt) = fs::read_to_string(path) else { return Ok(FileTr::new()) };
    let J::Obj(top) = json_parse(&txt).map_err(|e| format!("{path}: {e}"))? else { return Err(format!("{path}: not an object")) };
    let mut out = FileTr::new();
    for (k, v) in top {
        if k != "entries" { continue; }
        let J::Obj(es) = v else { return Err(format!("{path}: entries is not an object")) };
        for (ko, e) in es {
            let off = u16::from_str_radix(&ko, 16).map_err(|_| format!("{path}: bad key {ko:?}"))?;
            let J::Obj(f) = e else { return Err(format!("{path}: entry {ko} is not an object")) };
            let (mut h, mut t) = (None, None);
            for (fk, fv) in f {
                match (fk.as_str(), fv) {
                    ("h", J::Str(s)) => h = u32::from_str_radix(&s, 16).ok(),
                    ("t", J::Str(s)) => t = Some(s),
                    _ => {}
                }
            }
            out.insert(off, Entry { hash: h.ok_or(format!("{path}: entry {ko} has no hash"))?, text: t.ok_or(format!("{path}: entry {ko} has no text"))? });
        }
    }
    Ok(out)
}

fn write_entries(path: &str, file: &str, e: &FileTr) -> Result<(), String> {
    let mut s = format!("{{\n  \"file\": {},\n  \"lang\": \"pt-BR\",\n  \"entries\": {{\n", jstr(file));
    let n = e.len();
    for (k, (off, en)) in e.iter().enumerate() {
        s += &format!("    \"{off:04x}\": {{\"h\": \"{:08x}\", \"t\": {}}}{}\n", en.hash, jstr(&en.text), if k + 1 < n { "," } else { "" });
    }
    s += "  }\n}\n";
    fs::write(path, s).map_err(|e| format!("{path}: {e}"))
}

// ---------------------------------------------------------------- script structure (roles and context)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Say, Choice }

#[derive(Debug, Clone)]
pub struct Item {
    pub off: u16,
    pub kind: Kind,
    /// SAY speaker value (0 none, 1 narration, >= 2 named); 0 for choices
    pub spk: u8,
    /// offsets of the enclosing CHOICE labels
    pub path: Vec<u16>,
    pub scene: String,
    pub voice: Option<u16>,
    pub label: String,
    /// CHOICE belongs to a text-entry menu (keypad / keyboard, 9+ siblings): must stay English
    pub entry: bool,
}

#[allow(dead_code)]
pub struct FileData {
    pub sp: u8,
    pub strings: Vec<(u16, Vec<u8>)>,
    /// items in reading (depth-first) order
    pub items: Vec<Item>,
}

struct W<'a> {
    by_off: HashMap<usize, &'a Node>,
    seen: HashSet<usize>,
    items: Vec<Item>,
    scene: String,
    voice: Option<u16>,
    label: String,
    path: Vec<u16>,
}

impl<'a> W<'a> {
    fn visit(&mut self, n: &'a Node) {
        if !self.seen.insert(n.offset) { return; }
        match n.op {
            0x38 => {
                if let (Some(Operand::Text(t)), body) = (n.operands.first(), n.operands.get(1)) {
                    self.items.push(Item { off: *t, kind: Kind::Choice, spk: 0, path: self.path.clone(), scene: self.scene.clone(), voice: None, label: self.label.clone(), entry: false });
                    self.path.push(*t);
                    if let Some(Operand::Node(b)) = body { self.visit(b); }
                    self.path.pop();
                }
            }
            0x43 | 0x44 => {
                let (spk, text) = match n.operands.first() {
                    Some(Operand::Node(t)) if t.op != 0x44 => {
                        let s = match t.operands.first() { Some(Operand::Imm(v)) | Some(Operand::Res(v)) => *v as u8, _ => 0 };
                        (s, t.operands.get(1).cloned())
                    }
                    Some(o) => (0, Some(o.clone())),
                    None => (0, None),
                };
                if let Some(Operand::Text(t)) = text {
                    let v = self.voice.take();
                    self.items.push(Item { off: t, kind: Kind::Say, spk, path: self.path.clone(), scene: self.scene.clone(), voice: v, label: self.label.clone(), entry: false });
                }
            }
            0x3D => {
                if let Some(Operand::Res(v)) = n.operands.first() {
                    self.scene = format!("{}#{}", sc::FILE_NAMES.get((*v >> 8) as usize).copied().unwrap_or("?"), v & 0xFF);
                }
            }
            0x04 => { if let Some(Operand::Res(v)) = n.operands.first() { self.voice = Some(*v); } }
            _ => {
                for o in &n.operands { if let Operand::Node(c) = o { self.visit(c); } }
            }
        }
        if let Some(l) = n.link {
            if let Some(t) = self.by_off.get(&(l as usize)).copied() { self.visit(t); }
        }
    }
}

pub fn load_file(dir: &str, spn: u8) -> Result<FileData, String> {
    let p = format!("{dir}/SP{spn:02}.BIN");
    let raw = fs::read(&p).map_err(|e| format!("{p}: {e}"))?;
    let s = sc::parse_script(&raw)?;
    let mut by_off: HashMap<usize, &Node> = HashMap::new();
    for n in &s.nodes { sc::walk(n, &mut |x: &Node| { by_off.insert(x.offset, x); }); }
    let mut link_targets = HashSet::new();
    for n in &s.nodes { sc::walk(n, &mut |x: &Node| { if let Some(l) = x.link { link_targets.insert(l as usize); } }); }
    let mut w = W { by_off, seen: HashSet::new(), items: vec![], scene: String::new(), voice: None, label: String::new(), path: vec![] };
    let mut roots: Vec<&Node> = s.nodes.iter().filter(|n| !link_targets.contains(&n.offset)).collect();
    roots.extend(s.nodes.iter().filter(|n| link_targets.contains(&n.offset)));
    for n in roots {
        if w.seen.contains(&n.offset) { continue; }
        w.label = format!("L_{:04x}", n.offset);
        w.voice = None;
        w.path.clear();
        w.visit(n);
    }
    let mut items = w.items;
    // entry menus: 9 or more CHOICE siblings (same path and root) whose labels are single characters / digits / words typed by the player
    let mut groups: HashMap<(Vec<u16>, String), usize> = HashMap::new();
    for it in items.iter().filter(|x| x.kind == Kind::Choice) { *groups.entry((it.path.clone(), it.label.clone())).or_default() += 1; }
    for it in items.iter_mut().filter(|x| x.kind == Kind::Choice) {
        if groups[&(it.path.clone(), it.label.clone())] >= 9 { it.entry = true; }
    }
    Ok(FileData { sp: spn, strings: s.strings, items })
}

fn font_widths(dir: &str) -> Result<Vec<u8>, String> {
    let p = format!("{dir}/SUBCODE.BIN");
    let sub = fs::read(&p).map_err(|e| format!("{p}: {e}"))?;
    let font = sncore::actions::Font::from_subcode(&sub);
    Ok(font.widths)
}

fn spk_names(dir: &str) -> Vec<String> {
    let sub = fs::read(format!("{dir}/SUBCODE.BIN")).unwrap_or_default();
    if sub.is_empty() { return vec![]; }
    sncore::actions::speaker_names(&sub).into_iter().map(|(_, n)| String::from_utf8_lossy(&n).to_string()).collect()
}

fn spk_label(names: &[String], spk: u8) -> String {
    match spk { 0 => "-".into(), 1 => "narration".into(), n => names.get(n as usize - 1).filter(|s| !s.is_empty()).cloned().unwrap_or(format!("spk{n}")) }
}

fn sp_arg(s: &str) -> Result<u8, String> {
    s.to_ascii_uppercase().strip_prefix("SP").and_then(|n| n.parse().ok()).ok_or(format!("bad file name {s:?}"))
}

fn raw_of<'a>(fd: &'a FileData) -> HashMap<u16, &'a [u8]> { fd.strings.iter().map(|(o, r)| (*o, r.as_slice())).collect() }

// ---------------------------------------------------------------- i18n-ctx

fn ctx(dir: &str, spn: &str, from: Option<&str>, to: Option<&str>) -> Result<(), String> {
    let n = sp_arg(spn)?;
    let fd = load_file(dir, n)?;
    let raw = raw_of(&fd);
    let names = spk_names(dir);
    let from = from.map(|s| u16::from_str_radix(s, 16)).transpose().map_err(|e| e.to_string())?.unwrap_or(0);
    let to = to.map(|s| u16::from_str_radix(s, 16)).transpose().map_err(|e| e.to_string())?.unwrap_or(0xFFFF);
    let widths = font_widths(dir)?;
    let pt = match std::env::var("I18N_LANG") { Ok(l) => Some(read_entries(&lang_path(&l, &format!("SP{n:02}")))?), Err(_) => None };
    let hide_done = std::env::var("I18N_HIDE_DONE").is_ok();
    let mut lastlabel = String::new();
    let mut lastscene = String::new();
    let mut printed: HashSet<u16> = HashSet::new();
    for it in &fd.items {
        if it.off < from || it.off > to { continue; }
        if hide_done && pt.as_ref().map_or(false, |p| p.contains_key(&it.off)) { continue; }
        if it.label != lastlabel || it.scene != lastscene {
            println!("\n-- {} scene {}", it.label, if it.scene.is_empty() { "?" } else { &it.scene });
            lastlabel = it.label.clone(); lastscene = it.scene.clone();
        }
        let ind = "  ".repeat(it.path.len());
        let r = raw.get(&it.off).copied().unwrap_or(&[]);
        let m = i::bytes_to_markup(r);
        match it.kind {
            Kind::Choice => {
                if hide_done && !printed.insert(it.off) { continue; }
                let (w, bytes) = i::menu_width(r, &widths);
                println!("{ind}@{:04x} ? {m}   [menu {w}px {bytes}b]{}", it.off, if it.entry { "  [ENTRY MENU: keep]" } else { "" });
            }
            Kind::Say => {
                let l = i::layout_message(r, it.spk, &widths);
                let v = it.voice.map(|v| format!(" v{v:04x}")).unwrap_or_default();
                println!("{ind}@{:04x} [{}]{v} {m}   <{}p>", it.off, spk_label(&names, it.spk), l.pages);
            }
        }
        if let Some(pt) = &pt {
            match pt.get(&it.off) {
                Some(e) => println!("{ind}      PT: {}", e.text),
                None => println!("{ind}      PT: (missing)"),
            }
        }
    }
    let seen: BTreeSet<u16> = fd.items.iter().map(|x| x.off).collect();
    let missing: Vec<_> = fd.strings.iter().filter(|(o, _)| !seen.contains(o)).map(|(o, _)| format!("{o:04x}")).collect();
    if !missing.is_empty() { println!("\n-- strings not reached by the walk: {}", missing.join(" ")); }
    Ok(())
}

// ---------------------------------------------------------------- i18n-import

fn import(lang: &str, dir: &str, patch: &str) -> Result<(), String> {
    fs::create_dir_all(lang).map_err(|e| e.to_string())?;
    let txt = fs::read_to_string(patch).map_err(|e| format!("{patch}: {e}"))?;
    let mut per: BTreeMap<u8, Vec<(u16, String)>> = BTreeMap::new();
    let mut names: Vec<(u8, String)> = Vec::new();
    for (ln, line) in txt.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let (k, t) = line.split_once('\t').ok_or(format!("{patch}:{}: no TAB", ln + 1))?;
        if let Some(n) = k.strip_prefix("NAME@") {
            names.push((n.parse().map_err(|_| format!("{patch}:{}: bad name index", ln + 1))?, t.to_string()));
            continue;
        }
        let (f, o) = k.split_once('@').ok_or(format!("{patch}:{}: key must be SPnn@off", ln + 1))?;
        per.entry(sp_arg(f)?).or_default().push((u16::from_str_radix(o, 16).map_err(|_| format!("{patch}:{}: bad offset", ln + 1))?, t.to_string()));
    }
    let mut count = 0;
    for (n, list) in per {
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        let widths = font_widths(dir)?;
        let path = lang_path(lang, &format!("SP{n:02}"));
        let mut e = read_entries(&path)?;
        let by: HashMap<u16, &Item> = fd.items.iter().map(|x| (x.off, x)).collect();
        for (off, t) in list {
            let r = raw.get(&off).ok_or(format!("SP{n:02}@{off:04x}: no such string"))?;
            // a leading `~` asks for automatic line breaks (greedy fill of the real box width)
            let t = match t.strip_prefix('~') {
                Some(plain) => match by.get(&off).map(|x| (x.kind, x.spk)) {
                    Some((Kind::Say, spk)) => reflow(plain, spk, &widths)?,
                    _ => plain.to_string(),
                },
                None => t,
            };
            e.insert(off, Entry { hash: i::hash32(r), text: t });
            count += 1;
        }
        write_entries(&path, &format!("SP{n:02}"), &e)?;
    }
    if !names.is_empty() {
        let path = lang_path(lang, "names");
        let mut cur = read_names(&path)?;
        for (n, t) in names { cur.insert(n, t); count += 1; }
        write_names(&path, &cur)?;
    }
    println!("imported {count} entries");
    Ok(())
}

/// Greedy line breaking of a message to the real box width (existing `{nl}` are treated as spaces, `{page}` is kept).
pub fn reflow(text: &str, spk: u8, widths: &[u8]) -> Result<String, String> {
    let bytes = i::markup_to_bytes(text)?;
    let (x0, right) = if spk == 1 { (i::X_NARRATION, i::RIGHT_NARRATION) } else { (i::X_SPEECH, i::RIGHT_SPEECH) };
    let limit = right - x0;
    let mut out: Vec<u8> = vec![];
    for (si, seg) in bytes.split(|&c| c == i::PAGE).enumerate() {
        if si > 0 { out.push(i::PAGE); }
        let mut cur = 0u32;
        let space = i::advance(widths, b' ');
        let words: Vec<Vec<u8>> = seg.split(|&c| c == b' ' || c == i::NL).filter(|w| !w.is_empty()).map(|w| w.to_vec()).collect();
        for (k, w) in words.iter().enumerate() {
            let ww: u32 = w.iter().map(|&c| i::advance(widths, c)).sum();
            if k > 0 {
                if cur + space + ww > limit { out.push(i::NL); cur = 0; } else { out.push(b' '); cur += space; }
            }
            out.extend_from_slice(w);
            cur += ww;
        }
    }
    Ok(i::bytes_to_markup(&out))
}

fn read_names(path: &str) -> Result<BTreeMap<u8, String>, String> {
    let Ok(txt) = fs::read_to_string(path) else { return Ok(BTreeMap::new()) };
    let J::Obj(top) = json_parse(&txt)? else { return Err("names: not an object".into()) };
    let mut m = BTreeMap::new();
    for (k, v) in top {
        if k != "names" { continue; }
        if let J::Obj(es) = v { for (ko, e) in es { if let J::Str(s) = e { m.insert(ko.parse().map_err(|_| "names: bad key")?, s); } } }
    }
    Ok(m)
}

fn write_names(path: &str, m: &BTreeMap<u8, String>) -> Result<(), String> {
    let mut s = String::from("{\n  \"lang\": \"pt-BR\",\n  \"names\": {\n");
    let n = m.len();
    for (k, (i, t)) in m.iter().enumerate() { s += &format!("    \"{i}\": {}{}\n", jstr(t), if k + 1 < n { "," } else { "" }); }
    s += "  }\n}\n";
    fs::write(path, s).map_err(|e| format!("{path}: {e}"))
}

fn names_dump(dir: &str) -> Result<(), String> {
    for (i, n) in spk_names(dir).iter().enumerate() { println!("{:2} {n}", i + 1); }
    Ok(())
}

// ---------------------------------------------------------------- i18n-check

/// Validation limits (documented in docs/i18n.md, measured with `i18n-stats`).

#[derive(Default)]
struct Report { fails: Vec<String>, warns: Vec<String>, checked: usize }

fn tokens(raw: &[u8]) -> (Vec<u8>, Vec<u8>, usize, usize) {
    // (colour sequence, button placeholders, auto count, page count)
    let mut col = vec![]; let mut btn = vec![]; let (mut auto, mut page) = (0, 0);
    for &c in raw {
        match c { 0xF8..=0xFE => col.push(c & 7), 0xEC | 0xEE | 0xF0 => btn.push(c), i::AUTO => auto += 1, i::PAGE => page += 1, _ => {} }
    }
    btn.sort();
    (col, btn, auto, page)
}

fn check_file(dir: &str, lang: &str, spn: u8, widths: &[u8], names: &[String], rep: &mut Report) -> Result<(usize, usize), String> {
    let fd = load_file(dir, spn)?;
    let raw = raw_of(&fd);
    let by_off: HashMap<u16, &Item> = fd.items.iter().map(|x| (x.off, x)).collect();
    let e = read_entries(&lang_path(lang, &format!("SP{spn:02}")))?;
    let tag = |off: u16| format!("SP{spn:02}@{off:04x}");
    for (off, en) in &e {
        rep.checked += 1;
        let t = tag(*off);
        let Some(orig) = raw.get(off) else { rep.fails.push(format!("{t}: key does not exist in the English file")); continue };
        if i::hash32(orig) != en.hash { rep.fails.push(format!("{t}: hash mismatch (English changed or wrong key): file {:08x} entry {:08x}", i::hash32(orig), en.hash)); continue; }
        let bytes = match i::markup_to_bytes(&en.text) { Ok(b) => b, Err(m) => { rep.fails.push(format!("{t}: {m}")); continue } };
        let Some(it) = by_off.get(off) else { rep.fails.push(format!("{t}: string not used by any SAY/CHOICE")); continue };
        if it.entry { rep.fails.push(format!("{t}: text-entry menu item must stay English")); continue; }
        let (oc, ob, oa, op) = tokens(orig);
        let (pc, pb, pa, pp) = tokens(&bytes);
        if oc != pc { rep.fails.push(format!("{t}: colour sequence differs (EN {oc:?} PT {pc:?})")); }
        if ob != pb { rep.fails.push(format!("{t}: button placeholders differ (EN {ob:02x?} PT {pb:02x?})")); }
        if oa != pa { rep.fails.push(format!("{t}: auto-advance {{auto}} count differs (EN {oa} PT {pa})")); }
        if op != pp { rep.fails.push(format!("{t}: page break {{page}} count differs (EN {op} PT {pp})")); }
        match it.kind {
            Kind::Choice => {
                if bytes.contains(&i::NL) || bytes.contains(&i::PAGE) { rep.fails.push(format!("{t}: menu label has a line or page break")); }
                let (w, n) = i::menu_width(&bytes, widths);
                let (ow, _) = i::menu_width(orig, widths);
                if n > i::MENU_MAX_BYTES { rep.fails.push(format!("{t}: menu label has {n} bytes, the menu draws at most {}", i::MENU_MAX_BYTES)); }
                if w > i::RIGHT_MENU { rep.fails.push(format!("{t}: menu label ends at x={w} px (starts at x={}), limit {} (EN {ow})", i::X_MENU, i::RIGHT_MENU)); }
            }
            Kind::Say => {
                let l = i::layout_message(&bytes, it.spk, widths);
                let lo = i::layout_message(orig, it.spk, widths);
                for (k, line) in l.lines.iter().enumerate() {
                    let (x0, right) = if it.spk == 1 { (i::X_NARRATION, i::RIGHT_NARRATION) } else { (i::X_SPEECH, i::RIGHT_SPEECH) };
                    if line.x_end > right {
                        rep.fails.push(format!("{t}: line {} is {} px wide, limit {} ({}): \"{}\"", k + 1, line.x_end - x0, right - x0, spk_label(names, it.spk), i::bytes_to_markup(&line.text)));
                    }
                    if line.cut { rep.fails.push(format!("{t}: line {} reaches the 50 character cut", k + 1)); }
                }
                if l.pages > lo.pages {
                    let msg = format!("{t}: {} pages, English has {} ({})", l.pages, lo.pages, spk_label(names, it.spk));
                    if pa > 0 { rep.fails.push(msg + " and the line is auto-advance (timing)"); } else { rep.warns.push(msg); }
                }
            }
        }
    }
    Ok((e.len(), fd.strings.len()))
}

fn check(lang: &str, dir: &str, which: &[String]) -> Result<(), String> {
    let widths = font_widths(dir)?;
    let names = spk_names(dir);
    let list: Vec<u8> = if which.is_empty() || which[0] == "all" { (0..=38).collect() } else { which.iter().map(|s| sp_arg(s)).collect::<Result<_, _>>()? };
    let mut rep = Report::default();
    let (mut tr, mut tot) = (0, 0);
    for n in list {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let (a, b) = check_file(dir, lang, n, &widths, &names, &mut rep)?;
        tr += a; tot += b;
    }
    // names
    for (idx, t) in read_names(&lang_path(lang, "names"))? {
        match i::markup_to_bytes(&t) {
            Ok(b) => {
                if b.len() > 0x19 { rep.fails.push(format!("name {idx}: {} bytes, limit 25", b.len())); }
                if b.iter().any(|&c| c < 0x20 || (0xF8..=0xFF).contains(&c)) { rep.fails.push(format!("name {idx}: control tokens not allowed")); }
            }
            Err(m) => rep.fails.push(format!("name {idx}: {m}")),
        }
    }
    for f in &rep.fails { println!("FAIL {f}"); }
    for w in &rep.warns { println!("warn {w}"); }
    println!("i18n-check: {} entries checked, {} failures, {} warnings ({tr} of {tot} strings translated in the files examined)", rep.checked, rep.fails.len(), rep.warns.len());
    if rep.fails.is_empty() { Ok(()) } else { Err(format!("{} failures", rep.fails.len())) }
}

// ---------------------------------------------------------------- i18n-build / status

fn build(lang: &str, out: &str) -> Result<(), String> {
    let mut t = i::Translation::default();
    let mut n = 0;
    for sp in 0..=38u8 {
        let e = read_entries(&lang_path(lang, &format!("SP{sp:02}")))?;
        if e.is_empty() { continue; }
        let m = t.files.entry(sp).or_default();
        for (off, en) in e { m.insert(off, i::markup_to_bytes(&en.text).map_err(|e| format!("SP{sp:02}@{off:04x}: {e}"))?); n += 1; }
    }
    for (idx, s) in read_names(&lang_path(lang, "names"))? {
        // keys in names.json are speaker numbers (spk), as printed by `snt i18n-names`; the pack uses table indices (spk - 1)
        t.names.insert(idx.checked_sub(1).ok_or("name 0")?, i::markup_to_bytes(&s)?);
    }
    let b = t.build();
    if let Some(p) = Path::new(out).parent() { fs::create_dir_all(p).map_err(|e| e.to_string())?; }
    fs::write(out, &b).map_err(|e| format!("{out}: {e}"))?;
    println!("pack: {n} strings, {} names, {} bytes -> {out}", t.names.len(), b.len());
    Ok(())
}

fn status(lang: &str, dir: &str) -> Result<(), String> {
    let (mut a, mut b) = (0, 0);
    for n in 0..=38u8 {
        let p = format!("{dir}/SP{n:02}.BIN");
        if !Path::new(&p).exists() { continue; }
        let fd = load_file(dir, n)?;
        let e = read_entries(&lang_path(lang, &format!("SP{n:02}")))?;
        let entry_menu = fd.items.iter().filter(|x| x.entry).count();
        println!("SP{n:02} strings {:5} translated {:5} entry-menu {entry_menu}", fd.strings.len(), e.len());
        a += e.len(); b += fd.strings.len();
    }
    println!("total {b} translated {a}");
    Ok(())
}

// ---------------------------------------------------------------- i18n-stats

fn stats(dir: &str) -> Result<(), String> {
    let widths = font_widths(dir)?;
    let names = spk_names(dir);
    let mut maxw: BTreeMap<String, (u32, String)> = BTreeMap::new();
    let mut menu_max: Vec<(u32, String, String)> = vec![];
    let mut pages: BTreeMap<usize, usize> = BTreeMap::new();
    let mut lines_hist: BTreeMap<usize, usize> = BTreeMap::new();
    for n in 0..=38u8 {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        for it in &fd.items {
            let r = raw[&it.off];
            match it.kind {
                Kind::Say => {
                    let l = i::layout_message(r, it.spk, &widths);
                    let x0 = if it.spk == 1 { i::X_NARRATION } else { i::X_SPEECH };
                    let key = format!("say spk{}", if it.spk >= 2 { "N" } else if it.spk == 1 { "1" } else { "0" });
                    for line in &l.lines {
                        let e = maxw.entry(key.clone()).or_insert((0, String::new()));
                        if line.x_end - x0 > e.0 { *e = (line.x_end - x0, format!("SP{n:02}@{:04x} {}", it.off, i::bytes_to_markup(&line.text))); }
                    }
                    *pages.entry(l.pages).or_default() += 1;
                    let mut per_page: HashMap<usize, usize> = HashMap::new();
                    for line in &l.lines { *per_page.entry(line.page).or_default() += 1; }
                    for (_, c) in per_page { *lines_hist.entry(c).or_default() += 1; }
                }
                Kind::Choice => {
                    let (w, b) = i::menu_width(r, &widths);
                    menu_max.push((w, format!("SP{n:02}@{:04x}", it.off), format!("{} ({b}b){}", i::bytes_to_markup(r), if it.entry { " ENTRY" } else { "" })));
                }
            }
        }
    }
    println!("max message line width (px, from text start) by kind (window is {} px, text starts at {} / {}):", i::WIN_W, i::X_SPEECH, i::X_NARRATION);
    for (k, (w, s)) in &maxw { println!("  {k}: {w}  {s}"); }
    menu_max.sort_by(|a, b| b.0.cmp(&a.0));
    println!("widest menu labels (end x):");
    for (w, o, s) in menu_max.iter().take(15) { println!("  {w} {o} {s}"); }
    println!("pages histogram: {pages:?}");
    println!("lines-per-page histogram: {lines_hist:?}");
    let _ = names;
    Ok(())
}

// ---------------------------------------------------------------- i18n-sheet

fn sheet(dir: &str, out: &str, samples: &[String]) -> Result<(), String> {
    let sub = fs::read(format!("{dir}/SUBCODE.BIN")).map_err(|e| e.to_string())?;
    let font = sncore::actions::Font::from_subcode(&sub);
    // lines: every glyph from 0x20..0x7E, then the pt-BR glyphs, then the samples; drawn like the game (proportional)
    let mut lines: Vec<Vec<u8>> = vec![];
    lines.push((0x20u8..0x50).collect()); lines.push((0x50u8..0x7F).collect());

    lines.push((i::PT_BASE..i::PT_BASE + i::PT_GLYPHS.len() as u8).collect());
    for s in samples { lines.push(i::markup_to_bytes(s)?.into_iter().filter(|&c| !(c >= 0xF2 && c != 0xF0 && c != 0xEC && c != 0xEE)).collect()); }
    let scale = 4usize;
    let w = 250 * scale; let h = (lines.len() * 12 + 4) * scale;
    let mut img = vec![0u8; w * h * 4];
    for px in img.chunks_mut(4) { px.copy_from_slice(&[0x10, 0x10, 0x30, 0xFF]); }
    for (ln, l) in lines.iter().enumerate() {
        let mut x = 6usize; let y = 4 + ln * 12;
        for &c in l {
            let g = (c as usize).wrapping_sub(0x20);
            if let Some(bits) = font.glyphs.get(g) {
                for (r, &row) in bits.iter().enumerate() { for k in 0..8 {
                    if row & (0x80 >> k) != 0 {
                        for dy in 0..scale { for dx in 0..scale {
                            let (px, py) = ((x + k) * scale + dx, (y + r) * scale + dy);
                            if px < w && py < h { let o = (py * w + px) * 4; img[o..o + 4].copy_from_slice(&[0xF0, 0xF0, 0xF0, 0xFF]); }
                        } }
                    }
                } }
            }
            x += font.widths.get(g).copied().unwrap_or(8) as usize;
        }
    }
    let f = fs::File::create(out).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight);
    enc.write_header().map_err(|e| e.to_string())?.write_image_data(&img).map_err(|e| e.to_string())?;
    Ok(())
}

#[allow(dead_code)]
pub fn kind_counts(dir: &str) -> Result<(), String> {
    let mut say = 0; let mut choice = BTreeSet::new(); let mut ch_off = 0; let mut entry = BTreeSet::new(); let mut tot = 0;
    let mut say_off: HashSet<(u8, u16)> = HashSet::new();
    for n in 0..=38u8 {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        tot += fd.strings.len();
        let mut offs = HashSet::new();
        for it in &fd.items {
            match it.kind {
                Kind::Say => { say_off.insert((n, it.off)); }
                Kind::Choice => { if offs.insert(it.off) { ch_off += 1; } choice.insert(raw[&it.off].to_vec()); if it.entry { entry.insert((n, it.off)); } }
            }
        }
        say += fd.items.iter().filter(|x| x.kind == Kind::Say).count();
    }
    println!("strings {tot} say-nodes {say} unique say offsets {} choice offsets {ch_off} unique choice texts {} entry {}", say_off.len(), choice.len(), entry.len());
    Ok(())
}

// ---------------------------------------------------------------- i18n-preview

const PALETTE: [[u8; 3]; 8] = [[0, 0, 0], [0xEE, 0xEE, 0xEE], [0xEE, 0xEE, 0], [0xEE, 0x66, 0x88], [0, 0xEE, 0xEE], [0x22, 0x44, 0xEE], [0, 0xEE, 0x44], [0x70, 0x70, 0x70]];

fn paint(img: &mut [u8], w: usize, x0: usize, y0: usize, win: &sncore::actions::TextWindow, scale: usize) {
    for y in 0..sncore::actions::WIN_H { for x in 0..sncore::actions::WIN_W {
        let p = win.pix[y * sncore::actions::WIN_W + x] as usize & 7;
        if p == 0 { continue; }
        if y >= 56 && x >= 248 { continue; } // the corner marker tile
        for dy in 0..scale { for dx in 0..scale {
            let o = (((y0 + y) * scale + dy) * w + (x0 + x) * scale + dx) * 4;
            img[o..o + 3].copy_from_slice(&PALETTE[p]);
        } }
    } }
}

fn frame_pages(font: &sncore::actions::Font, names: &[(u8, Vec<u8>)], text: &[u8], spk: u8) -> Vec<sncore::actions::TextWindow> {
    use sncore::actions::{MessageBox, MsgEvent, TextWindow};
    let mut win = TextWindow::new();
    let mut m = MessageBox::new(spk as u16, 0, 0);
    let mut pages = vec![];
    for _ in 0..4000 {
        if win.busy { win.set_fast(); win.tick(font); continue; }
        let confirm = m.state == 5;
        if confirm { let mut w = TextWindow::new(); w.pix = win.pix.clone(); pages.push(w); }
        if m.tick(&mut win, text, names, confirm) == MsgEvent::Done { break; }
    }
    pages
}

fn preview(lang: &str, dir: &str, out: &str, spn: &str, picks: &[String]) -> Result<(), String> {
    let n = sp_arg(spn)?;
    let fd = load_file(dir, n)?;
    let e = read_entries(&lang_path(lang, &format!("SP{n:02}")))?;
    let widths = font_widths(dir)?;
    let sub = fs::read(format!("{dir}/SUBCODE.BIN")).map_err(|e| e.to_string())?;
    let font = sncore::actions::Font::from_subcode(&sub);
    let mut names = sncore::actions::speaker_names(&sub);
    for (idx, t) in read_names(&lang_path(lang, "names"))? { if let Some(s) = names.get_mut((idx as usize).wrapping_sub(1)) { s.1 = i::markup_to_bytes(&t)?; } }
    let by: HashMap<u16, &Item> = fd.items.iter().map(|x| (x.off, x)).collect();
    // selection: explicit offsets, `longest:N`, or `all`
    let mut sel: Vec<u16> = vec![];
    for p in picks {
        if let Some(k) = p.strip_prefix("longest:") {
            let k: usize = k.parse().map_err(|_| "bad count")?;
            let mut v: Vec<(u32, u16)> = e.iter().filter_map(|(o, en)| {
                let it = by.get(o)?; let b = i::markup_to_bytes(&en.text).ok()?;
                let w = match it.kind { Kind::Say => i::layout_message(&b, it.spk, &widths).lines.iter().map(|l| l.x_end).max().unwrap_or(0), Kind::Choice => i::menu_width(&b, &widths).0 };
                Some((w, *o))
            }).collect();
            v.sort_by(|a, b| b.cmp(a));
            sel.extend(v.iter().take(k).map(|x| x.1));
        } else if let Some(k) = p.strip_prefix("sample:") {
            let k: usize = k.parse().map_err(|_| "bad count")?;
            let all: Vec<u16> = e.keys().copied().collect();
            let step = (all.len() / k.max(1)).max(1);
            sel.extend(all.iter().step_by(step).take(k));
        } else { sel.push(u16::from_str_radix(p, 16).map_err(|_| format!("bad offset {p}"))?); }
    }
    if sel.is_empty() { sel = e.keys().copied().collect(); }
    // layout of the sheet: one row per page / menu item, 256 wide + a label column
    let scale = 2usize;
    struct Row { label: String, win: sncore::actions::TextWindow, h: usize, menu: bool }
    let mut rows: Vec<Row> = vec![];
    for off in sel {
        let Some(en) = e.get(&off) else { continue };
        let Some(it) = by.get(&off) else { continue };
        let mut raw = i::markup_to_bytes(&en.text)?; raw.push(i::END);
        match it.kind {
            Kind::Say => {
                for (pg, w) in frame_pages(&font, &names, &raw, it.spk).into_iter().enumerate() {
                    rows.push(Row { label: format!("SP{n:02}@{off:04x} p{} spk{}", pg + 1, it.spk), win: w, h: 64, menu: false });
                }
            }
            Kind::Choice => {
                let mut win = sncore::actions::TextWindow::new();
                let mut s = vec![0xFC, 0x00, 0xF9, 0x01, 0xFE, 1, 0xFB, 0x10, 0];
                s.extend(raw.iter().copied().take_while(|&c| c != i::END).take(0x20));
                s.push(0xFF);
                win.start(&s);
                while win.busy { win.tick(&font); }
                rows.push(Row { label: format!("SP{n:02}@{off:04x} menu"), win, h: 10, menu: true });
            }
        }
    }
    let lw = 150usize; // label column (px, unscaled)
    let total_h: usize = rows.iter().map(|r| r.h + 6).sum::<usize>() + 4;
    let (w, h) = ((256 + lw) * scale, total_h * scale);
    let mut img = vec![0u8; w * h * 4];
    for px in img.chunks_mut(4) { px.copy_from_slice(&[0x18, 0x18, 0x20, 0xFF]); }
    let mut y = 4usize;
    let mut lab = sncore::actions::TextWindow::new();
    for r in &rows {
        // window background + limit guides
        for yy in 0..r.h { for xx in 0..256 { for dy in 0..scale { for dx in 0..scale {
            let o = (((y + yy) * scale + dy) * w + (lw + xx) * scale + dx) * 4;
            let guide = !r.menu && (xx == 24 || xx == 40 || xx == i::RIGHT_SPEECH as usize || xx == i::RIGHT_NARRATION as usize);
            let guide_m = r.menu && (xx == i::X_MENU as usize || xx == i::RIGHT_MENU as usize);
            img[o..o + 3].copy_from_slice(if guide || guide_m { &[0x50, 0x20, 0x20] } else { &[0, 0, 0] });
        } } } }
        paint(&mut img, w, lw, y, &r.win, scale);
        // label (drawn with the game font in white)
        lab.clear();
        let mut s = vec![0xFC, 0, 0xFE, 1, 0xFB, 0, 0];
        s.extend(r.label.bytes()); s.push(0xFF);
        lab.start(&s);
        while lab.busy { lab.tick(&font); }
        for yy in 0..9 { for xx in 0..lw { if lab.pix[yy * 256 + xx] != 0 { for dy in 0..scale { for dx in 0..scale {
            let o = (((y + yy) * scale + dy) * w + xx * scale + dx) * 4; img[o..o + 3].copy_from_slice(&[0xC0, 0xC0, 0xC0]);
        } } } } }
        y += r.h + 6;
    }
    let f = fs::File::create(out).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba); enc.set_depth(png::BitDepth::Eight);
    enc.write_header().map_err(|e| e.to_string())?.write_image_data(&img).map_err(|e| e.to_string())?;
    println!("preview: {} rows -> {out}", rows.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets() -> Option<String> {
        let d = format!("{}/../assets/raw/files", env!("CARGO_MANIFEST_DIR"));
        Path::new(&format!("{d}/SP06.BIN")).exists().then_some(d)
    }

    fn run(dir: &str, name: &str, patch: &str) -> Report {
        let lang = format!("{}/../target/i18n-test-{name}", env!("CARGO_MANIFEST_DIR"));
        let _ = fs::remove_dir_all(&lang);
        let p = format!("{lang}.tsv");
        fs::write(&p, patch).unwrap();
        import(&lang, dir, &p).unwrap();
        let mut rep = Report::default();
        check_file(dir, &lang, 6, &font_widths(dir).unwrap(), &spk_names(dir), &mut rep).unwrap();
        rep
    }

    #[test]
    fn json_round_trip() {
        let mut e = FileTr::new();
        e.insert(0x12, Entry { hash: 0xDEADBEEF, text: "a \"q\" \\ {nl}ção\tx".into() });
        let p = format!("{}/../target/i18n-test-json.json", env!("CARGO_MANIFEST_DIR"));
        write_entries(&p, "SP06", &e).unwrap();
        assert_eq!(read_entries(&p).unwrap(), e);
        assert!(json_parse("{\"a\": ").is_err());
        assert!(json_parse("{\"a\": \"b\"} x").is_err());
    }

    #[test]
    fn known_good_samples_pass() {
        let Some(d) = assets() else { return };
        let r = run(&d, "good", "SP06@0320\tBem-vinda à sede dos Junkers.{nl}Posso ajudar?\nSP06@019a\tENTRAR\nSP06@0130\tCÂMERA\n");
        assert!(r.fails.is_empty(), "{:?}", r.fails);
        assert_eq!(r.checked, 3);
    }

    #[test]
    fn known_bad_samples_fail_with_exact_messages() {
        let Some(d) = assets() else { return };
        // too wide message line, too wide menu label, bad character, missing key
        let long = "Esta linha e muito mais comprida do que a caixa de mensagem permite desenhar";
        let r = run(&d, "bad1", &format!("SP06@0320\t{long}\nSP06@019a\tENTRAR NO PREDIO DA JUNKER AGORA MESMO SIM\nSP06@0130\tCA\u{00F1}\n"));
        assert!(r.fails.iter().any(|f| f.starts_with("SP06@0320: line 1 is") && f.contains("px wide, limit 208")), "{:?}", r.fails);
        assert!(r.fails.iter().any(|f| f.starts_with("SP06@019a: menu label ends at x=") || f.contains("menu label has")), "{:?}", r.fails);
        assert!(r.fails.iter().any(|f| f.starts_with("SP06@0130:") && f.contains("unsupported character")), "{:?}", r.fails);
        // an unreferenced offset
        let lang = format!("{}/../target/i18n-test-bad2", env!("CARGO_MANIFEST_DIR"));
        let _ = fs::remove_dir_all(&lang); fs::create_dir_all(&lang).unwrap();
        let mut e = FileTr::new();
        e.insert(0x7fff, Entry { hash: 1, text: "x".into() });
        e.insert(0x0130, Entry { hash: 0x1234, text: "CAMERA".into() });
        write_entries(&format!("{lang}/SP06.json"), "SP06", &e).unwrap();
        let mut rep = Report::default();
        check_file(&d, &lang, 6, &font_widths(&d).unwrap(), &[], &mut rep).unwrap();
        assert!(rep.fails.iter().any(|f| f.starts_with("SP06@7fff: key does not exist")));
        assert!(rep.fails.iter().any(|f| f.starts_with("SP06@0130: hash mismatch")));
    }

    #[test]
    fn token_rules() {
        // colour sequence, auto-advance and page breaks must be kept; compare via the token summary
        let a = i::markup_to_bytes("{c3}505-1234{c1} {auto}").unwrap();
        let b = i::markup_to_bytes("{c3}505-1234{c1}").unwrap();
        assert_ne!(tokens(&a), tokens(&b));
        assert_eq!(tokens(&a).0, tokens(&b).0);
        assert_eq!(tokens(&i::markup_to_bytes("{btn:F0}{btn:EC}").unwrap()).1, tokens(&i::markup_to_bytes("{btn:EC}{btn:F0}").unwrap()).1);
    }
}

/// `snt i18n-labels <files>`: unique menu labels with their use counts (work aid for the menu dictionary).
pub fn labels(dir: &str) -> Result<(), String> {
    let mut m: BTreeMap<Vec<u8>, (usize, BTreeSet<u8>, bool)> = BTreeMap::new();
    for n in 0..=38u8 {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        let mut seen = HashSet::new();
        for it in fd.items.iter().filter(|x| x.kind == Kind::Choice) {
            if !seen.insert(it.off) { continue; }
            let e = m.entry(raw[&it.off].to_vec()).or_default();
            e.0 += 1; e.1.insert(n); e.2 |= it.entry;
        }
    }
    let widths = font_widths(dir)?;
    let mut v: Vec<_> = m.into_iter().collect();
    v.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(&b.0)));
    for (raw, (c, files, entry)) in v {
        let (w, _) = i::menu_width(&raw, &widths);
        println!("{c:3} {w:3} {:08x} {}{}  [{}]", i::hash32(&raw), i::bytes_to_markup(&raw), if entry { "  ENTRY" } else { "" }, files.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
    }
    Ok(())
}

/// `snt i18n-apply-menu <lang-dir> <files-dir>`: fill every menu label (not text-entry) that has no entry yet from the dictionary
/// `<lang-dir>/menu.json` ({"labels": {"<hash of the English label>": "<translation>"}}), so common labels (LOOK, TALK, ...) are the same everywhere.
pub fn apply_menu(lang: &str, dir: &str, force: bool) -> Result<(), String> {
    let txt = fs::read_to_string(lang_path(lang, "menu")).map_err(|e| format!("menu.json: {e}"))?;
    let J::Obj(top) = json_parse(&txt)? else { return Err("menu.json: not an object".into()) };
    let mut dict: HashMap<u32, String> = HashMap::new();
    for (k, v) in top { if k == "labels" { if let J::Obj(es) = v { for (h, t) in es { if let J::Str(t) = t { dict.insert(u32::from_str_radix(&h, 16).map_err(|e| e.to_string())?, t); } } } } }
    let mut added = 0;
    for n in 0..=38u8 {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        let path = lang_path(lang, &format!("SP{n:02}"));
        let mut e = read_entries(&path)?;
        let mut changed = false;
        for it in fd.items.iter().filter(|x| x.kind == Kind::Choice && !x.entry) {
            let h = i::hash32(raw[&it.off]);
            if e.contains_key(&it.off) && !(force && dict.contains_key(&h)) { continue; }
            if let Some(t) = dict.get(&h) { e.insert(it.off, Entry { hash: h, text: t.clone() }); added += 1; changed = true; }
        }
        if changed { write_entries(&path, &format!("SP{n:02}"), &e)?; }
    }
    println!("apply-menu: {added} entries added");
    Ok(())
}

/// `snt i18n-learn-menu <lang-dir> <files-dir>`: add to `menu.json` every translated menu label whose English text is used in two or
/// more script files (hash -> translation, first translation wins, conflicts are listed), then run `i18n-apply-menu` to spread it.
pub fn learn_menu(lang: &str, dir: &str) -> Result<(), String> {
    let path = lang_path(lang, "menu");
    let mut dict: BTreeMap<u32, String> = BTreeMap::new();
    if let Ok(txt) = fs::read_to_string(&path) {
        if let J::Obj(top) = json_parse(&txt)? { for (k, v) in top { if k == "labels" { if let J::Obj(es) = v { for (h, t) in es { if let J::Str(t) = t { dict.insert(u32::from_str_radix(&h, 16).map_err(|e| e.to_string())?, t); } } } } } }
    }
    let mut uses: HashMap<u32, BTreeSet<u8>> = HashMap::new();
    let mut tr: HashMap<u32, Vec<(u8, String)>> = HashMap::new();
    for n in 0..=38u8 {
        if !Path::new(&format!("{dir}/SP{n:02}.BIN")).exists() { continue; }
        let fd = load_file(dir, n)?;
        let raw = raw_of(&fd);
        let e = read_entries(&lang_path(lang, &format!("SP{n:02}")))?;
        for it in fd.items.iter().filter(|x| x.kind == Kind::Choice && !x.entry) {
            let h = i::hash32(raw[&it.off]);
            uses.entry(h).or_default().insert(n);
            if let Some(en) = e.get(&it.off) { let v = tr.entry(h).or_default(); if !v.iter().any(|(f, t)| *f == n && *t == en.text) { v.push((n, en.text.clone())); } }
        }
    }
    let (mut added, mut conflicts) = (0, 0);
    for (h, v) in &tr {
        if uses[h].len() < 2 { continue; }
        let first = &v[0].1;
        if v.iter().any(|(_, t)| t != first) { conflicts += 1; println!("conflict {h:08x}: {:?}", v); }
        if !dict.contains_key(h) { dict.insert(*h, first.clone()); added += 1; }
    }
    let mut s = String::from("{\n  \"lang\": \"pt-BR\",\n  \"labels\": {\n");
    let n = dict.len();
    for (k, (h, t)) in dict.iter().enumerate() { s += &format!("    \"{h:08x}\": {}{}\n", jstr(t), if k + 1 < n { "," } else { "" }); }
    s += "  }\n}\n";
    fs::write(&path, s).map_err(|e| e.to_string())?;
    println!("learn-menu: {added} labels added ({n} in the dictionary), {conflicts} conflicts");
    Ok(())
}
