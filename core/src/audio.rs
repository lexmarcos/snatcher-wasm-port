//! Audio formats of Snatcher (Sega CD). See docs/re/audio.md for evidence.
//!
//! * RF5C164 PCM: 8-bit sign-magnitude samples (bit 7 set = positive), 0xFF = end marker
//!   (S:0x15C32 copies bytes into wave RAM and stops at 0xFF).
//! * PCMLT_01.BIN: voice table (8-byte entries, loaded at S:0x50000, read by S:0x154F8)
//!   and PCM sound-effect table at file offset 0x4000 (loaded at S:0x54000, read by S:0x1535E).
//! * PCMLD_01.BIN: voice/bank stream, addressed in 2048-byte sectors relative to the file.
//! * PCMDRMDT.BIN / type-1 banks: chains of segments `[u32 len+4][FFFFFFFF][u16 rate][6×0]`.

use crate::{be16, be32};

pub mod ym2612;
pub mod kdriver;
pub mod render;
pub mod verify;

/// RF5C164 output rate: 12.5 MHz / 384 (HYPOTHESIS from hardware docs; FD=0x800 = 1 sample/tick).
pub const PCM_CLOCK_HZ: f64 = 12_500_000.0 / 384.0;

/// Playback rate in Hz for an RF5C164 frequency-delta (FD) register value.
pub fn fd_to_hz(fd: u16) -> f64 { fd as f64 * PCM_CLOCK_HZ / 2048.0 }

/// FD used for streamed voices (S:0x156B0 `move.w #$400,$1A(a0,d3)`).
pub const VOICE_FD: u16 = 0x0400;

/// CD-DA tracks of the disc: (track number, length in sectors of 1/75 s). The engine's CD clock and the subtitle validator use them.
pub const CD_TRACK_SECTORS: [(u16, u32); 20] = [
    (2, 346), (3, 9971), (4, 20406), (5, 1431), (6, 5268), (7, 12353), (8, 15941), (9, 6404), (10, 9532), (11, 7999),
    (12, 20079), (13, 8473), (14, 5993), (15, 5709), (16, 1138), (17, 7275), (18, 3386), (19, 31548), (20, 2336), (21, 31231),
];

/// Convert one sign-magnitude PCM byte to a signed 16-bit sample.
#[inline]
pub fn sm8_to_i16(b: u8) -> i16 {
    let m = (b & 0x7F) as i16;
    let v = if b & 0x80 != 0 { m } else { -m };
    v << 8
}

/// Decode sign-magnitude bytes until the first 0xFF end marker (exclusive).
pub fn decode_sm8(d: &[u8]) -> Vec<i16> {
    d.iter().take_while(|&&b| b != 0xFF).map(|&b| sm8_to_i16(b)).collect()
}

/// Voice-table entry (PCMLT_01, 8 bytes, index = voice id used by script op 0x04).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceEntry {
    pub id: u16,
    /// byte 0 → S:$7F807, compared against channel priority.
    pub prio: u8,
    /// byte 1: 1 = segment bank loaded whole to S:0x65000 (sfx bank),
    /// 2 = streamed voice with SFX/FM ducking, 0x82 = streamed voice without ducking.
    pub kind: u8,
    /// byte 2: RF5C164 ENV (volume) for the voice.
    pub env: u8,
    /// byte 3: RF5C164 PAN as stored (driver swaps nibbles before writing, S:0x154A2).
    pub pan: u8,
    /// Start sector relative to PCMLD_01.BIN (driver adds 0x17 = file LBA, S:0x15542).
    pub sector: u16,
    pub sectors: u16,
}

pub const VOICE_TABLE_ENTRIES: usize = 1198; // entry 0 is a header, 1..=1197 used

pub fn parse_voice_table(lt: &[u8]) -> Vec<VoiceEntry> {
    let mut v = Vec::new();
    for i in 1..VOICE_TABLE_ENTRIES {
        let e = &lt[i * 8..i * 8 + 8];
        if e.iter().all(|&b| b == 0) { break }
        v.push(VoiceEntry { id: i as u16, prio: e[0], kind: e[1], env: e[2], pan: e[3], sector: be16(e, 4), sectors: be16(e, 6) });
    }
    v
}

/// PCM sound-effect entry (PCMLT_01 + 0x4000, 16 bytes, index = sfx id used by script op 0x05).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PcmSfxEntry {
    pub id: u8,
    pub prio: u8,
    /// logical channel 0/1 (each drives two RF5C164 channels)
    pub chan: u8,
    pub env: u8,
    pub pan: u8,
    pub fd: u16,
    /// Sub-CPU address of the segment header: 0x55000.. = PCMDRMDT.BIN, 0x65000.. = loaded bank.
    pub addr: u32,
}

pub const SFX_TABLE_OFFSET: usize = 0x4000;
pub const DRUM_BASE: u32 = 0x55000;
pub const BANK_BASE: u32 = 0x65000;

pub fn parse_sfx_table(lt: &[u8]) -> Vec<PcmSfxEntry> {
    let t = &lt[SFX_TABLE_OFFSET..];
    let n = t[0] as usize; // S:0x15338 `cmp.b $54000,d0` – ids must be < count
    (1..n).map(|k| {
        let e = &t[k * 16..k * 16 + 16];
        PcmSfxEntry { id: k as u8, prio: e[0], chan: e[1], env: e[2], pan: e[3], fd: be16(e, 4), addr: be32(e, 6) }
    }).collect()
}

