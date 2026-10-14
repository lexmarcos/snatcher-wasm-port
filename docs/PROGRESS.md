# Progress

## How to run
```
cargo build --release                       # tools (snt)
chdman extractcd -i "Snatcher (USA).chd" -o work/snatcher.cue -ob work/snatcher.bin
./target/release/snt extract work/snatcher.bin work/snatcher.cue assets/raw
```
RE: `re/analyze.sh` (Ghidra headless, see re/README.md) and `re/dis68k.py` (capstone listing).

## Milestones
- M0 extraction: DONE (IP/SP carved, 97 ISO files + 3 txt dumped, 20 CD-DA tracks → WAV/Opus).
