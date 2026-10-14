# Graphics – RE notes (owner: graphics workstream)

Conventions as in docs/RE_NOTES.md. Listings: `re/out/dis/gfx_sub.asm`, `re/out/dis/gfx_main.asm`
(generated with `python3 re/dis68k.py ... -o re/out/dis/gfx_*.asm`). Labels: `re/labels/*_10_gfx.txt`.

## 1. LZKN1 (verified)

- Sub decompressor S:0xF346 (`addq.l #2,a5` = skip size header) / S:0xF348 (stream start),
  main copy M:0xFF1994 / M:0xFF1996. Both are instruction-for-instruction the Contra Hard Corps
  `KonDec` from third_party/konami-compression-tools/m68k/decompress.asm (main version uses
  `bra.b` where the reference uses `jmp.l`; same semantics). No Snatcher-specific variant.
- Header word = uncompressed size in **bytes** (verified: decoded length == header for every blob
  of DATA_A0 scene 0, see `re/py/gfx_lzkn.py`).
- Callers:
  - S:0xF32C `lz_to_wordram`: reads size word, decompresses (a5) → S:0xB8000 (Word RAM 2M,
    main view 0x238000), then queues a main-CPU DMA (type 2) Word RAM 0x238000 → VRAM.
  - M:0xFF1902: decompress (a5)+ to RAM a6 and DMA to VRAM (main-only data).

## 2. DATA_xx.BIN = scene modules (loaded at S:0x28000)

Loaded by `subfn_04(d1=file, a1=dest)` with a1=0x28000 (e.g. S:0xE0E8, S:0xE282). Header:
- +0: long ptr → 16-byte name block "DATA_xx_BIN" (always points right after the header).
- +4: long ptr → **scene table** (used by `subfn_16` S:0xF68E: `movea.l $28004,a5`).
- +8 (only DATA_D0/D1/D2/Y16 have 3 pointers): long ptr → **sub-CPU code** entry,
  called with `movea.l $28008,a0; jsr (a0)` (S:0xE168, S:0xE292). So some DATA files carry code.
All pointers are absolute S: addresses (file offset + 0x28000).

### Scene table / scene records (`subfn_16`, S:0xF68E, d0 = scene number)
`a5 = table + table[d0]` (word offsets relative to the table). A record is a chunk list:
`[type:u8 (bit7 set = end)] [len:u8] [payload len bytes]`, jump table S:0xF6C2:
| type | handler | meaning |
|---|---|---|
| 0 | S:0xF6D6 → `subfn_08` S:0xE41A | start a palette channel (payload = channel descriptor) |
| 1 | S:0xF6E8 | VRAM upload list → $7880, processed per frame by S:0xE34E |
| 2 | S:0xF708 | create objects (sprites) in slots at S:$7A00 (0x50 bytes each) |
| 3,4 | S:0xF78E | link objects (slot a ↔ slot b) |
| 5,6,7 | S:0xF7CA | $7893 = payload[1] |
| 8 | S:0xF7D6 | word → S:$B6406 (= M:$236406, screen-mode request, see §4) |
| 9 | S:0xF7E4 | $78BC = payload[1] (chunked transfer flag → $78BD) |

#### Type 1: VRAM upload list
8-byte entries `[src:u32 (low 24 bits = S: address of LZKN1 blob)] [delay:u16 frames] [vram_dst:u16]`,
terminated by `FFFF`. Processor S:0xE34E: waits `delay` frames, then either `subfn_07`
(decompress whole blob and DMA it) or, if $78BD≠0, sends it in 0x800-byte pieces.
Blobs are stored back-to-back (end of one == start of the next, verified DATA_A0 scene 0).

