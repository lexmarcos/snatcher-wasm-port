#!/usr/bin/env python3
"""Dump sub/main memory of the original game at a frame. usage: probe_mem.py scenario frame cpu addr len"""
import json, os, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from machine import Machine
sc = json.load(open(sys.argv[1])); target = int(sys.argv[2])
m = Machine(); m.inputs = [list(x) for x in sc['inputs']]
while m.frame < target: m.step_frame(capture=False)
cpu = m.sub if sys.argv[3] == 'sub' else m.main
a, n = int(sys.argv[4], 16), int(sys.argv[5], 16)
d = bytes(cpu.r8(a + i) for i in range(n))
for i in range(0, n, 16): print('%06x %s' % (a + i, d[i:i+16].hex(' ')))
