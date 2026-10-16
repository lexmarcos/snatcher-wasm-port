# Voice subtitles (pt-BR)

The original game never shows text with a voice clip (docs/voice-inventory.md: the message box stays dark and empty while a clip plays; the script waits on it with `CMD0A`).
A pt-BR player would hear 83 minutes of English with nothing to read, so the port can draw a **subtitle in the empty message box** while a clip plays. This file describes the
whole chain: transcription, subtitle data, validator, engine, front end, status per act. **English transcripts are game content and are never committed**: they live in `work/` (gitignored).

## Status

| act | clips with a subtitle | transcribed | translated | reviewed |
|---|---:|---|---|---|
| all (1146 clips started by scripts, 82.8 min) | | 1146 (Whisper large-v3, 2 passes) | | |
| Act 1 (SP06-SP19) | 291 | yes | yes | yes, 4/5 (review below); layouts by portrait: see section 4 |
| Act 2 (SP20-SP35) | 0 | yes | no | no |
| Act 3 (SP36-SP38) | 0 | yes | no | no |

## 1. Transcription (`work/voice/transcripts/`, not committed)

```sh
./target/release/snt voice-export assets/raw/files work/voice/wav        # 16 kHz mono WAV of the 1146 script-started clips + index.json (seconds per clip)
python3 -I scripts/voice-transcribe.py                                    # Whisper, 3 requests at a time, resumable (raw/NNNN.json); merges into en.json / en.tsv
python3 -I scripts/voice-transcribe.py --alt                              # second pass without the names prompt (raw_noprompt/), kept as `alt` in en.json
python3 -I scripts/voice-script.py                                        # work/voice/script/SPnn.txt: script lines and transcripts interleaved (context for translators)
```
* Server: the user's local OpenAI-compatible Whisper server on the LAN (`large-v3`, `language=en`, `word_timestamps=true`, temperature 0, a prompt with the proper names).
  URL and key come from `work/whisper.env` or the environment (`WHISPER_URL`, `WHISPER_API_KEY`); never committed. Audio only goes to that LAN server.
* `en.json`: per clip text, segments, words with times, `avg_logprob`, `no_speech_prob`, `alt` (second pass), `speech` (bool) and `flag`. `en.tsv` is the flat copy the Rust tools read.
* **Non-speech** (no subtitle): empty Whisper output (34 clips: effects and screams, mostly ids 36-68), pure vocalisations (`ah`, `ugh`, `whoa`, `oh oh oh`: 10 clips), the usual Whisper
  hallucinations (`Thank you`, `Thanks for watching`) on a clip without speech, `no_speech_prob > 0.3`, `avg_logprob < -0.8`. Manual decisions and corrected spellings
  (`Killian`/`Gideon` -> `Gillian`, `Jean-Jacques` -> `Jean-Jack`) are in `work/voice/transcripts/overrides.json` (`{"id": {"text": ..., "speech": false, "why": ...}}`).
* Quality: names are right with the prompt (the second pass without it only differs in names and punctuation: 93 of 1146 clips differ, all trivial); the SP09 file-reading pictures
  show the English page that is read aloud and match the transcript. Contexts were read for all of Act 1.

## 2. Data: `i18n/pt-BR/voice.json`

```json
{"lang": "pt-BR", "clips": {"77": {"h": "90276f11", "d": "3.501", "s": "3", "t": "Sou Gillian Seed. Fui designado{nl}para o QG Junker a partir de hoje."}}}
```
Keyed by clip id (one entry per clip, not per trigger). `h` = FNV-1a (the hash of the SP files) of the English transcript, so a new transcription that changes the English is detected;
`d` = clip duration in seconds; `s` = speaker number as printed by `snt i18n-names` (2 Metal Gear, 3 Gillian, 4 Mika ...; absent = no name, only when it is clear from the context);
`t` = text with the usual markup (`{nl}`, `{page}`, colours; no `{auto}`, no `{btn}`). Clips without an entry get no subtitle.

