# Voice subtitles (pt-BR)

The original game never shows text with a voice clip (docs/voice-inventory.md: the message box stays dark and empty while a clip plays; the script waits on it with `CMD0A`).
A pt-BR player would hear 83 minutes of English with nothing to read, so the port can draw a **subtitle in the empty message box** while a clip plays. This file describes the
whole chain: transcription, subtitle data, validator, engine, front end, status per act. **English transcripts are game content and are never committed**: they live in `work/` (gitignored).

## Status

| act | clips with a subtitle | transcribed | translated | reviewed |
|---|---:|---|---|---|
| all (1146 clips started by scripts, 82.8 min) | | 1146 (Whisper large-v3, 2 passes) | | |
| Act 1 (SP06-SP19) | 291 | yes | yes | yes, 4/5 (review below) |
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

Authoring: write `id<TAB>speaker<TAB>~text` lines (speaker number, name or 0; `~` = wrap and paginate: pages of 3 lines (4 when that avoids a break inside a phrase) whose ends are chosen by sentence / clause
boundaries, `{page}` written by hand are kept, `|` forces a line break) and run `snt voice-import i18n/pt-BR assets/raw/files patch.tsv` (fills `h` and `d`).
`snt i18n-build` puts the subtitles in the pack (a voice section after the names; old packs without it still load).

## 3. Validator

`snt voice-check i18n/pt-BR assets/raw/files` (also run by `snt i18n-check i18n/pt-BR assets/raw/files all`):
the clip exists and its duration matches; the English hash matches `work/voice/transcripts/en.tsv` (skipped with a warning when the file is absent) and the clip is not flagged non-speech;
markup and characters; **line width** (limit 208 px: x 24..232, like a message), **lines per page** (5 with a name, 6 without; a warning above 4; the pagination default is 3);
**reading speed**: the pages share the display time in proportion to their text, a clip may stay up for a hold after its end (see below), and the text must be readable at
**17 characters per second** over clip + hold (`READ_CPS`); a page must also hold that speed on its own.

## 4. Engine (`engine/src/vsub.rs`)

* **Overlay on the finished frame**: no game state, script flow or timing is touched (tests compare `debug_info` frame by frame); with the option off, in English, or without a
  subtitle for the clip nothing is drawn, so the output is byte-identical. The text is rendered with the game's glyph renderer in the pt-BR 10 px layout into a private 256x64 bitmap,
  at the positions of a message (name line at x 16, text at x 24; with a name 5 lines, without 6), and blitted on the dialogue strip (screen rows 150..214) over a black backing
  (rows 144..224, hiding the portrait icons the way a real message does). The palette is the message window's. If the picture reaches the bottom of the screen (no strip,
  SP09 GAME OVER style screens) the text goes on a compact black band at the bottom edge instead.
* **Start/stop**: sequencer command 0x0D with an id >= 36 starts the subtitle (`Out::Voice` is emitted at the same place); it ends when the clip is over plus the hold, when the
  script starts another clip (replaced), draws a message, opens a menu, or runs a command that changes the picture (fade, clear, scene, load).
* **Timing**: display time = `voice_left` (clip length + 6 frames, the engine's voice model) + hold; pages split it in proportion to their characters (`voice_page_starts`).
  The hold gives fast clips the reading time they lack (`voice_display_frames`: `ceil(chars / 17 * 60) - clip` frames, at least 15 so the backing does not flicker between two clips, at most 90).
* **API**: wasm `set_voice_subtitles(bool)`, `voice_subtitle_count()`, `voice_subtitle_active()` (-1 or the clip id); native `Engine.vsub.on`. Only has an effect when the language is pt-BR and the pack has a voice section.
* Tests: `engine/tests/vsub.rs` (English byte-identical with subtitles loaded; pt-BR with the option off unchanged; subtitle on during the clip, gone after, pages change, game state identical),
  `core/src/i18n.rs` (pack round trip, timing).

## 5. Front end

Menu panel (Esc) > Display > **Legendas de voz / Voice subtitles** (`web/index.html` `#vsub`): on by default in pt-BR, off (and disabled) in English; an explicit choice is stored in
`localStorage` `snatcher.vsub` (`1`/`0`, every access in try/catch); `?vsub=0|1` overrides it for the visit. Screenshot check: `web/test/vsub.ts` (`PORT=5300 bun test/vsub.ts "scene=SP10:05bd&lang=pt-BR" prefix 3 150`);
native: `VSUB=1 I18N_PACK=i18n/build/pt-BR.pack PROBE_FRACS=0.2,0.5,0.8 voice_probe <outdir> <frames> flags=ones SPnn:pc[:menu path[:scene]]` (screenshots at fractions of every clip).
An English subtitle mode (reading the gitignored transcripts at build time, for local use only) is possible with the same pack format but is not built.

## 6. Clips without a subtitle

Never started by a script (16 ids), non-speech clips (above), the GAME OVER / quit voice (id 48: Whisper hears only "Thank you." in the first 0.6 s of 6.6 s), clips whose words stay uncertain after two passes
(id 293, listed in `overrides.json`), the SP01 debug menu clips. Screens without a dialogue strip (about 3 % of the minutes) get the compact band; none of the Act 1 clips with speech is one of them.

## 7. Next: Acts 2 and 3

Transcripts exist for all clips (`work/voice/script/SPnn.txt` has the context); what is left is translation (Act 2: 397 speech clips, 25.6 min; Act 3: 409 clips, 34.3 min), `snt voice-check`, one review pass per act, and a play-test.
Clips shared with Act 1 are already done. The overrides file must be kept in `work/` (copy it to the next machine with the transcripts, or run the transcription again: about 7 minutes per pass on the RTX 3080).

## 8. Act 1 review (2026-10-16)

One review subagent (sonnet, high effort; read-only; prompt kept in `work/voice/review/`) read all 291 subtitles against the transcripts, the script context, the glossary and four engine screenshots. Score **4/5**: faithful, natural, consistent terminology,
conservative and correct speaker names. Findings applied (about 45 clips): awkward page breaks (fixed in the importer too: page ends are weighted by `break_severity` and a page may take a 4th line to avoid a mid-phrase break), register slips of informal speakers
(Katrina "Não se preocupa", Gillian "Guarda o fôlego", the vendor "tira / estraga": colloquial imperatives as in the glossary), dense clips shortened, two terms ("Equipe de Limpeza Junker", "relacionamento"), capitalisation, four speaker names added where the
conversation makes them certain (clips 132, 138, 169, 209). Not applied: naming the speaker of clip 88 (the Chief is likely but not certain), clip 293 stays without a subtitle (the words are unclear; someone should listen to it).
