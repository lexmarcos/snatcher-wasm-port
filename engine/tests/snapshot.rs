//! Snapshot / restore round trip (engine/src/snapshot.rs): after a restore the game runs on exactly as it would have without one.
//! Needs the user's extracted assets (assets/raw); skipped when they are absent.
use snengine::game::{Engine, FILE_NAMES};
use snengine::snapshot::SnapError;

fn base() -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/raw") }

fn engine(pt: bool) -> Option<Engine> {
    let base = base();
    if !base.join("files/SUBCODE.BIN").exists() { eprintln!("assets missing, skipped"); return None; }
    let mut e = Engine::new();
    for n in FILE_NAMES.iter() {
        if *n == "PCMLD_01.BIN" { continue; }
        if let Ok(d) = std::fs::read(base.join("files").join(n)) { e.load_file(n, d); }
    }
    for (n, f) in [("HEADER.BIN", "header.bin"), ("IP.BIN", "ip.bin")] {
        if let Ok(d) = std::fs::read(base.join(f)) { e.load_file(n, d); }
    }
    let aw = base.join("../audio/web");
    if let (Ok(p), Ok(f)) = (std::fs::read_to_string(aw.join("pcm.json")), std::fs::read_to_string(aw.join("fm.json"))) { e.load_audio_manifests(&p, &f); }
    if pt {
        let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../i18n/build/pt-BR.pack");
        let Ok(bytes) = std::fs::read(&pack) else { eprintln!("pt-BR pack missing, skipped"); return None; };
        e.load_translation(&bytes).unwrap();
        e.set_language(true);
        e.vsub.on = true;
    }
    Some(e)
}

/// The pad of frame `f`: confirm now and then, a step down now and then (only a function of the frame, so two runs see the same input).
fn pad(f: u64) -> u8 {
    match f % 24 { 0 => 0x20, 12 if (f / 24) % 5 == 4 => 0x02, _ => 0 }
}

fn fb_hash(e: &Engine) -> u64 { sncore::gfxpatch::fnv64(&e.fb) }

/// frame, kind, script, scene, bytes
type Snap = (u64, u8, u16, u16, Vec<u8>);
struct Run { fbs: Vec<u64>, outs: Vec<String>, states: Vec<(u64, u64)>, snaps: Vec<Snap>, audio: Vec<Vec<String>> }

/// What a restored engine must start again, from the live engine at the snapshot: the CD track at its position, looping FM music, looping FM effects.
fn audio_now(e: &Engine) -> Vec<String> {
    let mut v = vec![];
    if let Some(s) = e.cd_start { v.push(format!("MusicAt {{ track: {}, at_ms: {} }}", e.regs.get(0x00CD_0000), (e.frame - s) * 1000 / 60)); }
    if let Some((id, s)) = e.fm_music { if e.voice_music_left == u32::MAX { v.push(format!("FmAt {{ id: {id}, music: true, at_ms: {} }}", (e.frame - s) * 1000 / 60)); } }
    for id in &e.fm_loops { v.push(format!("Fm {{ id: {id}, music: false }}")); }
    v
}

/// Where a run starts: a new game, or a scene root of the voice sweep (`SPnn:pc[::scene]`, story flags "zero" / "ones" / "alt").
#[derive(Clone, Copy)]
enum Start { NewGame, Root(&'static str, &'static str) }

fn begin(e: &mut Engine, s: Start) {
    let Start::Root(root, flags) = s else { return e.new_game() };
    let mut it = root.split(':');
    let (f, pc) = (it.next().unwrap(), it.next().unwrap());
    let scene: Option<u16> = it.next().and_then(|x| u16::from_str_radix(x, 16).ok());
    e.jump_path(57 + f[2..].parse::<u16>().unwrap(), u16::from_str_radix(pc, 16).unwrap(), &[]);
    if let Some(r) = scene {
        use sncore::script::vm::Action;
        e.queue.push(Action { cmd: 0x0F, args: vec![] });
        e.queue.push(Action { cmd: 0x11, args: vec![2, 0x8000, r >> 8] });
        e.queue.push(Action { cmd: 0x12, args: vec![r & 0xFF] });
    }
    match flags {
        "ones" => for b in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut() { *b = 0xFF; },
        "alt" => for (i, b) in e.vm.save[sncore::script::vm::VARS_OFF..].iter_mut().enumerate() { *b = if i % 2 == 0 { 0xAA } else { 0x55 }; },
        _ => {}
    }
}

