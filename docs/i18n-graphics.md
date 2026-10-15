# Text drawn in graphics (not translated yet)

Scope of the pt-BR work (docs/i18n.md): everything the script VM prints with the game font is translated. The items below are **pixels in picture
data** (tile sets inside `DATA_*.BIN` scenes or the IP.BIN boot code), not strings. They are left in English on purpose; redrawing them is a
separate art job. This file is the inventory so that job can start without searching again. Ids come from `docs/re/modules.md`,
`docs/re/oracle.md` and `docs/re/oracle2.md` (scenario names of the reference screenshots in brackets). Entries marked (to verify) were seen on
screen but their exact scene chunk was not looked up; use the asset viewer (`bun web/viewer/server.ts 5310`) to find the chunk.

## Boot, title and options (module code in IP.BIN, pictures in DATA_D2)
| Screen | File / module | Text in the picture |
|---|---|---|
| Copyright screen [`copyright`] | IP.BIN boot code (main CPU, `boot.rs`) | legal text: Konami, Sega license lines |
| Konami laser logo / logo [`konami_laser`, `konami_logo`] | IP.BIN boot | KONAMI wordmark (no translation needed) |
| Title picture [`title`] | DATA_D2 scene 0 | SNATCHER logo, copyright line, "PUSH START" / NEW GAME / CONTINUE (the two menu words are an animated object, object 17 of the module script) |
| Options screen [`options`] | DATA_D2 (`title.rs` settings state) | OPTIONS title, SOUND (STEREO / MONO), GUN (JUSTIFIER ...), BUTTON CONFIG A..F, pad button labels. Also lists the original button letters, see the controller section in docs/PROGRESS.md for the icon work |
| Slot select (CONTINUE) | DATA_D2 | SLOT 1..4 rows, EMPTY (to verify) |
| Disclaimer [`disclaimer`] | DATA_D2 / D1 (to verify) | the "this is a work of fiction" notice |

## Intro / prologue (DATA_D1, DATA_D0)
Scenes rendered with the prologue timeline (`timeline.rs`): "Moscow: June 6, 1996", the corridor, the DNA screens, the Neo Kobe Times front page,
the SNATCHER logo, JAPAN, "Dec. 2047 NEO KOBE CITY". All are pictures. The Neo Kobe Times page is a full newspaper with article text.

## Act cards and location captions (ACT 1 [`act1`], Omni Building [`omni_building`, `omni_caption`])
* The "ACT 1 / SNATCH" card and the next act cards are pictures (DATA file of the act intro; to verify).
* The typed location captions ("Konami Omni Building", "Junker Headquarters", ...) are drawn letter by letter in a serif face by the scene
  code; the letters come from a picture tile set, not from the message font, so they cannot be translated by the pack.

## In-game pictures with legible text (to verify one by one in the viewer)
* **SP09 file reading** (script label L_0ff4, silent replay L_10e9): Metal Gear reads an investigation file aloud on a framed screen; the pages are pictures drawn by the animation objects 0x14..0x1c of DATA_G0 (no script string), shown while the voice clips 221-235 (2.1 min) play. English only (docs/voice-inventory.md).
* GAME OVER picture and the videophone not-in-service / no-video banners (SP18, SP35): graphics that go with voice clips 48, 1193, 1191.
* GAME OVER picture (shown at state 4, then the game restarts at the title: `Engine::reset_to_title`).
* Staff roll of the ending (DATA_Y16): the credit lines are tile text (names, job titles in English/Japanese romanisation).
* Posters, signs and screens inside the room scenes (hospital neon "OLEEN", Konami posters, shop signs, newspaper headlines). They matter for
  puzzle text only where the dialogue reads them aloud ("Oleen Hospital", "Queens"); those readings are translated in the script.
* Jordan / patient-record screens use the message font, so they are translated.

## What stays English on purpose (not graphics, but listed here so nobody hunts for it)
* Typed-name and keyboard entry menus (SP05 name entry, Napoleon password words, SP09 birthmark answers, SP18 numbers): the engine compares
  typed or selected English text with the game's English answers, so those labels cannot be changed without changing logic.
* Letters of the on-screen keyboard and the word START printed by the controller icon patch.
* Intentionally untranslated strings: 8 in SP02 (song lyrics, a Japanese greeting, silence dots), the JUNKER/JORDAN acronym expansions, the
  "Face to face" memo, "liquid sky" / SNOW-9 / Junker's Eye (names), and the SP01 developer debug menu (see docs/i18n.md).

## How to redraw later
1. Locate the tile set with the asset viewer (palette + tile sheet of the scene chunk).
2. Redraw into the same tile budget (tiles are 8x8, 4 bpp, the scene palette applies) and re-encode with the LZKN1 tool (`snt lzkn-d` is the
   decoder; an encoder is needed or the blob can be stored uncompressed if the loader allows it).
3. Ship it as an overlay patch loaded only when the language is pt-BR, the same way the string pack works (engine side: a per-scene asset
   substitution table). Keep the English picture as default.
