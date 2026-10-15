#!/usr/bin/env bash
# Web front end checks: unit tests (pacer, picked-folder recognition, gamepad families/maps/prompts), then headless Chrome runs against a dev
# server (started here on PORT, default 5301, unless one is already listening there).
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
bun test/pads.ts "$out"
bun test/shot.ts newgame "$out/shot-newgame.png" 180
bun test/shot.ts "scene=SP01:0" "$out/shot-sp01.png" 200
