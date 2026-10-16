//! Run-time layout and word wrap of voice subtitles (docs/voice-subtitles.md "Layouts"), and the pt-BR break heuristics shared with the
//! tools (`snt i18n-breaks`, `snt i18n-reflow`, `snt voice-check`): where a page may end without cutting a sentence in the middle.
use crate::i18n as i;

/// Severity of the break in front of each word (index 0 = 0): see `break_severity`, judged on a window of 4 words each side.
pub fn word_severities(words: &[Vec<u8>]) -> Vec<u8> {
    let txt: Vec<String> = words.iter().map(|w| plain_page(w)).collect();
    (0..words.len()).map(|k| {
        if k == 0 { return 0; }
        let before = txt[k.saturating_sub(4)..k].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(" ");
        let after = txt[k..(k + 4).min(words.len())].iter().filter(|t| !t.is_empty()).cloned().collect::<Vec<_>>().join(" ");
        break_severity(&before, &after).0
    }).collect()
}

/// Plain text of one page for the break analysis: line breaks become spaces, colour / auto tokens vanish, a button placeholder is one word.
pub fn plain_page(page: &[u8]) -> String {
    let mut s = String::new();
    for &c in page {
        match c {
            i::NL => s.push(' '),
            0xF8..=0xFE | i::AUTO => {}
            0xEC | 0xEE | 0xF0 => s.push('#'),
            _ => s.push_str(&i::bytes_to_markup(&[c])),
        }
    }
    s.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

const FUNCTION_WORDS: &[&str] = &[
    "a", "o", "as", "os", "um", "uma", "uns", "umas", "de", "do", "da", "dos", "das", "em", "no", "na", "nos", "nas", "num", "numa", "por", "pelo", "pela", "pelos",
    "pelas", "para", "pra", "pro", "com", "sem", "sob", "sobre", "ao", "aos", "à", "às", "e", "ou", "mas", "que", "se", "nem", "como", "quando", "porque", "onde", "até",
    "entre", "contra", "seu", "sua", "seus", "suas", "meu", "minha", "meus", "minhas", "teu", "tua", "este", "esse", "esta", "essa", "esses", "essas", "estes", "estas",
    "nosso", "nossa", "nossos", "nossas", "eu", "você", "ele", "ela", "nós", "vocês", "eles", "elas", "me", "te", "lhe", "lhes", "não", "já", "mais", "muito", "tão",
    "todo", "toda", "todos", "todas", "cada", "qual", "quais", "quem", "cujo", "cuja", "d'água", "ser", "é", "foi", "são", "está", "estão", "vai", "vou", "vamos", "tem",
    "ter", "tinha", "há", "faz", "fazer", "pode", "posso", "deve", "preciso", "quer", "quero", "estou", "estava", "era",
];
const ABBREVIATIONS: &[&str] = &["sr.", "sra.", "srta.", "dr.", "dra.", "prof.", "st.", "mr.", "mrs.", "ms.", "n.", "nº.", "vs.", "ex.", "av.", "cap."];

/// Strip trailing closing punctuation / spaces so that `x." ` and `x.` look the same.
fn core_end(s: &str) -> &str { s.trim_end_matches(|c: char| matches!(c, '"' | '\'' | ')' | ' ' | '»')) }

const CONJUNCTIONS: &[&str] = &["e", "mas", "ou", "porque", "pois", "que", "quando", "onde", "como", "enquanto", "embora", "então", "porém", "senão", "caso", "quem", "quanto", "portanto"];

/// How badly a page break reads, from the text just before it and just after it (`plain_page` form; a window of a few words each is enough).
/// 5 = after an abbreviation (`Sr.` / page / `Seed`), 4 = after a function word (article, preposition, conjunction, pronoun, auxiliary),
/// 3 = no punctuation at all, a dangling dash, a colon (a label cut off from its text), 2 = in front of a conjunction or after an ellipsis followed by a lower-case continuation
/// (a clause boundary: tolerated), 1 = after a comma / semicolon (a natural pause: tolerated), 0 = the text before ends a sentence or clause
/// (`. ? ! …`), the next text starts a new field (`Label:`) or a new list entry (`Name - Role`).
pub fn break_severity(before: &str, after: &str) -> (u8, &'static str) {
    let b = core_end(before);
    let last = b.rsplit(' ').next().unwrap_or("");
    let lastc = b.chars().last().unwrap_or(' ');
    let low = last.to_lowercase();
    let first_after = after.split(' ').next().unwrap_or("");
    let next_lower = first_after.chars().next().map_or(false, |c| c.is_lowercase());
    if ABBREVIATIONS.contains(&low.as_str()) { return (5, "abbreviation"); }
    let aw: Vec<&str> = after.split(' ').take(4).collect();
    let upper = |w: &str| w.chars().find(|c| c.is_alphabetic()).map_or(false, |c| c.is_uppercase());
    if aw.first().map_or(false, |w| upper(w)) {
        if aw.iter().take(3).any(|w| w.len() > 1 && w.ends_with(':')) { return (0, "next field"); }
        if let Some(d) = aw.iter().take(4).position(|w| *w == "-") { if d >= 1 && aw[..d].iter().all(|w| upper(w)) { return (0, "next list entry"); } }
    }
    match lastc {
        ':' => (3, "colon: the label is cut off from its text"),
        '.' | '?' | '!' | '…' => if lastc == '.' && b.ends_with("...") && next_lower { (2, "ellipsis + lower-case continuation") } else { (0, "") },
        ',' | ';' => {
            // a short opener ("Depois,", "Hoje,", "Ao mesmo tempo,") is not a clause of its own: its sentence goes on after the break
            let since = b.rsplit(|c: char| matches!(c, '.' | '?' | '!' | ':' | '…')).next().unwrap_or(b);
            if lastc == ',' && since.trim().split(' ').count() <= 3 && b.len() > since.len() { (2, "short opener before the comma") } else { (1, "comma") }
        }
        '-' if b.ends_with(" -") => (3, "dangling dash"),
        '-' => (1, "dash"),
        _ => {
            if FUNCTION_WORDS.contains(&low.trim_matches(|c: char| !c.is_alphabetic() && c != '\'').to_string().as_str()) { (4, "function word before the break") }
            else if CONJUNCTIONS.contains(&first_after.to_lowercase().trim_matches(|c: char| !c.is_alphabetic()).to_string().as_str()) { (2, "before a conjunction") }
            else { (3, "no punctuation") }
        }
    }
}


// ---------------------------------------------------------------- layouts

/// Where a subtitle goes relative to the dialogue strip (docs/voice-subtitles.md section 4, "Layouts").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VKind {
    /// no portrait in the strip: the message box, full width (name on its own line)
    Box,
    /// the free side of the strip next to the portrait(s): a column of at least `LANE_MIN` px
    Lane,
    /// two or more portraits leave no usable column: the text goes full width on a black band just above the portraits (name inline)
    Band,
    /// the picture reaches the bottom of the screen (no strip): a compact band at the bottom edge (name inline)
    Compact,
}

