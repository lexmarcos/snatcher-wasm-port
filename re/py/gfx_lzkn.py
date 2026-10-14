"""Python LZKN1 decoder (experiments). Mirrors S:0xF348 / M:0xFF1996 (identical to Contra HC KonDec)."""
def lzkn1(d, o, header=True):
    """Decompress from d[o:]. Returns (out bytes, size_from_header or None, end offset)."""
    size = None
    if header:
        size = (d[o] << 8) | d[o+1]; o += 2
    out = bytearray(); bits = 0; n = 0
    while True:
        if n == 0:
            bits = d[o]; o += 1; n = 8
        bit = bits & 1; bits >>= 1; n -= 1
        if not bit:
            out.append(d[o]); o += 1; continue
        c = d[o]; o += 1
        if c & 0x80:
            if c & 0x40:
                cnt = c - 0xB9 + 1  # (c-0xb9) dbf -> +1 bytes
                out += d[o:o+cnt]; o += cnt
            else:
                cnt = ((c >> 4) - 7) + 1
                disp = c & 0xF
                for _ in range(cnt): out.append(out[-disp] if disp else out[len(out)])
        else:
            if c == 0x1F: break
            disp = ((c & 0x60) << 3) | d[o]; o += 1
            cnt = (c & 0x1F) + 2 + 1
            for _ in range(cnt): out.append(out[-disp])
    return bytes(out), size, o
