//! Snatcher scenario-script bytecode (SPxx.BIN) – parser, text decoder, metadata.
//!
//! See docs/re/script.md for the full specification and evidence.
//!
//! File layout (verified on all 39 SP files, `snt script-disasm-all`):
//! * `0x0000 .. CODE_END`  bytecode (a forest of tree nodes, see [`parse_node`])
//! * `CODE_END .. 0x3800`  padding (0xFF, sometimes one 0x00 first)
//! * `0x3800 .. EOF`       text area: 0xFF-terminated strings, then a 0x00 pad byte
//!   on odd lengths. Text operands (type 2) are offsets relative to 0x3800.
//!
//! In memory the file is loaded at S:0x1A800 (`script_buf`), so the text area is at S:0x1E000.

/// Offset of the text area inside an SP file (S:0x1E000 - S:0x1A800).
pub const TEXT_BASE: usize = 0x3800;

pub mod vm;

/// Operand type nibbles from the opcode table (S:0x16CDC).
pub mod ty {
    /// no operand
    pub const NONE: u8 = 0;
    /// variable descriptor (16 bit): byte offset<<7 | bit shift<<4 | (width-1)
    pub const VAR: u8 = 1;
    /// text offset relative to [`super::TEXT_BASE`] (16 bit)
    pub const TEXT: u8 = 2;
    /// plain immediate (16 bit)
    pub const IMM: u8 = 3;
    /// nested node (inline)
    pub const NODE: u8 = 4;
    /// code label: offset in the current script file (16 bit)
    pub const LABEL: u8 = 5;
    /// resource id (16 bit): script file id, DATA file/image, sound, sys-call number...
    pub const RES: u8 = 6;
    /// "nothing here" – also terminates lists (list end marker)
    pub const END: u8 = 7;
    /// link flag: a 16-bit pointer follows the opcode byte
    pub const LINK: u8 = 8;
}

/// One opcode table entry as stored in the binary: `[t1][t2<<4|t3][handler-0x15400]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpInfo {
    pub t1: u8,
    pub t2: u8,
    pub t3: u8,
    /// absolute handler address in sub-CPU PRG-RAM
    pub handler: u32,
    pub mnemonic: &'static str,
}

impl OpInfo {
    pub fn has_link(&self) -> bool { (self.t2 | self.t3) & ty::LINK != 0 }
    pub fn types(&self) -> [u8; 3] { [self.t1, self.t2, self.t3] }
}

/// Raw 32-bit values of the opcode table at S:0x16CDC (0x4B entries). Checked against the
/// binary by the test `optable_matches_binary` when `re/images/sub_prg.bin` is present.
pub const RAW_OPTABLE: [u32; 0x4B] = [
    0x04001f30, 0x0410224a, 0x0130224a, 0x0110224a, 0x06001fa4, 0x06001fc0, 0x0440226a, 0x0410226a,
    0x0140226a, 0x0110226a, 0x06001fec, 0x04001d50, 0x05701e0c, 0x06002018, 0x01001cc4, 0x06001f78,
    0x045022c2, 0x043022c2, 0x013022c2, 0x01001caa, 0x01001c90, 0x03001e48, 0x01302352, 0x0130232e,
    0x04481a6e, 0x01481a6e, 0x04401a52, 0x01401a52, 0x01302376, 0x01402376, 0x03301a12, 0x03401a12,
    0x03201a12, 0x06501a12, 0x06301a12, 0x06601a12, 0x06401a12, 0x04401a12, 0x04801a12, 0x04701a12,
    0x0430230a, 0x0130230a, 0x013022e6, 0x05001cfa, 0x04001cfa, 0x00001c2c, 0x040022aa, 0x010022aa,
    0x04001a08, 0x0440228a, 0x0410228a, 0x0110228a, 0x03401c18, 0x03701c18, 0x07401c2e, 0x07701c2e,
    0x02401b94, 0x07001c78, 0x07001dd2, 0x05001e66, 0x03001e66, 0x06001f00, 0x01401b4c, 0x01401af0,
    0x01401a96, 0x06002060, 0x04002060, 0x04001ec0, 0x02001ec0, 0x06001f58, 0x07001fdc, 0x07002008,
    0x07002034, 0x07001f94, 0x03002044,
];