/// One segment of a segment chain (PCMDRMDT or a type-1 bank).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// offset of the 16-byte header inside the chain
    pub offset: usize,
    /// authoring rate stored in the header (actual playback uses the sfx entry FD)
    pub rate: u16,
    pub samples: Vec<i16>,
}

/// Parse a chain of `[u32 n][FFFFFFFF][u16 rate][6 zero bytes][n bytes data incl. FF terminator]`.
pub fn parse_segments(d: &[u8]) -> Vec<Segment> {
    let mut o = 0usize;
    let mut out = Vec::new();
    while o + 16 <= d.len() {
        let n = be32(d, o);
        if n == 0xFFFF_FFFF || be32(d, o + 4) != 0xFFFF_FFFF { break }
        let start = o + 16;
        let end = (start + n as usize).min(d.len());
        out.push(Segment { offset: o, rate: be16(d, o + 8), samples: decode_sm8(&d[start..end]) });
        o = start + n as usize;
    }
    out
}

/// Minimal 16-bit PCM WAV writer.
pub fn wav_bytes(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut h = Vec::with_capacity(44 + data_len as usize);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&channels.to_le_bytes());
    h.extend_from_slice(&rate.to_le_bytes());
    h.extend_from_slice(&(rate * 2 * channels as u32).to_le_bytes());
    h.extend_from_slice(&(2 * channels).to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    for s in samples { h.extend_from_slice(&s.to_le_bytes()); }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_magnitude() {
        assert_eq!(sm8_to_i16(0x80), 0);
        assert_eq!(sm8_to_i16(0x00), 0);
        assert_eq!(sm8_to_i16(0x81), 256);
        assert_eq!(sm8_to_i16(0x01), -256);
        assert_eq!(sm8_to_i16(0xFE), 126 << 8);
        assert_eq!(decode_sm8(&[0x81, 0x01, 0xFF, 0x81]), vec![256, -256]);
    }

    #[test]
    fn segment_chain() {
        let mut d = vec![0, 0, 0, 6, 0xFF, 0xFF, 0xFF, 0xFF, 0x3E, 0x80, 0, 0, 0, 0, 0, 0];
        d.extend_from_slice(&[0x81, 0x82, 0xFF, 0xFF, 0xFF, 0xFF]);
        d.extend_from_slice(&[0xFF; 16]);
        let s = parse_segments(&d);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].rate, 16000);
        assert_eq!(s[0].samples, vec![256, 512]);
    }

    #[test]
    fn fd_rates() {
        assert!((fd_to_hz(0x800) - 32552.08).abs() < 0.1);
        assert!((fd_to_hz(VOICE_FD) - 16276.04).abs() < 0.1);
    }

    /// FM driver + renderer smoke test on real data (skipped without assets).
    #[test]
    fn fm_song_loops_if_present() {
        let raw = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/raw/");
        let (Ok(h), Ok(ipb), Ok(fm), Ok(lt), Ok(dr)) = (std::fs::read(format!("{raw}header.bin")), std::fs::read(format!("{raw}ip.bin")),
            std::fs::read(format!("{raw}files/FMWR_1.BIN")), std::fs::read(format!("{raw}files/PCMLT_01.BIN")), std::fs::read(format!("{raw}files/PCMDRMDT.BIN"))) else { return };
        let mut ip = h[0x200..0x800].to_vec(); ip.extend_from_slice(&ipb);
        let kit = render::DrumKit::new(&parse_sfx_table(&lt), &dr);
        let mut d = kdriver::KDriver::new(ip, fm);
        let r = render::render(&mut d, &kit, 0x4F, 60 * 60 * 3, 1.0);
        assert_eq!(r.loop_kind, Some(render::LoopKind::Exact));
        let len = r.loop_end - r.loop_start.unwrap();
        assert!((len as f64 / 48000.0 - 13.0).abs() < 1.0, "loop length {}", len as f64 / 48000.0);
        assert!(r.loop_corr.unwrap() > 0.9);
    }

    /// Runs only when the extracted disc files exist (never committed).
    #[test]
    fn real_tables_if_present() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets/raw/files/");
        let Ok(lt) = std::fs::read(format!("{dir}PCMLT_01.BIN")) else { return };
        let v = parse_voice_table(&lt);
        assert_eq!(v.len(), 1197);
        // entries are contiguous and cover PCMLD_01 exactly (41381 sectors)
        let mut next = 0u32;
        for e in &v { assert_eq!(e.sector as u32, next); next += e.sectors as u32; }
        assert_eq!(next, 41381);
        let s = parse_sfx_table(&lt);
        assert_eq!(s.len(), 0x49);
        if let Ok(dr) = std::fs::read(format!("{dir}PCMDRMDT.BIN")) {
            let segs = parse_segments(&dr);
            for e in s.iter().filter(|e| e.addr < BANK_BASE) {
                let off = (e.addr - DRUM_BASE) as usize;
                assert!(segs.iter().any(|g| g.offset == off), "sfx {:02x} not at a segment", e.id);
            }
        }
    }
}
