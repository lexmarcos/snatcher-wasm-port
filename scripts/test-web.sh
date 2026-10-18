#!/usr/bin/env bash
# Web front end checks: unit tests (pacer, picked-folder recognition, gamepad families/maps/prompts, cue/bin picking), then headless Chrome runs against a dev
# server (started here on PORT, default 5301, unless one is already listening there). The disc importer run (test/import.ts, own server on 5320 with /assets off,
# about 8 minutes) only happens when the local disc work/snatcher.cue exists; SKIP_IMPORT=1 skips it.
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
bun test/shot.ts newgame "$out/shot-newgame.png" 180
bun test/shot.ts "scene=SP01:0" "$out/shot-sp01.png" 200
if [ -z "${SKIP_IMPORT:-}" ] && [ -f ../work/snatcher.cue ]; then
  bun test/import.ts "$out/import"
fi