/// Mnemonics (one per opcode). Opcodes sharing a handler differ only in operand types.
/// Semantics: docs/re/script.md §Opcodes.
pub const MNEMONICS: [&str; 0x4B] = [
    "ANIM",     // 00 [tuple obj mode] -> queue 09 (mode, obj)
    "ADD",      // 01
    "ADD",      // 02
    "ADD",      // 03
    "CMD0D",    // 04 res -> queue 0D
    "CMD0C",    // 05 res -> queue 0C
    "AND",      // 06
    "AND",      // 07
    "AND",      // 08
    "AND",      // 09
    "MUSIC",    // 0A res -> queue 07
    "CALL",     // 0B [tuple file label]
    "GOSUB",    // 0C label
    "CMD0E",    // 0D res -> queue 0E
    "INC",      // 0E var (saturating)
    "SOUND",    // 0F res -> queue 1C
    "EQ",       // 10
    "EQ",       // 11
    "EQ",       // 12
    "CLR",      // 13 var = 0
    "SETALL",   // 14 var = all ones
    "HISTORY",  // 15 imm -> value (previous location n steps back)
    "GT",       // 16 a > b
    "GE",       // 17 a >= b
    "IFELSE",   // 18 [link=else] cond then
    "IFELSE",   // 19
    "WHEN",     // 1A cond body
    "WHEN",     // 1B
    "LET",      // 1C var = imm
    "LET",      // 1D var = expr
    "LIST",     // 1E..27 cons cells / tuples
    "LIST", "LIST", "LIST", "LIST", "LIST", "LIST", "LIST",
    "LIST*",    // 26 [link=rest] car
    "LIST.",    // 27 car (last element)
    "LT",       // 28 a < b
    "LT",       // 29
    "LE",       // 2A a <= b
    "GOTO",     // 2B label
    "JUMP",     // 2C [tuple file label]
    "NOP",      // 2D
    "NOT",      // 2E
    "NOT",      // 2F
    "DO",       // 30 node
    "OR",       // 31
    "OR",       // 32
    "OR",       // 33
    "CASE",     // 34 imm body
    "CASE",     // 35 imm (no body)
    "MARK",     // 36 [body]  push menu return depth
    "MARK",     // 37
    "CHOICE",   // 38 text body
    "UNMARK",   // 39
    "RETURN",   // 3A
    "INGOSUB",  // 3B label -> bool
    "GOSUBPC",  // 3C imm -> label
    "SCENE",    // 3D res (hi = DATA file, lo = image)
    "RANDOM",   // 3E var list
    "CYCLE",    // 3F var list
    "SWITCH",   // 40 var cases
    "SYS",      // 41 fn
    "SYS",      // 42 [tuple fn args...]
    "SAY",      // 43 [tuple speaker text]
    "SAY",      // 44 text
    "ANIMWAIT", // 45 res -> queue 09 (3, obj)
    "CMD0A",    // 46 -> queue 0A
    "CMD13",    // 47 -> queue 13
    "CMD0B",    // 48 -> queue 0B
    "CMD19",    // 49 -> queue 19
    "WAIT",     // 4A imm frames -> queue 10
];

pub const OP_COUNT: usize = 0x4B;

pub fn op_info(op: u8) -> Option<OpInfo> {
    let i = op as usize;
    if i >= OP_COUNT { return None }
    let r = RAW_OPTABLE[i];
    Some(OpInfo {
        t1: (r >> 24) as u8,
        t2: ((r >> 20) & 15) as u8,
        t3: ((r >> 16) & 15) as u8,
        handler: 0x15400 + (r & 0xFFFF),
        mnemonic: MNEMONICS[i],
    })
}

