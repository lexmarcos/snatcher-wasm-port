# Scenario script VM (SPxx.BIN) – RE notes

Owner: script workstream. Conventions as in `docs/RE_NOTES.md` (S: = sub-CPU address).
Listing used for evidence: `re/out/dis/script_sub.asm` (generate with
`python3 re/dis68k.py re/images/sub_prg.bin 6000 're/labels/sub_*.txt' -o re/out/dis/script_sub.asm`).
Labels: `re/labels/sub_20_script.txt`.

Tools (verified):
* `snt script-disasm <SPxx.BIN> [out.txt]` – disassemble one file.
* `snt script-disasm-all assets/raw/files assets/script` – all 39 files + `xrefs.txt` (cross-file
  JUMP/CALL) + `summary.txt` (coverage + opcode histogram).
* `snt script-strings <SPxx.BIN>` – dump the text area.
* Rust API: `sncore::script` (`parse_node`, `parse_script`, `op_info`, `decode_text`, `var_read`,
  `var_write`) and the reference interpreter `sncore::script::vm` (see §9).

## 1. Verification summary

`snt script-disasm-all` (output `assets/script/summary.txt`):

* 39 files, 164,612 bytecode bytes, **100 % parsed** by a linear sweep (each file: nodes from
  offset 0 until the 0xFF padding; padding verified to contain only 0xFF, optionally one 0x00 first).
* 67,704 nodes, **0 unknown opcodes**, all 75 opcodes 0x00–0x4A occur.
* 10,728 strings in the text areas; **10,728 text operands, each resolves to a string start, and
  every string is referenced** (no orphan strings, no unaccounted bytes: after the last 0xFF only a
  single 0x00 alignment byte may follow).
* Every label (link pointer, GOTO/GOSUB/INGOSUB label, same-file JUMP/CALL) lands on a node start
  (0 violations).
* Unit tests `cargo test -p sncore script`: op table equals the bytes at S:0x16CDC, the entry
  after 0x4A is code (`47EB 0001`), variable read/write round-trip, all SP files parse.
* Dialogue decodes to correct English (e.g. SP03 @01b8 "This is your apartment building,\nGillian.").

## 2. File layout (SPxx.BIN)

| range | content |
|---|---|
| 0x0000 .. code_end | bytecode: concatenated nodes |
| code_end .. 0x37FF | padding 0xFF (one 0x00 first in 20 files) |
| 0x3800 .. EOF | text: 0xFF-terminated strings, then 0x00 if needed for even size |

SP31 (1182 bytes) has no text area. Files are loaded to S:0x1A800 (`script_buf`), text area at
S:0x1E000 (S:0x13AD0 `lea $1E000,a1`, S:0x10ECE, S:0x11324, S:0x11B04).
Script file id = global file number (SP00 = 57 … SP38 = 95); a new game starts in **SP06 (63)**
at pc 0 (S:0x16796: `move.w #$3F,d0` → queue load; `$9800` cleared by sp_init).

## 3. Node encoding

Opcode table S:0x16CDC, 4 bytes per opcode: `[t1][t2<<4 | t3][handler - 0x15400]`.
Node = opcode byte, then a 16-bit **link** pointer if `(t2|t3) & 8`, then one operand per type
in (t1,t2,t3):

| type | bytes | meaning |
|---|---|---|
| 0 | 0 | none |
| 1 | 2 | variable descriptor (see §6) |
| 2 | 2 | text offset relative to 0x3800 |
| 3 | 2 | immediate |
| 4 | node | nested node (inline) |
| 5 | 2 | label = offset in the current script file |
| 6 | 2 | resource id (file id, DATA file<<8 \| image, track, object…) |
| 7 | 0 | nothing; in list cells = **end of list** |
| 8 | 0 | marks the link pointer (already read) |