/// Play `frames` frames from `start`; take a snapshot at the safe points `pick` chooses.
fn live(mut e: Engine, start: Start, frames: u64, pick: &mut dyn FnMut(&Engine, &[Snap]) -> bool) -> Run {
    begin(&mut e, start);
    let mut r = Run { fbs: vec![], outs: vec![], states: vec![], snaps: vec![], audio: vec![] };
    for _ in 0..frames {
        e.tick(pad(e.frame + 1));
        r.fbs.push(fb_hash(&e));
        r.outs.push(format!("{:?}", e.out));
        e.out.clear();
        if std::env::var("SNAPDBG").is_ok() && e.frame % 100 == 0 { eprintln!("{} {:?} {}", e.frame, e.safe_check(), e.debug_info()); }
        if e.frame % 50 == 0 { r.states.push((e.frame, sncore::gfxpatch::fnv64(&e.state_bytes()))); }
        if e.safe_point() != 0 && pick(&e, &r.snaps) {
            r.audio.push(audio_now(&e));
            let t0 = std::time::Instant::now();
            let _ = e.snapshot();
            if std::env::var("SNAPTIME").is_ok() { eprintln!("snapshot: {:?}", t0.elapsed()); }
            r.snaps.push((e.frame, e.safe_point(), e.vm.script_id, e.vm.scene, e.snapshot().expect("snapshot at a safe point")));
        }
    }
    r
}

fn check_restores(pt: bool, start: Start, frames: u64, after: u64, need_both: bool) {
    let Some(e) = engine(pt) else { return };
    // choose: the first menu, the first message wait, then a point after a scene / script change, a few more spread over the run
    let mut seen_key = None;
    let mut pick = |e: &Engine, got: &[Snap]| {
        let kind = e.safe_point();
        let key = (e.vm.script_id, e.vm.scene);
        let last = got.last().map(|g| g.0).unwrap_or(0);
        let first_of_kind = !got.iter().any(|g| g.1 == kind);
        let changed = seen_key.map_or(false, |k| k != key);
        seen_key = Some(key);
        got.len() < if need_both { 12 } else { 6 } && e.frame + after < frames && (first_of_kind || (changed && e.frame > last + 60) || e.frame > last + 700)
    };
    let l = live(e, start, frames, &mut pick);
    if let Start::Root(r, f) = start { eprintln!("root {r} {f} pt={pt}: {} snapshots {:?}", l.snaps.len(), l.snaps.iter().map(|s| (s.0, s.1, s.2, s.3)).collect::<Vec<_>>()); }
    assert!(l.snaps.len() >= if need_both { 3 } else { 1 }, "only {} safe points in {frames} frames", l.snaps.len());
    eprintln!("pt={pt}: {} snapshots: {:?}", l.snaps.len(), l.snaps.iter().map(|s| (s.0, s.1, s.2, s.3, s.4.len())).collect::<Vec<_>>());
    if need_both {
        assert!(l.snaps.iter().any(|s| s.1 == 1), "no command menu point");
        assert!(l.snaps.iter().any(|s| s.1 == 2), "no message point");
    }
    for (si, (f0, kind, _script, _scene, bytes)) in l.snaps.iter().enumerate() {
        let mut r = engine(pt).unwrap();
        r.new_game(); // any running state: the restore replaces all of it
        let t0 = std::time::Instant::now();
        let info = r.restore(bytes).unwrap_or_else(|e| panic!("restore at {f0}: {e:?}"));
        if std::env::var("SNAPTIME").is_ok() { eprintln!("restore {} bytes: {:?}", bytes.len(), t0.elapsed()); }
        assert_eq!(info.kind, *kind);
        assert_eq!(r.frame, *f0);
        assert_eq!(r.safe_point(), *kind, "the restored engine is at the same safe point");
        // the picture right after a restore is the picture of the frame the snapshot was taken at
        assert_eq!(fb_hash(&r), l.fbs[*f0 as usize - 1], "frame {f0}: restored picture");
        let restored: Vec<String> = r.out.iter().map(|o| format!("{o:?}")).collect();
        assert_eq!(restored, l.audio[si], "snapshot at {f0}: the audio a restore starts again");
        r.out.clear();
        for k in 0..after {
            let f = f0 + k + 1;
            r.tick(pad(r.frame + 1));
            assert_eq!(fb_hash(&r), l.fbs[f as usize - 1], "snapshot at {f0} (kind {kind}): picture differs at frame {f}");
            assert_eq!(format!("{:?}", r.out), l.outs[f as usize - 1], "snapshot at {f0}: events differ at frame {f}");
            r.out.clear();
            if f % 50 == 0 {
                let want = l.states.iter().find(|s| s.0 == f).unwrap().1;
                assert_eq!(sncore::gfxpatch::fnv64(&r.state_bytes()), want, "snapshot at {f0}: state differs at frame {f}");
            }
        }
    }
}

