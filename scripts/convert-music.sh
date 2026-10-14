#!/usr/bin/env bash
# Convert extracted CD-DA WAV tracks to Opus for the web front end.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p assets/web/music
for f in assets/raw/audio/track*.wav; do
  b=$(basename "$f" .wav)
  [ -f "assets/web/music/$b.ogg" ] || ffmpeg -loglevel error -y -i "$f" -c:a libopus -b:a 128k "assets/web/music/$b.ogg"
done
