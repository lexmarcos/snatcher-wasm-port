#!/bin/bash
# usage: run_batch.sh "name spec frames" ...   (fid_session.py per entry, 4 at a time; an existing out/fid2/NAME.json is reused: --skip-play; set REPLAY=1 to record again)
cd "$(dirname "$0")"
run() { set -- $1; sk=""; [ -z "$REPLAY" ] && [ -s out/fid2/$1.json ] && sk="--skip-play"; .venv/bin/python -I fid_session.py $1 $2 $3 --every 500 $sk > out/fid2/$1.out 2>&1; }
export -f run
printf '%s\n' "$@" | xargs -P ${PAR:-4} -I{} bash -c 'run "{}"'
