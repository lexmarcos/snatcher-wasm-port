//! LZKN1 (Konami LZSS variant 1) decompressor.
//!
//! Evidence: Snatcher's decompressors S:0xF346/S:0xF348 (sub CPU) and M:0xFF1994/M:0xFF1996
//! (main CPU) are instruction-for-instruction the `KonDec` routine documented in
//! third_party/konami-compression-tools/m68k/decompress.asm. Output is verified byte-identical
//! to the reference C tool on every blob referenced by the game (`snt gfx-lzkn-verify`).
//!
//! Stream format: a description byte supplies 8 flags (LSB first). Flag 0 = literal byte.
//! Flag 1 = command byte `c`:
//!  - `0x1F`                 end of stream
//!  - `0ddnnnnn dddddddd`    copy n+3 bytes from distance (dd<<8|d) (1..1023)
//!  - `10nndddd`             copy nn+2 bytes from distance dddd
//!  - `11nnnnnn`             copy (c-0xB8) literal bytes from the input

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LzError {
    Truncated,
    BadDistance { at: usize },
}

/// Decompress a blob that starts with the 2-byte big-endian size header.
/// Returns the output and the number of input bytes consumed (header included).
pub fn decompress(src: &[u8]) -> Result<(Vec<u8>, usize), LzError> {
    if src.len() < 2 {
        return Err(LzError::Truncated);
    }
    let size = u16::from_be_bytes([src[0], src[1]]) as usize;
    let (out, used) = decompress_stream(&src[2..], size)?;
    Ok((out, used + 2))
}

/// Decompress a raw stream (no header). `hint` is only a capacity hint.
pub fn decompress_stream(src: &[u8], hint: usize) -> Result<(Vec<u8>, usize), LzError> {
    let mut out: Vec<u8> = Vec::with_capacity(hint);
    let mut i = 0usize;
    let mut bits = 0u8;
    let mut left = 0u8;
    let get = |i: &mut usize| -> Result<u8, LzError> {
        let b = *src.get(*i).ok_or(LzError::Truncated)?;
        *i += 1;
        Ok(b)
    };
    loop {
        if left == 0 {
            bits = get(&mut i)?;
            left = 8;
        }
        let flag = bits & 1;
        bits >>= 1;
        left -= 1;
        if flag == 0 {
            let b = get(&mut i)?;
            out.push(b);
            continue;
        }
        let c = get(&mut i)?;
        if c & 0x80 == 0 {
            if c == 0x1F {
                return Ok((out, i));
            }
            let lo = get(&mut i)? as usize;
            let dist = (((c as usize) & 0x60) << 3) | lo;
            let count = (c as usize & 0x1F) + 3;
            copy_back(&mut out, dist, count, i)?;
        } else if c & 0x40 == 0 {
            let dist = (c & 0x0F) as usize;
            let count = ((c >> 4) as usize) - 8 + 2; // (c>>4)-7 then dbf => +1
            copy_back(&mut out, dist, count, i)?;
        } else {
            let count = (c as usize) - 0xB8; // (c-0xB9) then dbf => +1
            let s = src.get(i..i + count).ok_or(LzError::Truncated)?;
            out.extend_from_slice(s);
            i += count;
        }
    }
}

fn copy_back(out: &mut Vec<u8>, dist: usize, count: usize, at: usize) -> Result<(), LzError> {
    if dist == 0 || dist > out.len() {
        // dist 0 would read the byte being written (undefined on hardware); never used by the data.
        return Err(LzError::BadDistance { at });
    }
    for _ in 0..count {
        let b = out[out.len() - dist];
        out.push(b);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals_and_copies() {
        // header 0x0008, desc 0b0000_0110: lit 'A', cmd 0x8? ...
        // desc bits (LSB first): 0 = literal 'A', 1 = cmd 0xA1 (10 10 0001: count 2+2=4, dist 1), 1 = 0x1F end
        let src = [0x00, 0x05, 0b0000_0110, b'A', 0xA1, 0x1F];
        let (out, used) = decompress(&src).unwrap();
        assert_eq!(out, b"AAAAA");
        assert_eq!(used, src.len());
    }

    #[test]
    fn raw_run_and_long_copy() {
        // desc: bit0=1 (raw run 0xC0 -> 8 bytes), bit1=1 (long copy), bit2=1 (end)
        let mut src = vec![0x00, 0x00, 0b0000_0111, 0xC0];
        src.extend_from_slice(b"ABCDEFGH");
        src.extend_from_slice(&[0x00, 0x08]); // count 0+3, dist 8
        src.push(0x1F);
        let (out, _) = decompress(&src).unwrap();
        assert_eq!(out, b"ABCDEFGHABC");
    }
}