#[test]
fn restore_continues_exactly_english() { check_restores(false, Start::NewGame, 5200, 700, true); }

#[test]
fn restore_continues_exactly_pt_br_with_subtitles() { check_restores(true, Start::NewGame, 5200, 700, true); }

/// Other scripts and acts, FM and CD-DA music, scenes with and without voices: starts of the scripts of all three acts.
#[test]
fn restore_continues_exactly_in_other_scenes() {
    for (pt, root) in [(false, Start::Root("SP09:0", "zero")), (true, Start::Root("SP11:0", "zero")), (false, Start::Root("SP13:0", "ones")), (true, Start::Root("SP18:0", "ones")),
                       (false, Start::Root("SP21:0", "ones")), (true, Start::Root("SP22:0", "ones")), (false, Start::Root("SP26:0", "ones")), (true, Start::Root("SP30:0", "ones")),
                       (false, Start::Root("SP33:0", "zero"))] {
        check_restores(pt, root, 3200, 600, false);
    }
}

#[test]
fn snapshot_refuses_and_reports() {
    let Some(e) = engine(false) else { return };
    let l = live(e, Start::NewGame, 3000, &mut |e, got| got.len() < 1 && e.safe_point() != 0);
    let bytes = &l.snaps[0].4;
    // not at a safe point (power-on)
    let mut cold = engine(false).unwrap();
    cold.new_game();
    assert_eq!(cold.snapshot().unwrap_err(), SnapError::NotSafe);
    // header and compatibility
    let info = Engine::snapshot_compatible(bytes).unwrap();
    assert_eq!(info.script, l.snaps[0].2);
    assert!(info.act() <= 3);
    // other language
    let Some(mut pt) = engine(true) else { return };
    assert_eq!(pt.restore(bytes).unwrap_err(), SnapError::Language);
    // other disc: one file changed
    let mut other = engine(false).unwrap();
    let mut d = other.files.get("SP06.BIN").unwrap().clone();
    d[100] ^= 0xFF;
    other.load_file("SP06.BIN", d);
    assert_eq!(other.restore(bytes).unwrap_err(), SnapError::Disc);
    // missing file
    let mut none = Engine::new();
    assert_eq!(none.restore(bytes).unwrap_err(), SnapError::Disc);
    // other engine layout, other format, damage, truncation
    let mut b = bytes.clone(); b[6] ^= 1;
    let fix = |mut v: Vec<u8>| { let n = v.len() - 8; let mut h = 0xCBF2_9CE4_8422_2325u64; for &x in &v[..n] { h = (h ^ x as u64).wrapping_mul(0x0000_0100_0000_01B3); } v[n..].copy_from_slice(&h.to_be_bytes()); v };
    assert_eq!(Engine::snapshot_compatible(&fix(b)).unwrap_err(), SnapError::Engine);
    let mut b = bytes.clone(); b[5] ^= 1;
    assert_eq!(Engine::snapshot_compatible(&fix(b)).unwrap_err(), SnapError::Format);
    let mut b = bytes.clone(); b[200] ^= 0x55;
    assert!(matches!(Engine::snapshot_compatible(&b).unwrap_err(), SnapError::Corrupt(_)));
    assert!(matches!(Engine::snapshot_compatible(&bytes[..bytes.len() - 3]).unwrap_err(), SnapError::Corrupt(_)));
    assert_eq!(Engine::snapshot_compatible(b"junk").unwrap_err(), SnapError::Format);
    // a damaged payload with a valid checksum is an error too, and leaves the engine as it was
    let mut b = bytes.clone(); let n = b.len();
    for x in &mut b[60..n - 8] { *x = 0xFF; }
    let mut t = engine(false).unwrap();
    t.new_game();
    let before = t.state_bytes();
    assert!(matches!(t.restore(&fix(b)), Err(SnapError::Corrupt(_)) | Err(SnapError::Missing(_))));
    assert_eq!(t.state_bytes(), before, "a failed restore changes nothing");
    eprintln!("snapshot size {} bytes, act {}, play {:02x?}", bytes.len(), info.act(), info.play);
}