/// Handler offset (low word of the table entry) of the list/cons handler (S:0x16E12).
pub const HANDLER_LIST: u32 = 0x15400 + 0x1A12;
/// Handler of CASE (S:0x17018) – used by list counting and SWITCH.
pub const HANDLER_CASE: u32 = 0x15400 + 0x1C18;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Var(u16),
    Text(u16),
    Imm(u16),
    Label(u16),
    Res(u16),
    Node(Box<Node>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// file offset of the opcode byte
    pub offset: usize,
    pub op: u8,
    /// link pointer (present when (t2|t3)&8)
    pub link: Option<u16>,
    pub operands: Vec<Operand>,
    /// file offset just after this node (inline children included, link targets excluded)
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    BadOpcode { offset: usize, op: u8 },
    Truncated { offset: usize },
    TooDeep { offset: usize },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::BadOpcode { offset, op } => write!(f, "bad opcode {op:02x} at {offset:04x}"),
            ParseError::Truncated { offset } => write!(f, "truncated node at {offset:04x}"),
            ParseError::TooDeep { offset } => write!(f, "nesting too deep at {offset:04x}"),
        }
    }
}

fn rd16(b: &[u8], o: usize) -> Result<u16, ParseError> {
    if o + 2 > b.len() { return Err(ParseError::Truncated { offset: o }) }
    Ok(u16::from_be_bytes([b[o], b[o + 1]]))
}

/// Parse one node at `offset`. Layout: opcode byte, then the link pointer if the
/// opcode has the LINK flag, then one operand per type in (t1,t2,t3): NONE/END/LINK take
/// no bytes, NODE is an inline nested node, every other type is a 16-bit big-endian word.
/// This mirrors the VM's own skip routine S:0x1699A (types 0/7 skipped, 4 recursed,
/// others +2) and the link test S:0x16A64.
pub fn parse_node(b: &[u8], offset: usize) -> Result<Node, ParseError> { parse_rec(b, offset, 0) }

fn parse_rec(b: &[u8], offset: usize, depth: usize) -> Result<Node, ParseError> {
    if depth > 200 { return Err(ParseError::TooDeep { offset }) }
    let op = *b.get(offset).ok_or(ParseError::Truncated { offset })?;
    let info = op_info(op).ok_or(ParseError::BadOpcode { offset, op })?;
    let mut s = offset + 1;
    let link = if info.has_link() { let p = rd16(b, s)?; s += 2; Some(p) } else { None };
    let mut operands = Vec::new();
    for t in info.types() {
        match t {
            ty::NONE | ty::END | ty::LINK => {}
            ty::NODE => {
                let n = parse_rec(b, s, depth + 1)?;
                s = n.end;
                operands.push(Operand::Node(Box::new(n)));
            }
            _ => {
                let v = rd16(b, s)?;
                s += 2;
                operands.push(match t {
                    ty::VAR => Operand::Var(v),
                    ty::TEXT => Operand::Text(v),
                    ty::LABEL => Operand::Label(v),
                    ty::RES => Operand::Res(v),
                    _ => Operand::Imm(v),
                });
            }
        }
    }
    Ok(Node { offset, op, link, operands, end: s })
}

/// A whole SP file split into its regions.
#[derive(Debug, Clone)]
pub struct Script {
    /// top-level nodes in file order (linear sweep from 0)
    pub nodes: Vec<Node>,
    /// end of bytecode (first byte of padding)
    pub code_end: usize,
    /// text strings: (offset relative to TEXT_BASE, raw bytes without the 0xFF terminator)
    pub strings: Vec<(u16, Vec<u8>)>,
    /// offset just after the last string terminator (file-relative)
    pub text_end: usize,
}

