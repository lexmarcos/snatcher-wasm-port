# Snatcher Web Port

[Leia em português](README.pt-BR.md)

Play **Snatcher** (the 1994 Sega CD version, USA) in your web browser, using your own copy of the game.

**Play in your browser: https://markzuel.com/snatcher-wasm-port/**

This is not an emulator. The game's engine was rebuilt from scratch so that it runs natively in the browser. It reads
the original disc and plays it as the console did, with the same pictures, music and voices.

**No game files are included here.** You need your own disc image of Snatcher (USA, Sega CD).

## What you get

* The whole game, from the title screen to the ending
* English (the original text) or Brazilian Portuguese, switchable at any time
* Portuguese subtitles for the voiced scenes and the spoken parts of the CD music
* Keyboard, gamepad (Xbox, PlayStation, Switch) and mouse for the shooting scenes
* Phones and tablets: an on-screen pad, or touch the picture itself
* Picture styles in the Display tab: plain pixels, or a tube TV or studio monitor look (scanlines, phosphor mask, glow, TV pixel shape) that keeps the colours and the brightness; it only changes how the picture is drawn
* Autosave, so you can pick up where you left off (you can turn it off)
* Your files never leave your computer: everything happens inside the browser

## How to play

1. Open https://markzuel.com/snatcher-wasm-port/ (or run the page yourself, see "For developers" below).
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

## On a phone or tablet

The game works on phones and tablets (Android, iPhone, iPad). You can play in two ways, and you pick one in the menu (the **Menu** button, then the **Touch** tab):

* **On-screen pad** (the default on a phone): a Sega CD pad is drawn on the screen, with a direction pad, the buttons A, B and C, Start and Menu. You can hold two buttons at once. In the shooting scenes the direction pad aims at the nine targets, A draws the gun and C fires.
* **Touch the game**: no pad is drawn. You touch the picture itself. Tap a command or a key to choose it, touch a message to go on (hold it to read faster), and tap a target in the shooting scenes to shoot there. The buttons **Back** and **Start** show up only when they are needed, and a tap with two fingers is also Back. In the dark room, drag your finger to move the light.

Tips:

* The picture is bigger with the phone turned sideways, but both ways work.
* On iPhone, Safari cannot show the page in full screen. Tap Share, then **Add to Home Screen**, and open Snatcher from its icon: it runs without the browser bars. On Android and iPad the **Fullscreen** button in the Touch tab works.
* To import your disc, tap **Import disc image** and pick the `.cue` file and its `.bin` file(s) together. On iPhone, tap **Select** first so you can choose more than one file.
* The copy kept in the browser takes about 1.1 GB. If your phone has little free space, the page tells you before you start (you can untick the CD music to save 490 MB).
* The screen stays on while you play. You can turn this off, and you can turn vibration on, in the Touch tab.

## For developers

The public website is built from this repository by a GitHub Actions workflow on every push to `main`.
To run the page on your own computer you need [Rust](https://rustup.rs) and [Bun](https://bun.sh). Then, in a terminal, inside this folder:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129
scripts/build-web.sh
bun web/server.ts
```

Open http://localhost:5300 in your browser (tested in Chrome). To build the same static folder that the website
uses (`build/site/`), run `scripts/build-static.sh`.

Technical notes, the reverse engineering of the original game and the test tools are in [`docs/`](docs/).
Good places to start: [`docs/PROGRESS.md`](docs/PROGRESS.md) and [`docs/re/web.md`](docs/re/web.md).

## Legal

This is an independent fan project. It is not affiliated with or endorsed by Konami or Sega.
Snatcher is © Konami. Sega and Sega CD are trademarks of Sega. To play you need your own copy of the game.

The project's own code and text are under the MIT licence ([`LICENSE`](LICENSE)); it gives no rights to the game itself.
The DotGothic16 font in `web/fonts/` is under the SIL Open Font License ([`web/fonts/OFL.txt`](web/fonts/OFL.txt)).