#### Type 0: palette channel (`subfn_08` S:0xE41A, runner S:0xE4B4, 12 channels × 16 B at S:$9000)
Descriptor: `[kind][delay][cram_off][count-1][ptr:u32 …] (FFFF-terminated list of frame pointers)`.
`cram_off` is a byte offset into the 256-byte palette shadow S:$B6100 (= M:$236100):
0x00–0x7F = palette set A (64 colours), 0x80–0xFF = palette set B (64 colours).
Kinds (jump table S:0xE4FC): 1 copy frame(s) once (S:0xE576); 2 clear to black then fade in
to target (9); 3 fade to black step (S:0xE5C6, −2 per component); 4 fill white then fade (9);
5 fade to white step; 6 cycle frames forever (S:0xE598); 7 fade to one colour; 8 greyscale;
9 fade towards target (S:0xE6CE); 10 clear. `delay` = frames between steps (reloaded from
descriptor byte 1). After a change S:$B6404 bit0 (set A) / bit1 (set B) is set.
Colours are standard Mega Drive CRAM words `0000BBB0GGG0RRR0`.

#### Type 2: objects (12 bytes each, S:0xF708)
`[slot][flags→+$25][x:i16][y:i16][anim_table:u32][rel:i16]`; script = address after `rel` + rel.
A busy slot (`tst.b (a1)` S:0xF718) stops the chunk. The initial frame skips leading `A1`/`A7`
entries (S:0xF74A). Type 3/4 chunks: pairs `[parent][child]` → child.+$4C = parent.+$4A;
parent.+$4A = child; child.+$48 = parent (S:0xF7B2).

See §5 for the object/animation runtime.

## 3. Main CPU side (display)

- VDP register sets (table M:0xFF1584, applied by M:0xFF1506 at boot):
  `8004 8104 9011 8B00 8C00 832C 8238 8406 855F 8D2E 8700 8A3F 8F02 9100 9200`
  → **H32 mode (256×224)**, planes 64×64, plane A 0xE000, plane B 0xC000, window 0xB000,
  sprites 0xBE00, hscroll 0xB800. Alternate set M:0xFF15A4/0xFF15A6 (`9001 833C 8238 8406 857F 8D3E`:
  64×32 planes, window 0xF000, sprites 0xFE00, hscroll 0xF800), selected via M:$236406 (sub S:$B6406).
  Plane B base toggles 0xC000/0xE000 (M:0xFF1570 `8406`, M:0xFF157A `8407`) via M:$23642C.
- DMA queue in Word RAM M:0x236000 (= S:0xB6000), 16 × 12-byte entries
  `[type:u16][src:u32][dst:u32][len:u16 bytes]`, filled by S:`subfn_10` 0xE322, executed in VBlank by
  M:0xFF17C4 → M:0xFF1806; only type 2 is handled (M:0xFF183C: VRAM DMA, length/2 words).
- Palette: M:0xFF32AC copies M:$236100 (64 colours) → RAM $FFEB80 when $236404 bit0;
  M:0xFF32CA copies $236180 → $FFEC00 when bit1. CRAM DMA (M:0xFF1A5A) uploads 64 colours from
  $FFEB80 (or $FFEC80 when $E058≠0). HYPOTHESIS: set B is used for a mid-screen palette swap
  (text window / portraits) – still to verify (H-INT vector M:$A12006=0x08E2 points at an `rte`).

## 4. Rendering a scene (experiment, verified visually)
`re/py/gfx_render.py DATA_xx.BIN <scene> out.png`: apply type-0 kinds 1/2/6/9 as final colours,
decode every type-1 blob into a 64 KiB VRAM image, draw plane B (0xC000) then plane A (0xE000),
64-cell stride. DATA_A0 scene 0 = Neo Kobe street panorama, two 32-cell-wide copies side by side
(left/right half of the 64-wide plane), ~9 cell rows tall. Correct colours and no scrambling.

## 5. Object / animation runtime (implemented in core/src/gfx/anim.rs)