The text is stored **unwrapped**: the engine wraps and pages it at run time for the room the screen leaves (section 4). `{nl}` and `{page}` are only author hints (a forced line / page break, used for the two
lists of clips 152 and 155); everything else is plain running text.
Authoring: write `id<TAB>speaker<TAB>text` lines (speaker number, name or 0; `|` forces a line break; a leading `~` is accepted and ignored) and run `snt voice-import i18n/pt-BR assets/raw/files patch.tsv` (fills `h` and `d`).
`snt i18n-build` puts the subtitles in the pack (a voice section after the names; old packs without it still load).

## 3. Validator

`snt voice-check i18n/pt-BR assets/raw/files` (also run by `snt i18n-check i18n/pt-BR assets/raw/files all`):
the clip exists and its duration matches; the English hash matches `work/voice/transcripts/en.tsv` (skipped with a warning when the file is absent) and the clip is not flagged non-speech;
markup and characters; the speaker is a name; **every layout a clip can get** (`vwrap::all_layouts`: box, lane next to a left portrait, lane next to a right portrait, band, compact) is wrapped with the engine's own code
(`sncore::vwrap::paginate`, real glyph widths, the speaker name inline for the band) and checked: no line wider than the layout (a word that does not fit), no page over the layout's line limit;
**reading speed**: the pages share the display time in proportion to their text, a clip may stay up for a hold after its end, and the text must be readable at **17 characters per second**
over clip + hold (`READ_CPS`). The time is counted from `SETTLE` frames after the start (the text waits for the portraits), and a page must also hold that speed on its own in every layout.
It also prints the number of pages per layout (Act 1: box 323, lane 342, band 364, compact 364) and, for the record, what the 72 px gap between two portraits would need (490 pages, up to 9 for one clip: not used).

## 4. Engine (`engine/src/vsub.rs`, `core/src/vwrap.rs`)

### What the strip looks like (measured on the engine, `engine/examples/strip_probe.rs`)
* The picture ends at row 136 (rows 137..148 are black). The dialogue strip holds the **portrait frames of the people talking**: 64x64 px (white frame included), rows 149..212, in three slots: **left X 24..87, middle X 96..159, right X 168..231**
  (a few scenes use X 32). Below them rows 213..223 are black. The message box (256x64 at row 150) is empty and dark while a clip plays.
