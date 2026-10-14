"""Print listing lines in an address range: modules_range.py listing.asm START END [nodata]"""
import sys, re
s, e = int(sys.argv[2], 16), int(sys.argv[3], 16)
nodata = len(sys.argv) > 4
pend = None
for line in open(sys.argv[1]):
    m = re.match(r'^  ([0-9a-f]{6})  ', line)
    if m:
        a = int(m.group(1), 16)
        if s <= a < e and not (nodata and 'dc.b' in line):
            if pend: print(pend); pend = None
            print(line.rstrip())
    elif line.strip().endswith(':') or ':    ;' in line:
        pend = line.rstrip()