Object slots S:$7A00, 64 × 0x50 bytes. Fields: +0 active(w), +2 blink mask, +4/+5 flags word
(+5: bit0 hflip, bit1 vflip, bit2 add plane-A scroll, bit3 add plane-B scroll – S:0xEDBA),
+6/+A x/y 16.16, +E/+12 velocity setpoints, +16 palette override (bit7), +18 priority bits,
+1C group, +20 wait time, +25 state (bit0 paused, bit1 hidden, bit2 waiting CD time),
+26 fast, +27 repeat counter, +28 script index, +2A delay, +2E anim table, +32 script,
+36 current frame ptr, +40/+44 velocity, +48 parent, +4A first child, +4C next sibling, +4E, +4F hold.

Per frame (S:0xD71E…): `obj_update_all` S:0xF7F0 (only when no VRAM list is pending) →
`scroll_tick` S:0xF446 → `sat_build` S:0xED28 → `palch_run` S:0xE4B4 → `tilech_run` S:0xE846 →
`vram_list_tick` S:0xE34E.

Script = 4-byte entries `[b0][b1][b2][b3]`. `b0 < 0x80`: show frame b0 (frame data =
anim_table + word(anim_table + 2·b0)); delay = word(b2,b3) if b1 == 0, else random in [b2,b3)
(S:0xFD68, RNG S:0xFD96). `b0 ≥ 0x80`: opcode via table S:0xF924:
| op | meaning (evidence) |
|---|---|
| 80 | kill object (S:0xF988) |
| 81 n | goto entry n |
| 82 c _ t | repeat: if counter≠c {counter++; goto t} (S:0xF99E) |
| 83 a b t | as 82 with a random count (S:0xF9C2) |
| 84 w / 85 w | set X / Y = w, children move by the same delta (S:0xFEF0/0xFF08) |
| 86 f w / 87 f w | velocity X / Y = w.f (16.16) + parent's (S:0xFF20/0xFF4E) |
| 88 _ off | start palette channel at script+off (S:0xFA2E) |
| 89 | stop all palette channels |
| 8A _ off | start tile-stream channel at script+off (`subfn_15`) |
| 8C s w / 8D s w | scroll plane (s=0 A, else B) X / Y by w pixels (immediate) |
| 8E s w / 8F s w | scroll velocity X / Y (8.8) |
| 90 | pause self + children; 94 n unpause slot n; 95 n kill slot n; 96 n pause+hide slot n |
| 97 | hide self for the following delay (frame entry semantics) |
| 99 t2 t1 t0 | wait until CD time ($787A) ≥ t (lip-sync / music sync) |
| 9D n / AD n | sound request `subfn_12(n, 0/1)` (audio workstream) |
| 9F n | every 8 frames `subfn_06(n,-1)` |
| A0 w | flags word +4 (flip / scroll-follow bits) |
| A2 | hold while the VRAM list is busy |
| A7 n | palette override +16 = n |
| A8 n | `scroll_mode(n)` – n≥7 → main-CPU screen effect |
| AC | fast mode (+26) ; AF n group (+1C) |

Frame data: `[count-1:u16]` then per piece `[y:i16][size:u16 (SAT size<<8)][attr:u16][x:i16]`.
SAT entry = (obj.y + y + 0x80, size|link, attr ^ flip<<11 | prio, obj.x + x + 0x80);
flips mirror offsets around the piece size (S:0xEE1A, S:0xEE6C). SAT image → Word RAM S:$B6200
(M:$236200) → DMA to VRAM 0xBE00 (M:0xFF1750).

### Tile-stream channels (`subfn_15` S:0xE81A, run S:0xE846, 7 × 8 bytes at S:$8E00)
Descriptor `[kind | buf<<4][_]` + 10-byte entries `[src:u32][vram:u16][buf_off:u16][len:u16]`
(FFFF end). Buffers in Word RAM S:0xB8000/BA000/BC000/BE000. kind 0: per frame decompress
one entry (LZKN1) and DMA buf+off → vram; 1: raw copies; 2: all entries; 3: pixel-dissolve
reveal over 65 steps using the mask table S:0xEA78; 4: dissolve-out. (Runtime: TODO.)

