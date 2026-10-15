"""One interface for sub-CPU PC hooks on both oracles: new_machine('hle'|'2', load='junker_hq') -> machine with add_hook('sub', pc)
and `hits` (list of ('sub', pc, regs{d0,d1,d2,d3,a0,a1,a2,a7})) filled by every step_frame. HLE: core_add_hook + _sub_bios override
(like re/oracle/fid_stall.py); oracle2: Machine2.add_hook (the core is stepped per instruction while hooks are set, state is identical)."""
import os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
ORA = os.path.join(HERE, '..', 'oracle')

def new_machine(kind, load='junker_hq', trace=None):
    if kind == '2':
        sys.path.insert(0, HERE)
        from o2lib import Machine2
        m = Machine2(trace_path=trace)
        if load: m.load_state(os.path.join(HERE, 'states', load + '.state'))
        m.inputs = []
        return m
    sys.path.insert(0, ORA)
    import machine as M
    class HM(M.Machine):
        def __init__(self, *a, **k):
            super().__init__(*a, **k); self._hk = set(); self.hits = []
        def add_hook(self, cpu, pc):
            assert cpu == 'sub'
            self._hk.add(pc); self.lib.core_add_hook(M.SUB, pc)
        def _sub_bios(self, pc):
            if pc in self._hk:
                s = self.sub
                self.hits.append(('sub', pc, {'d0': s.d(0), 'd1': s.d(1), 'd2': s.d(2), 'd3': s.d(3), 'a0': s.a(0), 'a1': s.a(1), 'a2': s.a(2), 'a7': s.a(7)}))
                return
            return super()._sub_bios(pc)
        def step_frame(self, capture=False):
            self.hits = []
            return super().step_frame(capture)
    m = HM(trace_path=trace)
    if load: m.load_state(os.path.join(ORA, 'states', load + '.pkl'))
    m.inputs = []
    return m
