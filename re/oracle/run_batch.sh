#!/bin/bash
# usage: run_batch.sh "name spec frames" ...   (runs fid_session.py for each, 4 at a time)
cd "$(dirname "$0")"
run() { set -- $1; .venv/bin/python -I fid_session.py $1 $2 $3 --every 500 > out/fid2/$1.out 2>&1; }
export -f run
printf '%s\n' "$@" | xargs -P 4 -I{} bash -c 'run "{}"'