/// Horizontal geometry and page size of one layout. Rows are the engine's business (`engine/src/vsub.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VLayout {
    pub kind: VKind,
    /// X of the name line (`Box`, `Lane`)
    pub x_name: u32,
    /// X of the text lines
    pub x_text: u32,
    /// a line ends at or before this X
    pub right: u32,
    /// the name goes in front of the first line of every page (`Band`, `Compact`), not on a line of its own
    pub name_inline: bool,
    /// preferred and largest number of text lines of a page
    pub pref: usize,
    pub max: usize,
}

impl VLayout {
    /// Pixels a text line may use.
    pub fn width(&self) -> u32 { self.right - self.x_text }
}

/// Narrowest text column (px) that is used as a lane: about 18 characters. Below it the band is used (a 80 px gap between two portraits is 11 characters).
pub const LANE_MIN: u32 = 120;
/// Gap kept between a portrait frame and the text of a lane.
pub const LANE_GAP: u32 = 8;
/// Screen columns a lane may use (the message box's name starts at X 16, its text ends at X 232).
pub const LEFT_EDGE: u32 = 16;
pub const RIGHT_EDGE: u32 = 240;

/// The layout for the occupied columns of the strip: `runs` = inclusive column ranges (x0, x1) of everything that is not black in the strip rows (the
/// portrait frames), sorted and not overlapping; `strip` = false when the picture reaches the bottom of the screen; `named` = the subtitle shows a speaker name.
pub fn layout_for(runs: &[(u32, u32)], strip: bool, named: bool) -> VLayout {
    let cap = i::lines_per_page(if named { 3 } else { 0 }, true);
    if !strip { return VLayout { kind: VKind::Compact, x_name: 24, x_text: 24, right: 232, name_inline: true, pref: 2, max: 3 }; }
    if runs.is_empty() { return VLayout { kind: VKind::Box, x_name: 16, x_text: 24, right: 232, name_inline: false, pref: 3, max: 4.min(cap) }; }
    // free columns: before the first, between, after the last portrait
    let mut lanes: Vec<(u32, u32)> = vec![];
    let mut lo = LEFT_EDGE;
    for &(x0, x1) in runs {
        if x0 > lo + LANE_GAP { lanes.push((lo, x0 - LANE_GAP)); }
        lo = lo.max(x1 + 1 + LANE_GAP);
    }
    lanes.push((lo, RIGHT_EDGE));
    let mut best: Option<VLayout> = None;
    for (lo, hi) in lanes {
        if hi <= lo + 8 { continue; }
        let l = VLayout { kind: VKind::Lane, x_name: lo, x_text: lo + 8, right: if hi >= RIGHT_EDGE { 232 } else { hi }, name_inline: false, pref: 4, max: 5.min(cap) };
        if l.right < l.x_text + LANE_MIN { continue; }
        if best.map_or(true, |b| l.width() > b.width()) { best = Some(l); }
    }
    best.unwrap_or(VLayout { kind: VKind::Band, x_name: 24, x_text: 24, right: 232, name_inline: true, pref: 2, max: 3 })
}

