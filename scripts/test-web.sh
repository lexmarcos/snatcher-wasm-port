#!/usr/bin/env bash
# Web front end checks: unit tests (pacer, picked-folder recognition, gamepad families/maps/prompts, cue/bin picking), then headless Chrome runs against a dev
# server (started here on PORT, default 5301, unless one is already listening there). The disc importer run (test/import.ts, own server on 5320 with /assets off,
# about 8 minutes) only happens when the local disc work/snatcher.cue exists; SKIP_IMPORT=1 skips it.
# The autosave run (test/autosave.ts) plays a new game in real time until the first safe point, about a minute in all.
# The touch run (test/touch.ts) needs assets/raw (the dev server serves it) and skips cleanly without it.
# The display filter run (test/crt.ts, own server on 5374, software GL, about 10 minutes) checks the picture styles; its part B needs assets/raw and skips cleanly without it.
# usage: scripts/test-web.sh [outdir-for-screenshots]
set -euo pipefail
cd "$(dirname "$0")/../web"
out="${1:-/tmp}"
export PORT="${PORT:-5301}"
bun test test/
if ! (exec 3<>/dev/tcp/127.0.0.1/"$PORT") 2>/dev/null; then
  bun server.ts >/dev/null 2>&1 & srv=$!
  trap 'kill $srv 2>/dev/null || true' EXIT
  sleep 1
fi
bun test/ui.ts "$out"
bun test/lang.ts "$out"
bun test/pads.ts "$out"
bun test/design.ts "$out/design"   # design system screens and layout checks (own servers on 5330/5331)
bun test/autosave.ts "$out/autosave"   # autosave: first save, Continue, toggle, delete, Clear everything, bad autosaves, blocked storage (own server on 5340, about 1 minute)
bun test/touch.ts "$out/mobile"   # phones and tablets: device emulation, real multi-touch, both touch modes, layouts, settings (own server on 5360, about 3 minutes)
bun test/crt.ts "$out/crt"   # picture styles / CRT filter: shaders, settings, geometry, fallbacks; with the game: frame untouched, brightness, light gun and touch with the filter on
bun test/shot.ts newgame "$out/shot-newgame.png" 180
bun test/shot.ts "scene=SP01:0" "$out/shot-sp01.png" 200
if [ -z "${SKIP_IMPORT:-}" ] && [ -f ../work/snatcher.cue ]; then
  bun test/import.ts "$out/import"
fi
