# Snatcher Web Port

[Leia em português](README.pt-BR.md)

Play **Snatcher** (the 1994 Sega CD version, USA) in your web browser, using your own copy of the game.

This is not an emulator. The game's engine was rebuilt from scratch so that it runs natively in the browser. It reads
the original disc and plays it as the console did, with the same pictures, music and voices.

**No game files are included here.** You need your own disc image of Snatcher (USA, Sega CD).

## What you get

* The whole game, from the title screen to the ending
* English (the original text) or Brazilian Portuguese, switchable at any time
* Portuguese subtitles for the voiced scenes and the spoken parts of the CD music
* Keyboard, gamepad (Xbox, PlayStation, Switch) and mouse for the shooting scenes
* Autosave, so you can pick up where you left off (you can turn it off)
* Your files never leave your computer: everything happens inside the browser

## How to play

1. Open the page (see "Running the page" below).
2. Press **Import disc image** and choose your `.cue` file **together with** its `.bin` file(s). You can also drag them onto the page.
3. Wait a few seconds while the disc is checked and copied into the browser.
4. Press **Play**.

On your next visits the game is already there: just press **Play**, or **Continue where you left off**.

The copy kept in the browser takes about 1.1 GB. You can remove it at any time from the start page
(**Remove the files and clear the site**).

**Have a `.chd` file?** It is not supported yet. Convert it first with
`chdman extractcd -i "Snatcher (USA).chd" -o snatcher.cue -ob snatcher.bin`.

## Controls

| Keyboard | Action |
|---|---|
| Arrow keys | Move / choose |
| Z, X, C (or A, S, D) | Buttons A, B, C |
| Enter | Start |
| Esc | Menu and pause (sound, screen, controls, saves) |
| F | Full screen |
| M | Mute |

In the shooting scenes, aim with the mouse and click to fire. Gamepads work as soon as you press a button,
and their buttons can be changed in the menu.

## Running the page

There is no public website yet, so for now the page runs on your own computer.
You need [Rust](https://rustup.rs) and [Bun](https://bun.sh). Then, in a terminal, inside this folder:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129
scripts/build-web.sh
bun web/server.ts
```

Open http://localhost:5300 in your browser (tested in Chrome).

## For developers

Technical notes, the reverse engineering of the original game and the test tools are in [`docs/`](docs/).
Good places to start: [`docs/PROGRESS.md`](docs/PROGRESS.md) and [`docs/re/web.md`](docs/re/web.md).

## Legal

This is an independent fan project. It is not affiliated with or endorsed by Konami or Sega.
Snatcher is © Konami. Sega and Sega CD are trademarks of Sega. To play you need your own copy of the game.

The project's own code and text are under the MIT licence ([`LICENSE`](LICENSE)); it gives no rights to the game itself.
The DotGothic16 font in `web/fonts/` is under the SIL Open Font License ([`web/fonts/OFL.txt`](web/fonts/OFL.txt)).
