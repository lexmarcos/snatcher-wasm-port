#!/usr/bin/env bash
# usage: runshot.sh <name> <frames> <shotat> <inputs> <jump> [crop x0,y0,x1,y1] [scale]   -> work/i18n/gfx/shots/<name>_{en,pt}.png and cmp_<name>.png
# env passes through (SAVES=...). Needs target/release/examples/run and i18n/build/pt-BR.pack.
set -e
cd "$(dirname "$0")/../.."
name=$1; frames=$2; shotat=$3; inputs=$4; jump=$5; crop=${6:--}; scale=${7:-3}
out=work/i18n/gfx/tmp_$name; rm -rf $out; mkdir -p $out/en $out/pt work/i18n/gfx/shots
SHOTAT=$shotat ./target/release/examples/run $frames 100000 $out/en "$inputs" $jump >/dev/null 2>&1
I18N_PACK=i18n/build/pt-BR.pack SHOTAT=$shotat ./target/release/examples/run $frames 100000 $out/pt "$inputs" $jump >/dev/null 2>&1
for f in $(ls $out/en | grep f0); do
  python3 scripts/gfx_i18n/shots.py $out/en/$f $out/pt/$f work/i18n/gfx/shots/cmp_${name}_${f%.png}.png $crop $scale
done
ls work/i18n/gfx/shots | grep "cmp_$name"
