"""Select the oracle behind the fid_* / shoot_* / interact tools: default = the BIOS-free HLE oracle (machine.py);
ORACLE=2 in the environment = oracle2 (the original on the real Sega CD BIOS, re/oracle2, docs/re/oracle2.md).
`Machine` has the same interface in both (step_frame, inputs, sub/main memory accessors, vdp.frame_image, load/save_state, gun)."""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
KIND = os.environ.get('ORACLE', 'hle')
if KIND == '2':
    sys.path.insert(0, os.path.join(HERE, '..', 'oracle2'))
    from o2compat import Machine
else:
    from machine import Machine

def is2():
    return KIND == '2'
