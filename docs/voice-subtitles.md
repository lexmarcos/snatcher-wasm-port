# Voice subtitles (pt-BR)

The original game never shows text with a voice clip (docs/voice-inventory.md: the message box stays dark and empty while a clip plays; the script waits on it with `CMD0A`).
A pt-BR player would hear 83 minutes of English with nothing to read, so the port can draw a **subtitle in the empty message box** while a clip plays. This file describes the
whole chain: transcription, subtitle data, validator, engine, front end, status per act. **English transcripts are game content and are never committed**: they live in `work/` (gitignored).

## Status

| act | clips with a subtitle | transcribed | translated | reviewed |
|---|---:|---|---|---|
| all (1146 clips started by scripts, 82.8 min) | | 1146 (Whisper large-v3, 2 passes) | | |
| Act 1 (SP06-SP19) | 291 | yes | yes | yes, 4/5 (review below); layouts by portrait: see section 4 |
| Act 2 (SP20-SP35) | 395 (397 clip ids with the 2 shared with Act 1) | yes | yes | yes, 4/5 (section 11) |
| Act 3 (SP36-SP38) | 409 (3 of them, 945 / 951 / 952, are also started by the SP01 debug menu) | yes | yes | yes, 4/5 (section 12) |

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
  script starts another clip (replaced), draws a message or opens a menu. A command that changes the picture (fade, clear, scene, load, inset picture 0x15, shooting) ends it **only once the clip is over** (the reading hold):
  while the clip still plays the player is listening to it, so the subtitle stays and the layout follows the strip (section 9).
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

## 7. Next: play-test (Acts 1-3 are done, sections 11 and 12)

Transcripts exist for all clips (`work/voice/script/SPnn.txt` has the context). All three acts are translated, checked (`snt voice-check`), swept and reviewed; what is left is a play-test of the timing in the browser and listening to the flagged clips (sections 11 and 12).
Clips shared between acts are subtitled once. The overrides file must be kept in `work/` (copy it to the next machine with the transcripts, or run the transcription again: about 7 minutes per pass on the RTX 3080).

## 8. Act 1 review (2026-10-16)

One review subagent (sonnet, high effort; read-only; prompt kept in `work/voice/review/`) read all 291 subtitles against the transcripts, the script context, the glossary and four engine screenshots. Score **4/5**: faithful, natural, consistent terminology,
conservative and correct speaker names. Findings applied (about 45 clips): awkward page breaks (fixed in the importer too: page ends are weighted by `break_severity` and a page may take a 4th line to avoid a mid-phrase break), register slips of informal speakers
(Katrina "Não se preocupa", Gillian "Guarda o fôlego", the vendor "tira / estraga": colloquial imperatives as in the glossary), dense clips shortened, two terms ("Equipe de Limpeza Junker", "relacionamento"), capitalisation, four speaker names added where the
conversation makes them certain (clips 132, 138, 169, 209). Not applied: naming the speaker of clip 88 (the Chief is likely but not certain), clip 293 stays without a subtitle (the words are unclear; someone should listen to it).

## 9. Sweep of Act 1 and the inset-picture bug (2026-10-16)

