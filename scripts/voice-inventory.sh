#!/bin/sh
# Rebuilds the voice inventory (docs/voice-inventory.md): static analysis of the scripts + engine probes of what the screen shows
# while each clip plays. Output in work/voice/ (gitignored). Needs assets/raw/files (extracted disc) and takes about 25 minutes
# on 12 cores because of the probe sweeps; set QUICK=1 to skip the sweeps and only rerun the static analysis with the old results.
set -e
cd "$(dirname "$0")/.."
OUT=work/voice
mkdir -p $OUT/probe_out
cargo build --release -p sntools -p snengine --example voice_probe
SNT=./target/release/snt
PROBE=./target/release/examples/voice_probe
$SNT voice-inventory assets/raw/files --probes $OUT/probes.txt >/dev/null
if [ -z "$QUICK" ]; then
  rm -rf $OUT/probe_out; mkdir -p $OUT/probe_out
  for fl in zero ones; do   # story flags all clear / all set
    xargs -P 11 -I{} sh -c "$PROBE $OUT/probe_$fl 3000 flags=$fl {} 2>&1 | cut -c1-300 >> $OUT/probe_out/$fl.\$\$.txt" < $OUT/probes.txt
  done
  cat $OUT/probe_out/*.txt | grep PROBE | sed 's/ png=.*//' > $OUT/probe_all.txt
  # roots whose clips no probe reached need particular variable values: pseudo-random flags
  $SNT voice-inventory assets/raw/files --csv $OUT/voice-triggers.csv --probe-results $OUT/probe_all.txt >/dev/null 2>&1
  python3 -I - <<'PY' > $OUT/probes_rest.txt
import csv
rows = [r for r in csv.DictReader(open('work/voice/voice-triggers.csv')) if r['class'] != 'bank' and not r['screen']]
roots = {(r['file'], r['label'][2:]) for r in rows}
for l in open('work/voice/probes.txt'):
    p = l.strip().split(':')
    if (p[0], p[1]) in roots: print(l.strip())
PY
  for fl in alt rand1 rand2 rand3 rand4 rand5 rand6 rand7 rand8; do
    xargs -P 11 -I{} sh -c "$PROBE $OUT/probe_$fl 3000 flags=$fl {} 2>&1 | cut -c1-300 >> $OUT/probe_out/$fl.\$\$.txt" < $OUT/probes_rest.txt
  done
  cat $OUT/probe_out/*.txt | grep PROBE | sed 's/ png=.*//' > $OUT/probe_all.txt
fi
$SNT voice-inventory assets/raw/files --csv $OUT/voice-triggers.csv --clips-csv $OUT/voice-clips.csv --scenes-csv $OUT/voice-scenes.csv \
  --md $OUT/tables.md --probe-results $OUT/probe_all.txt --icons docs/voice-icons.tsv --graphics-text docs/voice-graphics-text.tsv