* A portrait is three display objects (slots 5..14, in threes: eyes, mouth, frame; the frame object sits at y 146). The slot a portrait uses depends on the scene, not on its position. Portraits **appear when the clip that loads them starts**
  (the sequencer's 0x0D loads the portrait scene `S:0x13FEC[id]`): the objects exist 11 frames after the voice start and are visible 15 frames after it (SP06:032e: voice at frame 8, visible at 23). They stay until the scene changes; later clips add more.
  In the 207 Act 1 clips sampled the set never changed after the first 30 frames of a clip. Sets seen: none 36 %, left 7 %, right 9 %, left+right 12 %, left+middle 12 %, all three 20 %, middle only 3 %.
* **Who talks**: the mouth object of the speaking portrait runs an animation (its script index moves, about every 6 frames, for the length of the clip); the mouths of the others are paused. One clip = at most one talking portrait.
  In SP06:032e clip 77 Gillian's mouth moves, in 78 and 79 (Mika, who is not in the strip) no mouth moves.

### Layouts (`sncore::vwrap::layout_for`, chosen from the occupied columns of the strip, scanned on the finished frame)
| screen | layout | text area | name | page |
|---|---|---|---|---|
| no portrait (also black screens) | **box**: the message box as before | X 24..232, rows 150.. | own line, X 16 | 3 lines (4 max) |
| one portrait, or the widest free column >= 120 px | **lane**: next to the portrait, same rows as the box | right of the left one: X 96..232 (136 px); left of the right one: X 24..160 | own line, first line of the lane | 4 lines (5 max) |
| two or more portraits (left+right leave 80 px, left+middle 72 px, ...) | **band**: black band full width just above the portraits, growing upwards (bottom row 147), the picture is covered only where the text needs it (10 rows per line, the 12 black rows above the portraits are free) | X 24..232 | inline "Name: " in the speaker's colour, on every page | 2 lines (3 max) |
| the picture reaches the bottom (no strip) | **compact** band at the bottom edge, as before | X 24..232 | inline | 2 lines (3 max) |

The portrait frames are never painted over: a lane only starts 8 px from a frame, the band is above row 149. Text uses the game glyph renderer and the pt-BR 10 px layout; the window palette; no backing is needed in box and lane (the strip is black there).
**Why the band and not the 80 px gap**: between left and right portraits there are 80 px (72 of text): about 10-11 characters per line, 490 pages for Act 1 (up to 9 for a clip) against 364 in the band (about 30 characters per line, 3 lines); the band costs at most 20 rows of the bottom of the picture
(3 lines; 10 rows for 2 lines) and leaves the whole strip for the faces, which is what the user asked for. Screenshots: `work/voice/shots/layout/` (not committed): lane left (Gillian, SP06:032e), lane right (Metal Gear, SP06:1332), band with two portraits (Mika + Chief, SP06:05d6, as the reference frames), three portraits, a long text, box, compact.

### Run-time wrapping and paging (`vwrap::paginate`)
Greedy line fill with the real advance widths for the room of the layout (the first line of a page is shorter by the inline name); pages end where a sentence or clause ends (`break_severity`, the same dynamic programme the importer used: fewest pages, a break inside a clause is expensive);
every page is then re-balanced (narrowest limit that keeps the same number of lines, so no one-word last line). `{page}` is a forced break, `{nl}` a forced line break. The time of the clip (plus hold) is shared by the pages in proportion to their text, as before.

### Stability (no jumping text)
* The text is not drawn until the portraits are on screen: for a clip that loads a portrait scene `SETTLE` = 18 frames after the start (the portraits arrive after 15); other clips draw at once. The reading time and the hold are counted from there.
* The layout is chosen once, on the first drawn frame, from what the strip shows, and kept for the clip (the band keeps its height too: that of the longest page).
* If the strip changes later: a portrait that appears where the text is (lane / box) forces a new layout at once, re-wrapping the text still to be shown (from the current page); anything else (a portrait leaves, one appears in the band's case, the strip disappears) waits for the next page.

### Speaker
The name comes from `voice.json` (`s`); the portrait animation tells who talks, and it is used **to check and complete the speaker data**, not at run time (the art of a portrait is not a name, a wrong name is worse than none):
`python3 -I scripts/voice-speakers.py` (needs `strip_probe survey`, section 6) maps portrait art to speaker numbers by votes of the clips that have one and lists PROPOSE (no speaker, a mapped portrait talks) and CHECK (voice.json disagrees with the talking portrait). Act 1: clip 88 got
Gillian (the Chief introduces himself in 87, Gillian answers in 88, the Chief goes on in 89, and Gillian's mouth moves in 88); clip 92 ("Você é casado, não é?") was labelled Chief but Mika's mouth moves: unsure, so it shows no name now. Open CHECKs to listen to: 332 and 357 (labelled Gillian, Metal Gear's portrait talks).
In the box and lane layouts the name is its own line in the speaker's colour; in the band and compact layouts it is inline ("Chefe: ...") in the speaker's colour.

### The rest
* **Overlay on the finished frame**: no game state, script flow or timing is touched (tests compare `debug_info` frame by frame); with the option off, in English, or without a subtitle for the clip nothing is drawn, so the output is byte-identical.
* **Start/stop**: sequencer command 0x0D with an id >= 36 starts the subtitle (`Out::Voice` is emitted at the same place); it ends when the clip is over plus the hold, when the
  script starts another clip (replaced), draws a message, opens a menu, or runs a command that changes the picture (fade, clear, scene, load).
* **Timing**: display time = `voice_left` (clip length + 6 frames) + hold; the hold gives fast clips the reading time they lack (`voice_display_frames`: `ceil(chars / 17 * 60) - clip` frames counted from the first drawn frame, at least 15, at most 90).
* **API**: wasm `set_voice_subtitles(bool)`, `voice_subtitle_count()`, `voice_subtitle_active()` (-1 or the clip id), `voice_subtitle_layout()` (0 none, 1 box, 2 lane, 3 band, 4 compact); native `Engine.vsub.on`, `Engine::vsub_info()` (layout, page, occupied columns, painted rectangle).
  `debug_jump("SP09:0ff4:070e")` also shows a scene resource first (DATA index, image) for screens that an earlier SCENE node would set up.
* Tests: `engine/tests/vsub.rs` (English byte-identical; pt-BR with the option off unchanged; subtitle on during the clip, gone after; game state identical; **portrait rectangles pixel-identical with the subtitle on, every changed pixel inside the reported rectangle, which clears the portraits;
  layouts 0 / 1 / 2 / 3 portraits and no strip; stable layout; first draw waits for the portraits**), `engine/src/vsub.rs` unit tests (strip scan, when a change forces a new layout), `core/src/vwrap.rs` (layouts, wrapping, balance), `core/src/i18n.rs` (pack round trip, timing).

## 5. Front end

Menu panel (Esc) > Display > **Legendas de voz / Voice subtitles** (`web/index.html` `#vsub`): on by default in pt-BR, off (and disabled) in English; an explicit choice is stored in
`localStorage` `snatcher.vsub` (`1`/`0`, every access in try/catch); `?vsub=0|1` overrides it for the visit. Screenshot check: `web/test/vsub.ts` (`PORT=5300 bun test/vsub.ts "scene=SP10:05bd&lang=pt-BR" prefix 3 150 [skipFrames]`; it waits until the subtitle is drawn and logs the layout);
strip geometry / talking portraits: `engine/examples/strip_probe` (`strip_probe SPnn:pc frames [pngdir]`, `strip_probe survey frames roots...`, `strip_probe mouth root frames from to`);
native: `VSUB=1 I18N_PACK=i18n/build/pt-BR.pack PROBE_FRACS=0.2,0.5,0.8 voice_probe <outdir> <frames> flags=ones SPnn:pc[:menu path[:scene]]` (screenshots at fractions of every clip).
An English subtitle mode (reading the gitignored transcripts at build time, for local use only) is possible with the same pack format but is not built.

## 6. Clips without a subtitle

Never started by a script (16 ids), non-speech clips (above), the GAME OVER / quit voice (id 48: Whisper hears only "Thank you." in the first 0.6 s of 6.6 s), clips whose words stay uncertain after two passes
(id 293, listed in `overrides.json`), the SP01 debug menu clips. Screens without a dialogue strip (about 3 % of the minutes) get the compact band; none of the Act 1 clips with speech is one of them.
The survey for the speaker script: `strip_probe survey 3000 $(awk -F, 'NR>1{sub("L_","",$2);print $1":"$2}' work/voice/voice-scenes.csv) > work/voice/strip-survey.txt` (about 15 minutes for all acts).

## 7. Next: Acts 2 and 3

Transcripts exist for all clips (`work/voice/script/SPnn.txt` has the context); what is left is translation (Act 2: 397 speech clips, 25.6 min; Act 3: 409 clips, 34.3 min), `snt voice-check`, one review pass per act, and a play-test.
Clips shared with Act 1 are already done. The overrides file must be kept in `work/` (copy it to the next machine with the transcripts, or run the transcription again: about 7 minutes per pass on the RTX 3080).

## 8. Act 1 review (2026-10-16)

One review subagent (sonnet, high effort; read-only; prompt kept in `work/voice/review/`) read all 291 subtitles against the transcripts, the script context, the glossary and four engine screenshots. Score **4/5**: faithful, natural, consistent terminology,
conservative and correct speaker names. Findings applied (about 45 clips): awkward page breaks (fixed in the importer too: page ends are weighted by `break_severity` and a page may take a 4th line to avoid a mid-phrase break), register slips of informal speakers
(Katrina "Não se preocupa", Gillian "Guarda o fôlego", the vendor "tira / estraga": colloquial imperatives as in the glossary), dense clips shortened, two terms ("Equipe de Limpeza Junker", "relacionamento"), capitalisation, four speaker names added where the
conversation makes them certain (clips 132, 138, 169, 209). Not applied: naming the speaker of clip 88 (the Chief is likely but not certain), clip 293 stays without a subtitle (the words are unclear; someone should listen to it).