/// Linear sweep over the bytecode area. Stops at the 0xFF padding (0xFF is not an
/// opcode). Returns an error if anything other than padding remains before 0x3800.
pub fn parse_script(file: &[u8]) -> Result<Script, String> {
    let mut nodes = Vec::new();
    let mut o = 0usize;
    let limit = TEXT_BASE.min(file.len());
    while o < limit && file[o] != 0xFF {
        // a single 0x00 directly followed by 0xFF padding is a pad byte, not opcode 00
        if file[o] == 0x00 && file.get(o + 1) == Some(&0xFF) { break }
        let n = parse_node(file, o).map_err(|e| e.to_string())?;
        o = n.end;
        nodes.push(n);
    }
    let code_end = o;
    for (i, &c) in file[code_end..limit].iter().enumerate() {
        if c != 0xFF && !(i == 0 && c == 0x00) {
            return Err(format!("non-padding byte {c:02x} at {:04x}", code_end + i));
        }
    }
    let mut strings = Vec::new();
    let mut p = TEXT_BASE;
    let mut text_end = TEXT_BASE.min(file.len());
    while p < file.len() {
        match file[p..].iter().position(|&c| c == 0xFF) {
            Some(n) => {
                strings.push(((p - TEXT_BASE) as u16, file[p..p + n].to_vec()));
                p += n + 1;
                text_end = p;
            }
            None => break,
        }
    }
    for (i, &c) in file[text_end..].iter().enumerate() {
        if c != 0 { return Err(format!("unexpected byte {c:02x} after text at {:04x}", text_end + i)) }
    }
    Ok(Script { nodes, code_end, strings, text_end })
}

/// Visit every node (including nested ones) depth-first.
pub fn walk<'a>(n: &'a Node, f: &mut dyn FnMut(&'a Node)) {
    f(n);
    for o in &n.operands {
        if let Operand::Node(c) = o { walk(c, f) }
    }
}

/// Variable descriptor → (byte offset into the bit array at S:$9C60, bit shift from the MSB
/// of that byte, width in bits). See S:0x16B14 (read) / S:0x16B80 (write).
pub fn var_fields(desc: u16) -> (u16, u8, u8) {
    (desc >> 7, ((desc >> 4) & 7) as u8, (desc & 15) as u8 + 1)
}

/// Read a variable from the 0x160-byte variable area (S:$9C60..$9DBF). Bytes beyond the
/// slice read as 0. Exactly mirrors S:0x16B14: 3 bytes → long (b0 b1 b2 00), rol by
/// shift+width, mask width bits.
pub fn var_read(vars: &[u8], desc: u16) -> u16 {
    let (byte, shift, width) = var_fields(desc);
    let g = |i: usize| vars.get(byte as usize + i).copied().unwrap_or(0) as u32;
    let l = (g(0) << 24) | (g(1) << 16) | (g(2) << 8);
    let r = l.rotate_left(shift as u32 + width as u32);
    (r & ((1u32 << width) - 1)) as u16
}

/// Write a variable (S:0x16B80). Value is masked to the width.
pub fn var_write(vars: &mut [u8], desc: u16, value: u16) {
    let (byte, shift, width) = var_fields(desc);
    let mask = (1u32 << width) - 1;
    let rot = shift as u32 + width as u32;
    let m = mask.rotate_right(rot);
    let v = (value as u32 & mask).rotate_right(rot);
    for i in 0..3 {
        let idx = byte as usize + i;
        if idx >= vars.len() { break }
        let sh = 24 - 8 * i as u32;
        let mb = ((m >> sh) & 0xFF) as u8;
        let vb = ((v >> sh) & 0xFF) as u8;
        vars[idx] = (vars[idx] & !mb) | vb;
    }
}

pub fn var_name(desc: u16) -> String {
    let (b, s, w) = var_fields(desc);
    if w == 1 { format!("v[{b:03x}.{s}]") } else { format!("v[{b:03x}.{s}:{w}]") }
}