## 6. Text font and text bitmap
- Font: 102 1bpp 8×8 glyphs for chars 0x20–0x85 at S:0x14B7C (MSB = left), proportional
  advance table at S:0x14EAC (S:0x13EB8, S:0x13F94). Export: `snt gfx-font assets/raw assets/gfx`.
- Renderer S:0x13BC8 (per character, delay $9721/$9722) draws glyphs with colour $9723 into a
  4bpp bitmap at S:0xBE000 (M:0x23E000) addressed by pixel x=$972A, y=$972B:
  tile = (y/8)·32 + x/8 (32 tiles per row, 0x1FE0 bytes = 255 tiles), then DMAs the dirty
  rows to VRAM tiles 0..254 (S:0x13F46). Nametable (S:0x13D4E): 255 entries 0x8000+i
  (priority, palette 0) DMA'd to VRAM 0xEC80 (plane A row 25).
- Control bytes (S:0x13C08): FF end; FE c colour; FD newline (y+=9, x=0x18); FC n delay n;
  F9 b → $972F; FB x y position; FA a b → $972D/$972E + wait flag; F8 w → $13BC4;
  F7 b fixed-width flag ($9728, advance 9); 00–1F → $9725 += 4·c (then skip one byte).
  (Details belong to the actions/text workstream.)

## 7. Raster (H-INT) modes on the main CPU
`hint_mode_setup` M:0xFF0AD4 runs in VBlank; $E06C selects a mode (table M:0xFF0AF8):
- mode 0 (default): H-INT every line (reg 0x8A00), handler M:0xFF0C4A counts lines; at
  line 0x88 (136) it streams 48 colours (set B colours 16–63 from $FFEC20) into CRAM 0x20,
  4 colours per line → **palette split**: top uses set A, lines ≥137 use set B lines 1–3
  (line 0 shared). Implemented in `Vdp::render` (`split_line`).
- modes 1–21: letterbox/curtain style effects (H-INT M:0xFF0CCE / 0xFF0D6E toggling display
  enable and VSRAM at given lines, $E064 = line). Driven by the main "screen effect" table
  M:0xFF348E (30 effects) selected by sub mailbox S:$B6408/S:$B640C (scroll_mode ≥ 7).
  Not yet modelled in the engine.

## 8. Engine-facing API (core/src/gfx.rs)
```rust
let mut mem = SubMem::new();
mem.map(gfx::SUBCODE_BASE, subcode_bytes);      // built-in data (font, built-in scenes)
mem.map(gfx::DATA_BASE, data_xx_bytes);         // current scene module
let hdr = gfx::data_header(&data_xx_bytes).unwrap();
let rec = gfx::scene_addr(&mem, hdr.scene_table, n);
let (mut vdp, mut rt) = (Vdp::default(), anim::Runtime::new());
rt.load_scene(&mem, rec);                       // = subfn_16(n)
every frame: rt.tick(&mem, &mut vdp); vdp.render(&mut rgba_256x224);
rt.events.drain(..)                             // sound / screen-effect requests
```
VRAM persists across scenes (scenes often only add sprites to a previous picture), so keep
one `Vdp` for the whole game and call `load_scene` as the game does. Main-CPU plane clears
(`$B6403`) → `Event::ClearPlanes`.

## For other workstreams
- Save system lives in main code at M:0xFFE100 (copied from SP offset 0x1300 → S:0xB7800 →
  M:0x237800 → M:$FFE100 by S:0x61C2 and M:0xFF07D4). It calls the BIOS BRAM entry `jsr $FDAE.w`
  with file names "SNATCHER_01".."SNATCHER_04" (M:0xFFE38A) – save slots 1–4. (actions workstream)
- SP state 1 (S:0x619C) loads file 0x35 = FMWR_1.BIN to S:0x80000 (Word RAM; main 0x200000). (audio)
- Files at 0x50000 / 0x55000: PCMLT_01 (0x38) and PCMDRMDT (0x36) loaded by S:0xE2C6/S:0xE2DA. (audio)
- Screen is H32: the engine framebuffer should be **256×224**, not 320×224.
