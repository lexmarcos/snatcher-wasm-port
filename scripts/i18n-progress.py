#!/usr/bin/env python3 -I
"""Rewrite the translation status table in docs/PROGRESS.md from `snt i18n-status` and i18n/pt-BR/reviews.tsv.
usage: scripts/i18n-progress.py [assets/raw/files]"""
import subprocess, sys, re, os
root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
files = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "assets/raw/files")
out = subprocess.run([os.path.join(root, "target/release/snt"), "i18n-status", os.path.join(root, "i18n/pt-BR"), files], capture_output=True, text=True, check=True).stdout
rev = {}
for l in open(os.path.join(root, "i18n/pt-BR/reviews.tsv"), encoding="utf8"):
    if l.startswith("#") or not l.strip(): continue
    f = l.rstrip("\n").split("\t"); rev[f[0]] = f[2]
rows = ["| file | strings | translated | not translatable (typed entries / kept) | reviewed | score |", "|---|---|---|---|---|---|"]
tot = [0, 0]
for l in out.splitlines():
    m = re.match(r"(SP\d\d) strings\s+(\d+) translated\s+(\d+) entry-menu (\d+)", l)
    if not m: continue
    f, n, t, e = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4))
    tot[0] += n; tot[1] += t
    rows.append(f"| {f} | {n} | {t} | {e if e else ''} | {'yes' if f in rev else 'no'} | {rev.get(f, '')} |")
rows.append(f"| **total** | **{tot[0]}** | **{tot[1]}** | | | |")
table = "\n".join(rows)
p = os.path.join(root, "docs/PROGRESS.md")
s = open(p, encoding="utf8").read()
block = "<!-- i18n-table-begin -->\n" + table + "\n<!-- i18n-table-end -->"
if "<!-- i18n-table-begin -->" in s:
    s = re.sub(r"<!-- i18n-table-begin -->.*?<!-- i18n-table-end -->", lambda m: block, s, flags=re.S)
else:
    s += "\n## Translation pt-BR (docs/i18n.md) - status\n" + block + "\n"
open(p, "w", encoding="utf8").write(s)
print(f"{tot[1]} of {tot[0]} strings translated")
