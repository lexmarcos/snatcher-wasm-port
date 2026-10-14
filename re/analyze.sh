#!/usr/bin/env bash
# Usage: re/analyze.sh <name> <image> <base-hex> <labels-file> <memmap-file>
# Imports a raw 68000 image into re/ghidra/<name>.gpr (fresh each run), applies labels,
# runs auto-analysis and dumps functions/decompilation/listing to re/out/<name>/.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
name=$1; img=$2; base=$3; labels=$4; memmap=$5
mkdir -p "$here/ghidra" "$here/out/$name"
rm -rf "$here/ghidra/$name.gpr" "$here/ghidra/$name.rep"
"$GHIDRA_INSTALL_DIR/support/analyzeHeadless" "$here/ghidra" "$name" \
  -import "$img" -processor 68000:BE:32:default -loader BinaryLoader -loader-baseAddr "$base" \
  -scriptPath "$here/scripts" \
  -preScript SncSetup.java "$labels" "$memmap" \
  -postScript SncDump.java "$here/out/$name" "$labels" \
  > "$here/out/$name/headless.log" 2>&1
grep -E "ERROR|label |block " "$here/out/$name/headless.log" | head -20 || true
wc -l "$here/out/$name"/*.txt "$here/out/$name"/decomp.c