Evidence: skip routine S:0x1699A (types 0 and 7 skipped, 4 recursed, others `lea 2(a4)`),
link test S:0x16A64 (`or.b d3,d2; andi.b #8,d2`), link follow S:0x16A20, type reader S:0x16A3E.

Note on the VM's own skip: for a node with a link, `skip` jumps to the link target and skips that
node instead (S:0x1699E → S:0x16A20). The disassembler shows links as `->L_xxxx`.

## 4. Execution model

### 4.1 Main loop (S:0x166BA `subfn_20`, state word `$9808`, table S:0x166D4)

| state | addr | behaviour |
|---|---|---|
| 0 | S:0x166E0 | title/start handshake: waits for `$78AD` (<0 → S:0xEC5A then state 1) |
| 1 | S:0x16734 | init: `$980A=$980C=$9802=$FFFF`; stacks (cap,count) `$9840`=8, `$9852`=31, `$9892`=15, `$98B2`=18, `$98D8`=3, `$98E0`=16. New game (`$785C`=0): clear `$9C00..$9DBF`, load SP06, queue 0x0F. Continue (`$785C`≠0): restore `$79EA/$79F0` from `$9C02`, script id `$9C0C`, pc `$9C0E`→`$9800`, history `$9C10..$9C17`→`$98D8`, scene restore S:0x1500E. Run queue, state 2. |
| 2 | S:0x16814 | **one VM pass** (below) |
| 3 | S:0x166FA | `$7846=1`, state 5. Entered by SYS SETSTATE(1), used once: end of the final script (SP38:0e6b, after the ending's voice lines) → game end / credits (HYPOTHESIS for what `$7846` triggers) |
| 4 | S:0x1670C | queue cmd 2, wait, `st $78AE`, exit to SP (`$FF802F=2`). Entered by SETSTATE(2): "QUIT INVESTIGATION" (SP07:1a07), FINISH after saving (SP00:00b5), game-over paths → back to title |
| 5 | S:0x16730 | idle |

### 4.2 One VM pass (state 2)

1. S:0x16870: `$9805=0`; a2=0x1A800, a3=`$9800` (pc), a5=`$9940` (action queue), a6=`$A400`
   (temporary stack); **dispatch the single node at pc** (S:0x16974). Everything a pass does
   happens inside the recursive evaluation of that one node (usually a DO → SEQ chain).
2. `st $9B44` → the sequencer (S:0xFFC4, ACTIONS workstream) runs the queued actions over the
   next frames; the loop waits until `$9B44`=0 and `($1511E & 7)`=0 (S:0x1681E).
3. S:0x16892: if `$9802` ≠ $FFFF (pending jump): push old pc on the history stack `$98D8`
   (S:0x1071E: drops the oldest when full), `pc = $9802`, `$9802 = $FFFF`, `clr.b $9716`.
4. If the pass registered menu items (`$9853` = count of `$9852` ≠ 0): run the menu UI
   (S:0x1398E, S:0xD4AC, S:0x10CBE, S:0x11004 – ACTIONS) → `$79F4` = chosen index (1-based) or
   negative = cancel. Choice: push `menu[idx-1]` (a text offset) on the **path** stack `$9892`.
   Cancel: if path length ≠ (top of `$9840` or 0) pop one path element. Clear the menu list.
5. Else (no menu): truncate the path to (top of mark stack `$9840`, or 0) (S:0x16922), clear menu.
6. Back to the main loop (state 2 again).

### 4.3 Menu path, depth and the statement guard

* `$9892` = **path**: the menu choices made so far (text offsets of the CHOICE labels).
  `$9893` = its length. `$9804` = current CHOICE depth during the pass.
* `$9852` = menu items collected in this pass (text offsets), `$9853` = count.
* `$9840` = **mark stack** (depths to which the path returns after a leaf action).
* `$9805` = "leaf executed" flag, set by CHOICE when its body ran at full depth and registered
  no menu item.
* **Guard** S:0x16C90 – every *action* opcode first checks it and does nothing when inactive:
  ```
  if count($9852) != 0  -> inactive        (menu being collected)
  if $9802 != $FFFF     -> inactive        (jump pending)
  if $9805 == 0         -> active iff $9804 == count($9892)
  else                  -> active iff mark stack empty or top(mark) < $9804   (unsigned bytes)
  ```
  Guarded opcodes: 00 04 05 0A 0B 0D 0E 0F 13 14 1C 1D 2B 2C 36 37 39 3D 41 42 43 44 45 46 47 48
  49 4A. Not guarded: control flow (18 19 1A 1B 1E–27 30 34 35 38 3E 3F 40 0C 3A), expressions.

### 4.4 Stacks

Struct: byte 0 = capacity, byte 1 = count, then words. Helpers (S:0x1071E–0x10840):
`get(i)` 0x107DC (i<count), `top` 0x107FE, `peek_from_top(n)` 0x107F4 (= get(count-n)),
`push` 0x10806/0x10732 (fails when full), `push_drop_oldest` 0x1071E, `remove(i)` 0x1074E,
`drop` 0x10778, `contains(x)` 0x10788, `pop` 0x10816, `truncate(n)` 0x1082C, `clear` 0x10840.

| addr | cap | name | use |
|---|---|---|---|
| $9840 | 8 | marks | MARK/UNMARK depths |
| $9852 | 31 | menu | menu items of the current pass |
| $9892 | 15 | path | chosen menu items |
| $98B2 | 18 | calls | CALL/RETURN: pushes (script id, return pc) |
| $98D8 | 3 | history | previous pcs (on every jump) |
| $98E0 | 16 | gosubs | GOSUB return pcs |

Other VM globals: `$9800` pc, `$9802` pending jump, `$980A` current scene id, `$980C` current
script file id, `$9808` main state.

## 5. Opcodes

Notation: `[x]` = operand, `tuple(a,b)` = a nested LIST tuple node (opcodes 1E–24) whose scalar
fields the handler reads at fixed byte offsets. `Q nn(args)` = entry appended to the action
queue at `$9940` (word command, then args). "G" = guarded (§4.3).

| op | mnem | types | handler | semantics |
|---|---|---|---|---|
| 00 | ANIM | 4 | 17330 | G. `[00][tuple(obj,mode)]` → Q 09(mode, obj) (reads +2..+5) |
| 01–03 | ADD | 41,13,11 | 1764A | d5 = a + b (16-bit) |
| 04 | CMD0D | 6 | 173A4 | G. Q 0D(res) |
| 05 | CMD0C | 6 | 173C0 | G. Q 0C(res) |
| 06–09 | AND | 44,41,14,11 | 1766A | d5 = a & b |
| 0A | MUSIC | 6 | 173EC | G. Q 07(res) (seq: `$980E`=res, $FF→0, subfn_12 d1=1) |
| 0B | CALL | 4 | 17150 | G. `[0B][tuple(file,label)]`: push `$980C` on calls; Q 11(0x1A800,file) unconditionally, `$980C`=file; push (gosubs[0] if any else pc) on calls; `$9802`=label |
| 0C | GOSUB | 5,7 | 1720C | push pc on gosubs; pc=label; dispatch(label); pc=pop (inline, same pass) |
| 0D | CMD0E | 6 | 17418 | G. Q 0E(res) |
| 0E | INC | 1 | 170C4 | G. var += 1 unless var == max(width) |
| 0F | SOUND | 6 | 17378 | G. Q 1C(res) (seq: `$980F`=res, $3D→0, subfn_12 d1=0) |
| 10–12 | EQ | 45,43,13 | 176C2 | d5 = (a == b) ? $FFFF : 0 |
| 13 | CLR | 1 | 170AA | G. var = 0 |
| 14 | SETALL | 1 | 17090 | G. var = all ones |
| 15 | HISTORY | 3 | 17248 | d5 = history.peek_from_top(n) or $FFFF |
| 16 | GT | 13 | 17752 | d5 = (a > b, unsigned) ? $FFFF : 0 |
| 17 | GE | 13 | 1772E | a >= b |
| 18–19 | IFELSE | 4/1,4,link | 16E6E | `[op][else][cond][then]`: cond≠0 → dispatch(then) else dispatch(else) |
| 1A–1B | WHEN | 4/1,4 | 16E52 | `[op][cond][body]`: cond≠0 → dispatch(body) |
| 1C | LET | 1,3 | 17776 | G. var = imm |
| 1D | LET | 1,4 | 17776 | G. var = eval(node) |
| 1E–24 | LIST (tuples) | 33,34,32,65,63,66,64 | 16E12 | data tuples; as a statement = SEQ |
| 25 | LIST | 4,4 | 16E12 | cons: `[25][car][cdr]` – **SEQ**: execute all elements |
| 26 | LIST* | 4,link | 16E12 | `[26][rest][car]`: element car, list continues at `rest` |
| 27 | LIST. | 4,7 | 16E12 | `[27][car]`: last element |
| 28–29 | LT | 43,13 | 1770A | a < b |
| 2A | LE | 13 | 176E6 | a <= b |
| 2B | GOTO | 5 | 170FA | G. `$9802`=label |
| 2C | JUMP | 4 | 170FA | G. `[2C][tuple(file,label)]`: if file≠`$980C`: Q 11(0x1A800,file), `$980C`=file; `$9802`=label |
| 2D | NOP | – | 1702C | nothing |
| 2E–2F | NOT | 4/1 | 176AA | d5 = (a == 0) ? $FFFF : 0 |
| 30 | DO | 4 | 16E08 | dispatch(child) |
| 31–33 | OR | 44,41,11 | 1768A | a \| b |
| 34 | CASE | 3,4 | 17018 | `[34][k][body]`: dispatch(body). In lists counts as k elements; SWITCH key |
| 35 | CASE | 3,7 | 17018 | `[35][k]`: no body |
| 36 | MARK | 7,4 | 1702E | G. m=top(marks) or 0; if m < `$9804`: push `$9804` on marks, dispatch(body); else nothing |
| 37 | MARK | 7,7 | 1702E | same without body |
| 38 | CHOICE | 2,4 | 16F94 | `[38][text][body]` – see §4.3: if `$9802`≠$FFFF return; if depth==len(path): push text on menu; else if path[depth]==text: depth++, dispatch(body), if depth==len(path) && no menu items: `$9805`=$FF; depth-- |
| 39 | UNMARK | 7 | 17078 | G. drop top of marks |
| 3A | RETURN | 7 | 171D2 | `$9802`=pop(calls); id=pop(calls); if id≠`$980C`: Q 11(0x1A800,id), `$980C`=id |
| 3B | INGOSUB | 5 | 17266 | d5 = 1 if label ∈ gosubs else 0 |
| 3C | GOSUBPC | 3 | 17266 | n==0 (low byte): d5=pc; else gosubs.peek_from_top(n) or $FFFF |
| 3D | SCENE | 6 | 17300 | G. if res≠`$980A`: (if res>>8 ≠ `$980A`>>8: Q 0F, Q 11(0x28000, res>>8) else Q 0F); Q 12(res & 0xFF); `$980A`=res |
| 3E | RANDOM | 1,4 | 16F4C | n=count(list); k=random(1..n); dispatch(nth(k)); var=k |
| 3F | CYCLE | 1,4 | 16EF0 | n=count(list); v=var; dispatch(nth(v+1)); if no menu items: if v+1≠n: var=v+1 |
| 40 | SWITCH | 1,4 | 16E96 | v=var (low byte); walk CASE cells; first CASE k with k==v → dispatch(body) |
| 41 | SYS | 6 | 17460 | G. `[41][fn]` |
| 42 | SYS | 4 | 17460 | G. `[42][tuple(fn, …)]` – §5.1 |
| 43 | SAY | 4 | 172C0 | G. `[43][tuple(spk,text)]` → Q 08(spk, text) |
| 44 | SAY | 2 | 172C0 | G. `[44][text]` → Q 08(0, text) |
| 45 | ANIMWAIT | 6 | 17358 | G. Q 09(3, obj) |
| 46 | CMD0A | 7 | 173DC | G. Q 0A |
| 47 | CMD13 | 7 | 17408 | G. Q 13 |
| 48 | CMD0B | 7 | 17434 | G. Q 0B |
| 49 | CMD19 | 7 | 17394 | G. Q 19 |
| 4A | WAIT | 3 | 17444 | G. Q 10(frames) |

Comparison results are word-sized `$FFFF`/0 (`sXX.b d5; ext.w d5`); comparisons are unsigned
(`scs/scc/shi/sls`). Operand evaluation S:0x16ACA: type 1 → variable value, type 3/5 → literal,
any other type → dispatch the nested node and use `d5` (the "return value" register).

List helpers: `count` S:0x16BE2, `nth` S:0x16C32 (1-based; a CASE-k car spans k slots; a
non-list node terminates the chain and is itself the last element). SEQ (S:0x16E12) executes
`nth(i)` for i = 1..count – so a `CASE k body` element runs its body k times.

### 5.1 SYS sub-functions (table S:0x17492; fn word at node+1 (op 41) or node+2 (op 42))

Argument bytes are read at fixed offsets from the SYS node (`a2+a3+n`):

| fn | name | args | effect |
|---|---|---|---|
| 0 | Q04 | – | Q 04 |
| 1 | SAVE | +4 slot | Q 1E(slot) and snapshot the state into `$9C02..$9C5F` (S:0x14F12) |
| 2 | Q21 | – | Q 21 |
| 3 | Q1F | – | Q 1F |
| 4 | MINIGAME | +4 id | `$78E0`=id, clear `$78E4..$78EA`, `st $78AF`, Q 18 (shooting / special sequence) |
| 5 | PEEK | +4 addr | d5 = word at sub-CPU address addr |
| 6 | POKE | +5 addr, +7 val | addr==$FFFF: `$1512A`=val (byte) else word poke |
| 7 | Q1D | +4 n | Q 1D(n≠0 ? 1 : 0) |
| 8 | Q15 | +4 flag, +8 a, +10 b | word at +4 ≠ 0 (i.e. a nested tuple follows): Q 15(a,b); else Q 09(2, 1) |
| 9 | SETSTATE | +4 n | `$9808` = n+2 (main-loop state) |
| 10 | SET79FE | +4 n | `$79FE`=n; n==1 clears `$96D0/$96D4` |
| 11 | SCENESNAP | +4 n | n==0: snapshot scene/music/objects into `$9C18..$9C5F` (S:0x14F70); else restore (S:0x1500E) |
| 12 | RESETFLAGS | – | clear bit array from `$9C95` (keeps top 5 bits of $9C95) for 0x12A bytes |
| 13 | Q1B | +5 k, +8 a, +10 b | Q 1B(k, a, b) |

PEEK/POKE addresses used by the scripts (engine must map them to its own state):
`$79E2/$79E4/$79E6/$79E8` (save-slot used flags, written by the save command S:0x104C6),
`$79E0` (save/load result), `$CDAC`, `$7910 $790A $790C $790E $7908 $7916` (counters in the
shooting code S:0x12680–0x13002), `$7972` (S:0xD770), `$96DC` (S:0x10894), `$79EE $79F0`
(snapshot S:0x14F88), `$9718 $971A $971C $971E` (menu UI S:0x10CBE–0x11E48), `$FFFF`→`$1512A`.

## 6. Variables

Descriptor d: byte = d>>7 (0..511), shift = (d>>4)&7, width = (d&15)+1. Storage: bit array at
**S:$9C60** (inside the save block `$9C00..$9DBF`). Read S:0x16B14: long = (b0,b1,b2,0),
rotate left by shift+width, mask width bits. Write S:0x16B80 symmetric. The disassembler prints
`v[BBB.S]` (1 bit) or `v[BBB.S:W]`. Rust: `var_read`/`var_write`.

## 7. Text encoding

ASCII 0x20–0x7E as is. Control bytes (stage-1 copier S:0x13AC0, typewriter S:0x13BC8):

| byte | meaning | evidence |
|---|---|---|
| FF | end of string | S:0x13AEC |
| F2 | new line (12,414 uses) | S:0x13AF2 → `$974A`=1 |
| F6 | page break (HYPOTHESIS: wait for button, then continue on a new page) | S:0x13AFE → `$974A`=3; used mid-string, e.g. SP26 "…Stegasaur.{page}Thanks for calling!" |
| F4 | auto-advance (HYPOTHESIS: no button wait) | S:0x13B58 → `$97CC`=2; only at string ends in action/shooting scenes ("Gillian! Watch out!!{F4}") |
| F8–FE | colour n = byte & 7 (re-emitted as `FE n`) | S:0x13B12; F9 = normal (1), FA = 2 (button names, debug), FB = 3 (key info, e.g. phone numbers), FC = 4 (alerts), FE = 6 |
| EC / EE / F0 | button-name placeholder: letter A/B/C for "draw blaster" / "light" / "fire" under the pad configuration `$78BA` (0–5) | S:0x13B64 tables S:0x13BA0, S:0x13BAC, S:0x13BB8 |

Button placeholder mapping (config `$78BA` → (EC, EE, F0)), decoded from the three word tables:
0:(A,B,C) 1:(A,C,B) 2:(B,A,C) 3:(C,A,B) 4:(C,B,A) 5:(B,C,A).

Speaker names (SAY spk): table S:0x13552 = SUBCODE.BIN offset 0x6152, 48 signed word offsets
relative to the table; entry index = spk-1; entry 0 is empty (spk 1 = no name box; spk 0 from
op 44 → `$9744`=$FF = no name). Each name = attribute byte ('2' → colour 2) + ASCII + FF
(S:0x136E6 builds `FC 00 F9 01 FB 10 <len> FE` + colour + name). spk 2 = Metal Gear, 3 = Gillian,
4 = Mika, 5 = Togo, 6 = Driver, 7 = Chief, 8 = Harry, 9 = Napoleon, 10 = Jamie, … 48 = Random.
`sncore::script::speaker_names(subcode)` reads the table from the user's SUBCODE.BIN (names are
not embedded in the repository).