// ---------------------------------------------------------------- run-time word wrap

/// Width in pixels of a run of glyph bytes.
pub fn text_width(widths: &[u8], s: &[u8]) -> u32 { s.iter().map(|&c| i::advance(widths, c)).sum() }

/// Greedy fill: the first line may use `first_w` px, the others `w`. A word wider than the line is not split (it overflows: `voice-check` reports it).
fn greedy(words: &[&[u8]], widths: &[u8], first_w: u32, w: u32) -> Vec<Vec<u8>> {
    let space = i::advance(widths, b' ');
    let mut lines: Vec<Vec<u8>> = vec![];
    let mut cur: Vec<u8> = vec![];
    let mut cw = 0u32;
    for word in words {
        let ww = text_width(widths, word);
        if !cur.is_empty() {
            let lim = if lines.is_empty() { first_w } else { w };
            if cw + space + ww > lim { lines.push(std::mem::take(&mut cur)); cw = 0; } else { cur.push(b' '); cw += space; }
        }
        cur.extend_from_slice(word);
        cw += ww;
    }
    lines.push(cur);
    lines
}

/// Break a subtitle text (markup bytes: colours, `NL`, `PAGE`) into pages of lines for a layout. `prefix_px` is the width of the inline speaker name that
/// the first line of every page carries. Hints of the author: `PAGE` is a page break, `NL` a line break (such a segment is cut between lines);
/// everything else is wrapped by the real glyph widths and paged at the places where a sentence or clause ends (`break_severity`), preferring `pref`-line
/// pages and never more than `max`.
pub fn paginate(text: &[u8], widths: &[u8], lay: &VLayout, prefix_px: u32) -> Vec<Vec<Vec<u8>>> {
    let w = lay.width();
    let fw = w.saturating_sub(prefix_px);
    let mut pages: Vec<Vec<Vec<u8>>> = vec![];
    for seg in text.split(|&c| c == i::PAGE) {
        if seg.contains(&i::NL) {
            let mut lines: Vec<Vec<u8>> = vec![];
            for part in seg.split(|&c| c == i::NL) {
                let ws: Vec<&[u8]> = part.split(|&c| c == b' ').filter(|x| !x.is_empty()).collect();
                if !ws.is_empty() { lines.extend(greedy(&ws, widths, fw, fw)); }
            }
            for ch in lines.chunks(lay.pref.max(1)) { pages.push(ch.to_vec()); }
            continue;
        }
        let words: Vec<Vec<u8>> = seg.split(|&c| c == b' ').filter(|x| !x.is_empty()).map(|x| x.to_vec()).collect();
        let n = words.len();
        if n == 0 { continue; }
        let sev = word_severities(&words);
        let refs: Vec<&[u8]> = words.iter().map(|x| x.as_slice()).collect();
        let lines_of = |a: usize, b: usize| greedy(&refs[a..b], widths, fw, w);
        // cheapest paging: 30 per page, a short page a little, a break in the middle of a clause a lot
        let mut best: Vec<(f64, usize)> = vec![(f64::INFINITY, 0); n + 1];
        best[0] = (0.0, 0);
        for j in 1..=n {
            for a in (0..j).rev() {
                let nl = lines_of(a, j).len();
                if nl > lay.max && a + 1 < j { break; }
                if best[a].0.is_infinite() { continue; }
                let mut c = best[a].0 + 30.0 + if nl > lay.pref { 12.0 } else { (lay.pref - nl) as f64 };
                if nl > lay.max { c += 1000.0; }
                if a > 0 { c += 20.0 * sev[a] as f64; }
                if c < best[j].0 { best[j] = (c, a); }
            }
        }
        let mut cuts = vec![];
        let mut j = n;
        while j > 0 { cuts.push(j); j = best[j].1; }
        cuts.reverse();
        let mut a = 0;
        for b in cuts { pages.push(lines_of(a, b)); a = b; }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths() -> Vec<u8> { vec![7; 256] }

    fn text(s: &str) -> Vec<u8> { i::markup_to_bytes(s).unwrap() }

    #[test]
    fn layouts_by_portrait_columns() {
        // no portrait: the message box
        let b = layout_for(&[], true, true);
        assert_eq!((b.kind, b.x_name, b.x_text, b.right, b.name_inline), (VKind::Box, 16, 24, 232, false));
        // Gillian at the left (frame columns 24..87): the lane starts right of it
        let l = layout_for(&[(24, 87)], true, true);
        assert_eq!((l.kind, l.x_name, l.x_text, l.right), (VKind::Lane, 96, 104, 232));
        assert!(l.width() >= LANE_MIN);
        // the Chief alone at the right (168..231): the lane is left of it
        let r = layout_for(&[(168, 231)], true, true);
        assert_eq!((r.kind, r.x_name, r.x_text, r.right), (VKind::Lane, 16, 24, 160));
        // two portraits (left and right): the 80 px gap is too narrow, the band above them
        let lr = layout_for(&[(24, 87), (168, 231)], true, true);
        assert_eq!((lr.kind, lr.x_text, lr.right, lr.name_inline), (VKind::Band, 24, 232, true));
        // three portraits, or a middle one alone: band too
        assert_eq!(layout_for(&[(24, 87), (96, 159), (168, 231)], true, false).kind, VKind::Band);
        assert_eq!(layout_for(&[(96, 159)], true, false).kind, VKind::Band);
        // no strip: compact band
        assert_eq!(layout_for(&[], false, true).kind, VKind::Compact);
        // a lane never overlaps a portrait
        for runs in [vec![(24u32, 87u32)], vec![(168, 231)], vec![(24, 87), (168, 231)], vec![(32, 95), (168, 231)], vec![(96, 159)]] {
            let l = layout_for(&runs, true, true);
            if l.kind == VKind::Lane { for &(x0, x1) in &runs { assert!(l.right + LANE_GAP <= x0 || l.x_name > x1, "{runs:?} -> {l:?}"); } }
        }
    }

    #[test]
    fn paginate_keeps_every_word_and_the_geometry() {
        let t = text("Esta é a sua blaster, a arma oficial de um Junker. Tem circuito de feedback total, que se ajusta ao seu tempo de reação. Ou seja, é tão boa quanto você. Pode apostar, não é uma daquelas pistolas de raios do Exército.");
        for runs in [vec![], vec![(24u32, 87u32)], vec![(24, 87), (168, 231)]] {
            let lay = layout_for(&runs, true, true);
            let prefix = if lay.name_inline { 56 } else { 0 };
            let pages = paginate(&t, &widths(), &lay, prefix);
            let mut words: Vec<String> = vec![];
            for p in &pages {
                assert!(p.len() <= lay.max, "{:?}: a page of {} lines", lay.kind, p.len());
                for (k, l) in p.iter().enumerate() {
                    assert!(text_width(&widths(), l) + if k == 0 { prefix } else { 0 } <= lay.width(), "{:?}: line too wide", lay.kind);
                    words.extend(l.split(|&c| c == b' ').filter(|x| !x.is_empty()).map(|x| i::bytes_to_markup(x)));
                }
            }
            assert_eq!(words.join(" "), i::bytes_to_markup(&t).split(' ').collect::<Vec<_>>().join(" "));
        }
        // the narrower the layout, the more pages
        let n = |runs: &[(u32, u32)]| { let l = layout_for(runs, true, false); paginate(&t, &widths(), &l, 0).len() };
        assert!(n(&[(24, 87)]) >= n(&[]) && n(&[(24, 87), (168, 231)]) >= 1);
        // author hints: {page} is a break, {nl} a line break
        let lay = layout_for(&[], true, false);
        assert_eq!(paginate(&text("Um.{page}Dois."), &widths(), &lay, 0), vec![vec![b"Um.".to_vec()], vec![b"Dois.".to_vec()]]);
        assert_eq!(paginate(&text("Um{nl}dois"), &widths(), &lay, 0), vec![vec![b"Um".to_vec(), b"dois".to_vec()]]);
        assert!(paginate(&[], &widths(), &lay, 0).is_empty());
    }
}