/// Text control bytes (stage 1 copier S:0x13AC0 and the typewriter S:0x13BC8).
pub mod text {
    /// end of string
    pub const END: u8 = 0xFF;
    /// line break (S:0x13AF2 sets $974A=1: end of line)
    pub const NEWLINE: u8 = 0xF2;
    /// S:0x13B58 sets $97CC=2 and continues (HYPOTHESIS: switch text window mode)
    pub const F4: u8 = 0xF4;
    /// S:0x13B4E sets $974A=3 (HYPOTHESIS: end of page – wait for button and clear)
    pub const PAGE: u8 = 0xF6;
    /// 0xF8..=0xFE: colour/attribute change, attribute = byte & 7 (S:0x13B18 emits FE,n)
    pub const COLOR_FIRST: u8 = 0xF8;
    /// 0xEC/0xEE/0xF0: insert a character chosen by $78BA (tables S:0x13BA0/BAC/BB8)
    pub const SUBST: [u8; 3] = [0xEC, 0xEE, 0xF0];
}

/// Decode a raw string to a readable form: printable ASCII as is, `\n` for 0xF2,
/// `{page}` for 0xF6, `{cN}` for colour N (0xF8..0xFE), `{xx}` for other bytes.
pub fn decode_text(raw: &[u8]) -> String {
    let mut s = String::new();
    for &c in raw {
        match c {
            0x20..=0x7E => s.push(c as char),
            text::NEWLINE => s.push('\n'),
            text::PAGE => s.push_str("{page}"),
            0xF8..=0xFE => s.push_str(&format!("{{c{}}}", c & 7)),
            _ => s.push_str(&format!("{{{c:02X}}}")),
        }
    }
    s
}

/// String starting at a text offset (relative to TEXT_BASE). Returns raw bytes.
pub fn text_at(file: &[u8], off: u16) -> Option<&[u8]> {
    let p = TEXT_BASE + off as usize;
    let rest = file.get(p..)?;
    let n = rest.iter().position(|&c| c == 0xFF)?;
    Some(&rest[..n])
}

/// Script file ids are global file numbers: SP00 = 57 (0x39) … SP38 = 95 (0x5F).
pub fn sp_file_id(n: u8) -> u16 { 57 + n as u16 }
pub fn sp_number(file_id: u16) -> Option<u8> {
    if (57..=95).contains(&file_id) { Some((file_id - 57) as u8) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optable_matches_binary() {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../re/images/sub_prg.bin");
        let Ok(img) = std::fs::read(p) else { eprintln!("skip: {p} missing"); return };
        let t = 0x16CDC - 0x6000;
        for i in 0..OP_COUNT {
            let v = u32::from_be_bytes([img[t + 4 * i], img[t + 4 * i + 1], img[t + 4 * i + 2], img[t + 4 * i + 3]]);
            assert_eq!(v, RAW_OPTABLE[i], "op {i:02x}");
        }
        // entry 0x4B is code (`lea 1(a3),a3` = 47EB 0001), so the table has exactly 0x4B entries
        assert_eq!(&img[t + 4 * 0x4B..t + 4 * 0x4B + 4], &[0x47, 0xEB, 0x00, 0x01]);
    }

    #[test]
    fn var_roundtrip() {
        let mut v = vec![0u8; 0x160];
        for desc in [0x000Fu16, 0x0123, 0x0277, 0x10A3, 0x2F5F] {
            let (_, _, w) = var_fields(desc);
            let max = ((1u32 << w) - 1) as u16;
            var_write(&mut v, desc, max);
            assert_eq!(var_read(&v, desc), max);
            var_write(&mut v, desc, 0);
            assert_eq!(var_read(&v, desc), 0);
        }
        // bit 0 of byte 0 (shift 7, width 1) is the LSB of byte 0
        var_write(&mut v, 0x0070, 1);
        assert_eq!(v[0], 0x01);
        // shift 0 width 1 is the MSB
        var_write(&mut v, 0x0080, 1);
        assert_eq!(v[1], 0x80);
    }

    #[test]
    fn parse_all_sp_files() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/raw/files");
        let mut seen = 0;
        for n in 0..=38u8 {
            let p = format!("{dir}/SP{n:02}.BIN");
            let Ok(f) = std::fs::read(&p) else { continue };
            let s = parse_script(&f).unwrap_or_else(|e| panic!("{p}: {e}"));
            assert!(s.code_end > 0 && s.code_end <= TEXT_BASE);
            seen += 1;
        }
        if seen == 0 { eprintln!("skip: no SP files") }
    }
}