## 8. Special scripts, variables and verification by execution

* **SP00** (id 57): pc 0 = `JUMP SP01:0` (debug entry); `L_0007` = the save routine called with
  `CALL SP00:L_0007` (PEEK `$79E0` save result, "Save completed!", CONTINUE GAME / FINISH).
* **SP01** (id 58): "This file is Command Debug." – a developer scene-select menu with 83 CHOICE
  leaves (ACT1 IN / NEW GAME, GO TO KOUJO, …) that set the story flags through GOSUBs and JUMP
  into the target script. Useful for the engine's scene-jump feature (`pc=SP01:0`).
* **SP06** (id 63) pc 0: start of a new game (Junker HQ reception: "Welcome to Junker
  Headquarters. May I help you?").
* Variable cross reference (all reads/writes per descriptor): `assets/script/vars.txt`
  (generated). `v[000.0:16]` (desc 0x000F) is a scratch register (165 writes).
* Cross-file JUMP/CALL list: `assets/script/xrefs.txt`.

**Execution test** – `snt script-run assets/raw/files "<picks>" <passes>` runs the reference
VM headless from a new game with a stub host and prints scenes, dialogue and menus. Verified:
the opening plays as in the game (DATA_H15/H01/H00 images, music 0x4F, the receptionist's line),
the top menu is ENTER/LOOK/INVESTIGATE, choosing ENTER repeatedly cycles the receptionist's
answers and then reveals TALK; TALK leads to the LOOK/INVESTIGATE/TALK/ASK menu; LOOK→MIKA cycles
through three different lines and then repeats the last one; ASK→ABOUT JUNKERS plays the
two-line explanation with the coloured "'bioroid SNATCHER'".

**Random-walk stress test** – `snt script-run assets/raw/files random:<seed> <passes> [SPnn:pc]`
picks random menu items (never SAVE/FINISH, 1/8 cancels). 40 walks × 20,000 passes starting at
the debug menu `SP01:0000` visited SP01–SP05, SP08–SP13, SP16–SP23, SP25–SP38 and reached the
ending (state 3) 15 times, with no VM error, out-of-range pc or runaway recursion.

## 9. Engine interface (reference interpreter `sncore::script::vm`)

`core/src/script/vm.rs` is a byte-level port of every handler above (no tree pre-parse, so
it reproduces the original's quirks: link-following skip, CASE slot counting, byte arithmetic,
unsigned compares, unguarded RETURN, RNG S:0xFD96). Tests: `cargo test -p sncore script`.

```text
let (mut vm, q) = Vm::new_game();          // or Vm::from_save(&save_block, host)
loop {
    perform(q)                             // actions = original queue entries (§5, §10)
    if vm.state != 2 { handle main state 3/4 (ending / back to title) }
    match vm.finish_pass() {
        PassEnd::Continue => {}
        PassEnd::Menu(items) => { show items (text offsets, current script); vm.choose(Some(i) | None) }
    }
    q = vm.run_pass(&current_script_bytes, host);
}
```

* `Action { cmd, args }` – Q 11 has args `[addr_hi, addr_lo, file]`; when addr = 0x1A800 the
  engine must switch `current_script_bytes` to that SP file *after* the pass (the VM itself
  already updated `script_id`); when addr = 0x28000 it loads a DATA file for SCENE.
* `Host` – `peek/poke/poke_byte` of engine registers by original address (the PEEK/POKE list in
  §5.1 plus snapshot fields `$79EA $79EC $79F0 $7988 $78BA $78B8 $78A2 $15120 $980E $79EE $79FE
  $1512A $7890`), and `snapshot_objects` (64-byte animation-object snapshot, S:0x14FB4 format).
* Save game = `vm.save` (0x1C0 bytes, the `$9C00` block); `Vm::from_save` restores it like
  main-loop state 1.
* RNG: the original advances `$786A/$786C` also from other code paths (S:0xD644 etc.), so exact
  random sequences depend on frame timing; the VM exposes `rng_a/rng_c` and `rand16`.

## 10. For other workstreams

* Action queue command numbers emitted by the VM: 04 07 08 09 0A 0B 0C 0D 0E 0F 10 11 12 13 15
  18 19 1B 1C 1D 1E 1F 20 21 (20 only from the scene restore S:0x1500E; sequencer table S:0xFFE2, 36 entries). Args as in §5.
* Q 08(spk, text_off) = dialogue box; Q 07 = music, Q 1C = sound, Q 11(dest, file) = file load,
  Q 12(img) = show image of the loaded DATA file, Q 09(mode,obj) = animation object control
  (objects at `$7A00 + obj*0x50`; mode 0..5, 3 = wait until finished).
* Frequent pattern `CMD0D #nnnn` + `CMD0A`: Q 0D(n) then Q 0A (seq cmd 10 waits ≤ 0xE10 frames or
  for a button) – HYPOTHESIS: Q 0D = play voice clip n (ids 0x000–0x4A0), Q 0A = wait for it.
* Save block `$9C00..$9DBF` (0x1C0 bytes) is exactly what the save command writes to backup RAM
  ("SNATCHER_0n", S:0x10592).
