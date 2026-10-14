"""Quick linear capstone disassembly of a range of an image. usage: dis.py IMG BASE START END"""
import sys, capstone
img = open(sys.argv[1], 'rb').read(); base = int(sys.argv[2], 16)
s, e = int(sys.argv[3], 16), int(sys.argv[4], 16)
md = capstone.Cs(capstone.CS_ARCH_M68K, capstone.CS_MODE_BIG_ENDIAN | capstone.CS_MODE_M68K_000)
a = s
while a < e:
    ins = list(md.disasm(img[a - base:a - base + 10], a, count=1))
    if not ins:
        print('  %06x  dc.w %s' % (a, img[a-base:a-base+2].hex())); a += 2; continue
    i = ins[0]
    print('  %06x  %-20s %-8s %s' % (a, img[a-base:a-base+i.size].hex(), i.mnemonic, i.op_str)); a += i.size