/// Global file index used by the game (S:0x62FA name table; equals ISO order without .TXT).
pub const FILE_NAMES: [&str; 97] = [
    "DATA_A0", "DATA_B0", "DATA_D0", "DATA_D1", "DATA_D2", "DATA_F0", "DATA_F4", "DATA_G0",
    "DATA_H00", "DATA_H01", "DATA_H02", "DATA_H03", "DATA_H04", "DATA_H05", "DATA_H06", "DATA_H07",
    "DATA_H08", "DATA_H09", "DATA_H11", "DATA_H12", "DATA_H13", "DATA_H14", "DATA_H15", "DATA_I0",
    "DATA_J0", "DATA_K1", "DATA_M0", "DATA_O0", "DATA_P0", "DATA_Q3", "DATA_Q5", "DATA_Q8",
    "DATA_S0", "DATA_S1", "DATA_S2", "DATA_T0", "DATA_U0", "DATA_Y00", "DATA_Y01", "DATA_Y03",
    "DATA_Y04", "DATA_Y05", "DATA_Y06", "DATA_Y07", "DATA_Y08", "DATA_Y09", "DATA_Y10", "DATA_Y11",
    "DATA_Y12", "DATA_Y13", "DATA_Y14", "DATA_Y15", "DATA_Y16", "FMWR_1", "PCMDRMDT", "PCMLD_01",
    "PCMLT_01", "SP00", "SP01", "SP02", "SP03", "SP04", "SP05", "SP06", "SP07", "SP08", "SP09",
    "SP10", "SP11", "SP12", "SP13", "SP14", "SP15", "SP16", "SP17", "SP18", "SP19", "SP20", "SP21",
    "SP22", "SP23", "SP24", "SP25", "SP26", "SP27", "SP28", "SP29", "SP30", "SP31", "SP32", "SP33",
    "SP34", "SP35", "SP36", "SP37", "SP38", "SUBCODE",
];

/// Names of the SYS (0x41/0x42) sub-functions, table S:0x17492 (14 entries).
pub const SYS_NAMES: [&str; 14] = [
    "Q04", "Q1E", "Q21", "Q1F", "MINIGAME", "PEEK", "POKE", "Q1D", "Q15_OR_ANIM", "SETSTATE",
    "SET79FE", "SAVEMODE", "RESETFLAGS", "Q1B",
];

/// Offset of the speaker-name table (S:0x13552) inside SUBCODE.BIN (loaded at S:0xD400).
pub const SPEAKER_TABLE_IN_SUBCODE: usize = 0x13552 - 0xD400;

/// Speaker names for SAY: index = spk-1 (entry 0 is empty = no name box; S:0x13686 treats
/// `$9744`=0 specially). Each name: attribute byte (bit7 clear → colour = byte & 7, here '2'),
/// then ASCII, 0xFF-terminated. Table entries are signed word offsets relative to the table.
/// Returns (colour, name) pairs, read from the caller-supplied SUBCODE.BIN.
pub fn speaker_names(subcode: &[u8]) -> Vec<(u8, String)> {
    let t = SPEAKER_TABLE_IN_SUBCODE;
    let mut out = Vec::new();
    let mut prev = i32::MIN;
    for i in 0..256 {
        let Some(b) = subcode.get(t + 2 * i..t + 2 * i + 2) else { break };
        let o = i16::from_be_bytes([b[0], b[1]]) as i32;
        if o >= 0 || o < prev { break }
        prev = o;
        let a = (t as i32 + o) as usize;
        let Some(end) = subcode[a..].iter().position(|&c| c == 0xFF) else { break };
        let raw = &subcode[a..a + end];
        let (col, name) = match raw.first() {
            Some(&c) if c & 0x80 == 0 => (c & 7, &raw[1..]),
            _ => (0, raw),
        };
        out.push((col, String::from_utf8_lossy(name).to_string()));
    }
    out
}
