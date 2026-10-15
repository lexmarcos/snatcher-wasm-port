"""Drop-in `Machine` for the fid_*/shoot_* tools (re/oracle/oracle_sel.py, ORACLE=2): Machine2 with the HLE oracle's constructor and
state-file naming (states/NAME.pkl of the HLE oracle -> re/oracle2/states/NAME.state)."""
import os
from o2lib import Machine2, HERE

class Machine(Machine2):
    def __init__(self, trace_path=None, verbose=False, **kw):
        super().__init__(trace_path=trace_path, verbose=verbose, **kw)

    def load_state(self, path):
        if path.endswith('.pkl'):
            path = os.path.join(HERE, 'states', os.path.basename(path)[:-4] + '.state')
        super().load_state(path)

    def save_state(self, path):
        if path.endswith('.pkl'):
            path = os.path.join(HERE, 'states', os.path.basename(path)[:-4] + '.state')
        super().save_state(path)