**Bug** (found while playing): in the Chief's office, after all the topics were asked, the Chief gives Seed the Junker ID card (SP06:0b6e, clips 95..102). The script is `CMD0D 96`, `sys.Q15_OR_ANIM` (command 0x15: the card
as an inset picture at the top left of the scene), `CMD0A`. The overlay ended the subtitle on every picture command, including 0x15, so clip 96 (8 s, only the Chief's portrait, in the right slot)
ran with an empty strip and the next clip (97, Gillian's portrait arrives) was the first one with text. It was not the portraits: the "wait for the portraits" rule and the right-only lane were fine
(clip 95 before it and 97 after it draw as usual); the subtitle was taken away a few frames after it was created and never drawn.
**Fix** (`vsub::vsub_on_command`): a picture command (0x02 fade, 0x0F clear, 0x11 load, 0x12 / 0x20 scene, 0x15 inset, 0x18 / 0x1E / 0x1F shooting) ends the subtitle only when the clip is over (`t >= play`, i.e. during the
reading hold); while the clip plays only a message (0x08, 0x16), a menu, another clip or the end of the time end it. `VoiceSubs::ended_by` records why the last subtitle ended (the command, `END_TIME`, `END_SCREEN`, `END_REPLACED`).

**Sweep** (`engine/examples/vsub_sweep.rs`, the same code runs in `engine/tests/vsub.rs` over `engine/tests/vsub_roots.txt`): plays script entry points (`SPnn:pc[:menu path[:scene]]`, story flags `FLAGS=zero|ones|alt|randN`) with the real pack and
checks for every clip that starts: (a) the subtitle is drawn at most `MAX_LAG` = 24 frames after the clip starts (18 = `SETTLE` is normal; `LATE` / `MISSING` otherwise), (b) what is drawn is the text of the clip that started last (`WRONG`),
(c) nothing is drawn after clip + hold (`STALE`) or while a message box is open (`MSG`), (d) a subtitle is never taken away while its clip plays and no message, menu or other clip came (`EARLY`, with the command). `CUT` = the script ended the clip
itself within the first frames (not a violation). Usage: `vsub_sweep i18n/build/pt-BR.pack 40000 SP06:0b6e::0a00 [more roots | roots.txt]`, `ALL=1` lists the OK clips, `ONLY=<id>`, `PNG=<dir>` saves a frame of each violation.
Roots: every Act 1 clip start (`CMD0D` in SP06..SP19) gives the enclosing sequence of the script (from `snt script-disasm`: the nearest line with a smaller indent), with the scene of `work/voice/voice-scenes.csv`, plus the roots of `work/voice/probes.txt`.
**Static check** (all 292 Act 1 clips, no run needed): between a `CMD0D` and its `CMD0A` the Act 1 scripts only contain WAIT, ANIM, ANIMWAIT, MUSIC and, in clip 96 only, `sys.Q15_OR_ANIM`; so clip 96 was the only clip a picture command could cut.
Clips without an entry that Act 1 can start: all are non-speech (empty output, vocalisations 145 / 164 / 166 / 167 / 287, effects) or the known ones: 48 (game over voice), 165 (hallucination), 293 (unclear, no subtitle), and **300** (SP12:00cd, 6.1 s,
Whisper: "And that's it." with a log-probability of -1.0 for the first 0.6 s, second pass "HUH? PFFT!": it sounds like a vocalisation after "Excuse me." at the door of Outer Heaven; someone should listen to it).

## 10. CD-DA speech (2026-10-16)

The PCM clips are not the only speech of the game: the prologue (tracks 3, 4, 5 started by DATA_D1 / DATA_D0), and the cut scenes that start CD-DA tracks (docs/re/audio.md section 2) may carry spoken narration on the disc audio. These get subtitles keyed by **CD position**, not by clip.

**Which tracks have speech**: `python3 scripts/cdda-transcribe.py --prepare` (16 kHz mono WAV of tracks 2..21 in `work/cdda/wav`), then `python3 scripts/cdda-transcribe.py` runs Whisper large-v3 (LAN server, `language=en`, segment + word timestamps, prompt with the game's names) over chunks of 150 s with 10 s overlap, keeps absolute times,
filters what Whisper invents on music (`classify`: no-speech probability, log-probability, compression ratio, "Thank you." and friends) and writes `work/cdda/transcripts/{en.json, en.tsv, summary.txt}` (gitignored: English transcripts are game content). A track with less than 4 s of surviving speech is flagged `music_only`.
`scripts/cdda-heuristic.py` is a server-free pre-selection from the energy envelope (speech pauses between words, music does not; calibrated on PCM voice clips): see its header for the numbers.

**Data**: `i18n/pt-BR/cdda.json` (committed; pt-BR only): `{"tracks": {"3": {"<start ms>": {"e": "<end ms>", "h": "<hash of the English segment>", "s": "<speaker>", "x": "<pt-BR text>"}}}}`, keyed by track and segment start. Written by `snt cdda-import <lang-dir> <files-dir> patch.tsv`
(`track<TAB>start_ms<TAB>speaker<TAB>text`; end and hash come from `en.tsv`), validated by `snt cdda-check` (part of `voice-check` and `i18n-check ... all`): the track exists, times inside the track, the English hash and speech flag, characters, **every layout** (widths, lines per page) and the **reading speed** (17 chars/s over the speech plus the hold, the next subtitle of the track limits the hold: `sncore::i18n::cdda_display_frames`).
The pack carries it as a trailing section (`Translation::cdda`).

**Engine** (`engine/src/vsub.rs`, `cdsub_select` / `cdsub_now`): the subtitle is a function of the **CD clock** = frames since the engine started the track (`Engine::cd_start`, set by `cd_play`, the same clock `$787A` / the scene timelines use; sector counts of the tracks in `sncore::audio::CD_TRACK_SECTORS`). Every frame the subtitle whose start has passed is looked up; it shows until its display time is over
(speech + hold, never into the next one), pages share the time in proportion to their text, the layout is the one of voice clips (box on a black screen, compact band where the picture reaches the bottom, as on the prologue's DNA screens). It is hidden while a message or a menu is open and comes back with them closed; a voice clip's own subtitle has priority; commands (fades, scene loads) never end it. Same option ("Legendas de voz"), pt-BR only,
English byte-identical, no change to game state or timing (`engine/tests/cdsub.rs`, synthetic subtitles: game state identical frame by frame, every changed pixel inside the reported rectangle, times of appearance / disappearance).

**Web audio and the CD clock** (`web/src/audio.ts`, `web/src/cdclock.ts`): the engine's clock is frames at 60 Hz (the pacer), the audio runs on the audio hardware clock; they were never tied before. Now (1) a track that starts before its file is decoded starts at the position the engine is already at (the audio clock stands still while suspended, like the engine), and (2) every 0.5 s `syncCd(game.cd_position())`
nudges the track's `playbackRate` towards the engine clock (dead band 15 ms, at most +-0.3 %, inaudible). Without it a 59.94 Hz display makes the engine 0.1 % slower than the audio: 0.4 s over a 7 minute track. Checks: `bun test web/test/cdclock.test.ts` (simulated displays of 59.94 / 60 / 60.06 / 75 / 144 Hz over 420 s: drift 0.4 s without the correction, under 50 ms with it) and
`PORT=5300 bun web/test/cdsync.ts 60` (headless Chrome, real time, prologue track 3: engine position vs audio position every 5 s: within 35 ms).

**Transcription (2026-10-17)**: the Whisper server stayed off, so the transcription used **ElevenLabs Scribe v2** (CLI `elevenlabs speech-to-text convert --model-id scribe_v2 --language-code en --timestamps-granularity word --tag-audio-events true --diarize true`; the user approved sending the CD-DA audio there for this, nothing else, nothing published) through `python3 scripts/cdda-transcribe.py --backend eleven` (whole tracks, no chunking, resumable `work/cdda/transcripts/eleven/trackNN.json`, merged with the same `en.json` / `en.tsv` / `summary.txt`).
Words become segments at sentence ends followed by a pause, pauses >= 0.8 s, speaker changes and audio events; a sentence cut in two is joined again; Scribe lets a word swallow the silence before it (track 3's first word "started" 34 s early), so the start of an over-long word is clamped. All 20 tracks (45.2 min) cost about 850 credits of the plan.
**Where the speech is** (answer to "does the prologue have narration"): **yes, track 3 is the prologue narration**: the narrator reads the Chernoton / Lucifer Alpha / Snatchers text from 35 s to 126 s of the track (15 segments, 75 s). My first server-free heuristic (`scripts/cdda-heuristic.py`) was wrong about it (a narrator over music has shallow envelope dips); it is kept only as a record. Track 4 (the rest of the prologue, started by DATA_D0 after track 5) is music until 157 s and then the Gillian / Jamie farewell and the dossier narration (24 segments). Tracks 2 (4.6 s), 5 (19 s) and 16 (15 s) are music / effects only.
The other tracks are recap / dialogue scenes: 6 (SP08, Chief's office), 7, 8 (Metal Gear's recap), 9, 10, 11 (SP20..SP29), 12 (Harry's death), 13, 14, 15 (SP33), 17, 18 (SP38), 19, 20 (SP06), 21 (SP31). 17 tracks with speech, 560 cues, 26 min of speech; `i18n/pt-BR/cdda.json`.
**Translation**: segment by segment (`work/cdda/pt/tNN.txt`, one line per speech segment, `-` = skipped: a 0.1 s mic test and a 0.1 s name), speakers not named (the diarization ids are not names), glossary terms; 45 cues were condensed to meet the 17 chars/s rule (`cdda-check` reports the room each cue has). Screenshots of real moments (prologue, track 3 at 36 / 66 / 100 s and track 4 at 160 / 232 s): `work/voice/shots/cdda/` (not committed): the subtitle follows the pictures (the date line while the Moscow caption is up, "the greatest biohazard" on the DNA screen).
**Review (2026-10-17)**: one review subagent (sonnet, high effort, read-only) read all 560 cues against the transcripts, the glossary and the screenshots: score **4/5** (about 4.5 after the fixes). Applied: a mis-heard villain name inherited from the transcript ("Jin Sho" -> Chin Shu Oh, track 21), a number ("450" -> "150 metros", the countdown 1500 / 800 / 300 / 150 / 70), a gender slip ("a única" -> "o único"), old spelling ("pêlo"), "caça-recompensas legal" -> "legalizado", omissions ("por favor", "tratamento"), and the register pass: Gillian, Random and Harry use colloquial imperatives ("aperta", "pula", "corre", "sai", "fica") as the glossary demands, Metal Gear stays formal; a 0.2 s laugh ("Ha.") got no subtitle.
Not applied: "Maintenance to all pilot" (track 4) may be a speech-to-text error, to be checked by ear (**"Thank you, Metal" on track 9 was checked by ear by the user (2026-10-17) and is correct: it is no longer an open item**); the diarization speaker ids are not used (no speaker names are shown); a few cues the reviewer wanted longer do not fit the 17 chars/s rule in the room they have (`snt cdda-check` is the judge).

## 11. Act 2 (2026-10-17)

**Clip set**: the speech clips (`work/voice/transcripts/en.tsv`, speech = 1) that only SP20..SP35 start (`work/voice/clips.csv`, column `files`), not yet in `voice.json`, not started by Act 1 or Act 3 scripts: **395 clips** (ids 58, 371-760 with gaps, 1192, 1194-1197; the list is `work/voice/patches/act2.tsv`). Two more Act 2 clips are shared with Act 1 (already done). Non-speech clips of the Act 2 scripts get no subtitle (sneezes 419 / 420, groans 468 / 484 / 485 / 663 / 714, effects 661 / 662 and the others in `overrides.json`). Clip 1193 (the JT&T recording, Act 1 entry) got the same company spelling as 1192.
**Workflow**: same as Act 1: patch TSV (`work/voice/patches/act2.tsv`, not committed) -> `snt voice-import` -> `snt voice-check` (15 reading-speed failures in the first draft, all fixed by shortening) -> review -> fixes. Context for the translator: `work/voice/script/SPnn.txt` (message lines around the clips tell who talks).
**Speakers**: first from context, then checked with the talking-portrait survey (`strip_probe survey`, `scripts/voice-speakers.py`; the mouth of the speaking portrait moves). The survey corrected 12 clips (570, 572, 575, 576, 591, 592, 597, 601, 602, 635, 636, 639 and the MG/Gillian pairs 427 / 429 / 457 / 458) and left the unclear ones **without a name**: 379, 380, 403, 405, 431-435, 438, 440, 442, 444, 446, 449, 451 (the fortune teller and the ramen vendor have no name entry), 583, 585, 604-607, 637, 647, 657, 659, 660, 759. Portraits that are not people (the reconstructed face of the Chief in SP31) also animate, so a "talking" mouth is only evidence when the portrait of the labelled speaker is on screen.
`strip_probe` roots now accept a menu path like the sweep (`SPnn:pc:path:scene`); the portrait survey needs `strip_probe survey 5000 roots...`.
**Sweep** (`vsub_sweep`, roots `work/voice/act2_roots.txt`, flags zero / ones / alt, 3500-5000 frames each, plus the same roots without scene): **348 of the 395 clips were played and every one drew its subtitle in time (lag 18 frames, `OK`), none was `LATE`, `MISSING`, `STALE`, `EARLY`, `MSG` or `COVER`**; 399 and 430 start correctly and only the CD-DA cue of the next scene is drawn after them (it shares the overlay, so the sweep calls it `WRONG`; same for the non-speech clips 37, 43, 50, 55 and 662 that are followed by CD-DA speech: not a voice-clip problem). Not reached by any root (they need menu paths the roots do not know): 58 (radio), 415, 416, 448-453, 502-513, 536-540, 557-559, 578-585, 761-769, 1197. **Static check** (`snt script-disasm` of SP20..SP35, all 470 `CMD0D`): between a `CMD0D` and its `CMD0A` there is only `ANIM`, `WAIT` or `MUSIC`, no picture command, so nothing can cut a clip (the Act 1 clip 96 case does not occur in Act 2); `voice-check` already wraps every one of the 395 clips in all four layouts within the cps limit. Layout mix seen: box (black screens such as the ramen stall), lane (one portrait), band (two or three), compact (the Turbocycle cockpit, SP29). Screenshots: `work/voice/shots/act2/` (not committed; `sheet.png`): no subtitle touches a portrait frame.
**Sweep tool fixes** (found now): `vsub_sweep::make` put the story flags in before `jump_path`, which starts a blank VM, so FLAGS never had an effect (Act 1 roots ran with zero flags): now applied after the jump; new violation `COVER` (the painted rectangle overlaps a portrait frame); a root now stops after 1500 idle frames instead of 900 (clips of 17-20 s). `engine/tests/vsub_roots.txt` has six Act 2 roots (flags zero / alt).
**Review (2026-10-17)**: one review subagent (sonnet, high effort, read-only) read all 395 cues against the transcripts, the script context and the glossary: score **4/5**. Applied (28 clips): register slips (Gillian "Tranca a porta", "A gente tá procurando", "achar eles", "recuperar ela"; Katrina "toma cuidado"; Random "tem" for "há"; Mika "ajude-o"), calques ("definitivamente" -> "com certeza / sem dúvida", "vou me cuidar", "compensa mais que o bastante"), the broken sentence of 682, "Equipe de Limpeza Junker" everywhere, the object of 456 (revistando o lugar), "Encare como um homem", Chief "peguei você", Jamie "liga pro", and the company name **JT&T** (the transcript says JTNT in two clips; the game text SP35 says JT&T). Not applied: naming 379 / 380 / 637 (low confidence), "Estamos" in 452.
**Transcripts that look misheard (flagged, to listen to)**: **293, 300, 332, 357** (from Act 1: 293 and 300 have no subtitle; 332 and 357 are labelled Gillian but Metal Gear's portrait talks), and from Act 2: **665** ("I'm alienized", Metal Gear after the blast; "alienizado" is a guess), **667** ("I'll b-b-b-to each circuit to bend-d-damage somewhat": translated as damage to each circuit), **568** (a sentence that stops at "the turbo cycle": probably "... era o Harry", the subtitle stops there too), **636 / 637** ("Shin Fui, uh, what's his face" and "Chinchoo. Oh, Gillian": the second is unnamed, it may be Metal Gear), **465** ("You out of here!", probably "Get out of here!" by Katrina), **545** ("Oxygen is owned", surely "Oxygen is O"), **682** ("the two blinders' old lines", read as the Tube-Liner lines), **716 / 726** ("Killian", read as Gillian), **508** (the transcript "Eggful Young, and Wantons" is the joke "Egg Foo Young and Wontons"). The stutter lines 665 / 667 are Metal Gear's glitch voice, so a human should confirm them.

## 12. Act 3 (2026-10-17)

**Clip set**: the speech clips (`en.tsv`, speech = 1) that SP36..SP38 start and `voice.json` did not have: **409 clips** (ids 770-1184; the list is `work/voice/patches/act3_[abcd].tsv`). Three of them (945, 951, 952) are also started by the SP01 debug menu, which is no reason to skip them. Non-speech clips of the Act 3 scripts get no subtitle (36, 40, 43, 63-66, 946, 949, 962, 1142, 1169 and the other effects / shouts in `overrides.json`). Wording follows the in-game text of SP36-SP38 (Zona Neutra da Sibéria, Rio Ina, SNOW-9, Tube-Liner, Cúpula de Kyoto, "Lúcifer-Alfa" as SP04 spells it, Chernoton, Projeto SNATCHER / Frankenstein / Bayreuth).
**Workflow**: patch TSV -> `snt voice-import` -> `snt voice-check` (0 reading-speed failures in the first draft, one in the very first batch) -> sweep -> review -> fixes.
**Speakers**: Gillian 88, Jamie 52, Metal Gear 51, Mika 12, Random 12 clips; **194 unnamed**: Elijah Modnar (the villain has no name entry, so all his monologue is unnamed), news reports (770, 771) and every clip where the portrait survey and the context do not agree. The survey (`strip_probe survey` over the label roots of SP36-SP38, `scripts/voice-speakers.py`; a mouth that moves in 4 of 5 samples talks, 2/5 is idle noise, Metal Gear's mouth often stays at 2-3/5 even when he talks) found one wrong label: **981** ("And that was the Snatcher Project...", first labelled Jamie, Gillian's mouth moves and Jamie answers "That's right, Gillian" in 982). Names given by context only (no survey): the snatch-process clips 898-911 (Metal Gear explains the pictures, Gillian asks / comments: 901, 904, 908, 910) because that menu path cannot be run by `strip_probe`; the portrait of the other party in a two-person exchange (Jamie / Gillian in SP38). Mika is named only where her portrait talks (777, 781, 803, 808, 815, 824, 838, 845, 852, 861, 864, 879).
`strip_probe` now reads `FLAGS=ones|alt|randN` like the sweep.
**Sweep** (`vsub_sweep`, label roots of `work/voice/voice-scenes.csv` for SP36..SP38 with flags zero / ones / alt, 6000 frames, plus `SP37:02a5:9c` (church door, `ones`), `SP37:0f55:e1` (quit menu: clip 948), `SP37:088d:c3,113` with `FLAGS=rand6` (the snatch process, clips 898-911 need the cylinder flag combination, found by trying seeds) and `SP38:0e26` with `FLAGS=rand1` (clip 1181 only plays with one flag combination)): **all 409 clips were reached and drew their subtitle in time (lag 18 frames, `OK`); no `LATE`, `MISSING`, `STALE`, `EARLY`, `MSG` or `COVER`**. Layout mix seen: box (black screens and empty strips: news, the ending, 1177-1184), lane (one portrait, e.g. Gillian alone at the start of SP38), band (two or three portraits: the SP36 planning scene, the Jamie / Elijah scenes), no compact layout. 879, 1179 and 1184 are followed by a CD-DA speech cue that shares the overlay: the sweep first called them `WRONG`; it now skips cues of the CD clock (`Engine::vsub_is_cd`), the same case as the Act 2 clips 399 / 430. Clips that the roots cannot reach: none. `CUT` entries are clips that start in the last frames of a 6000-frame run (829, 1043, 1116, 1130: reached by other roots). `engine/tests/vsub_roots.txt` now has 38 Act 3 roots (a greedy cover of the clips plus the four special roots); `cargo test --release -p snengine --test vsub` passes (9 tests, 156 s).
**Static check** (`snt script-disasm` of SP36, SP37, SP38, all 442 `CMD0D`): between a `CMD0D` and its `CMD0A` there is only `WAIT` / `ANIMWAIT` and two `SCENE` commands: clip 807 (`SCENE DATA_Y00 img 8`, the bunker photograph, two seconds into the clip) and the non-speech clip 64; 807 keeps its subtitle through the picture change (sweep `OK`), so the Act 1 clip 96 case does not occur. `voice-check` wraps every clip in all layouts within the cps limit.
**Review (2026-10-17)**: one review subagent (sonnet, high effort, read-only) read all 409 cues against the transcripts, the script context and the glossary: score **4/5**. Applied (22 clips): **"Lúcifer-Alfa"** (SP04 spelling; also in `cdda.json`, track 3 / 4 cues), unit name "17ª Força Especial de Investigação da Sibéria" in 806, Elijah's register (Reanimou-me, Solte-me, Não a deixem ir, raptá-lo, "é o ponto de mira"), Metal Gear without slang ("excelente Junker"), Gillian "a gente ainda tem" (924), calques ("para sempre ligado" for "permanent on", "aprovaram por unanimidade o uso", "instinto de voltar ao lar", "a maioria da 17ª", "ameaça", Rio Moscova, "Receio que ... seja"). Not applied: 961 vocative (the reviewer wanted "Gillian" and Jamie as speaker; the portrait of Gillian is the one that loads, so the clip is left as heard and unnamed).
**Transcripts that look misheard or doubtful (flagged, to listen to)**: **961** ("Jamie, I'm afraid that Harry's... There was nothing I could do. They forced me": two speakers in one clip, Gillian then Jamie, or a wrong vocative; unnamed), **1136** (Elijah "You insignificant mass of metal. You'll never..." and Metal Gear "One move and I detonate. Gillian, run!": two voices, unnamed), **1012** ("Random? No, not quite": Gillian then Elijah, unnamed), **1145** ("You always were a real pain in the butt": Elijah's mouth is idle, so it may be Random or Gillian; unnamed), **1054** ("Bayreuth Project": may be misheard, both passes agree), **1060** ("vaccine L-Angels": name uncertain), **806** ("17th Siberian" versus "Special" Investigative Force in 825 / 1002), **1041** ("No politician" read as "I'm no politician"), **1063** ("byroids" read as bioroids), **996** (Chernotan -> Chernoton), **1180-1183** (the farewell women of the ending, who is who depends on flags; unnamed, register neutral) and 898-911 (speakers by context). Screenshots: `work/voice/shots/act3/` (not committed; `sheet.png`): no subtitle touches a portrait frame.

