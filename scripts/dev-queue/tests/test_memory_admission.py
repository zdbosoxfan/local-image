import importlib.machinery
import importlib.util
from pathlib import Path
import tempfile
import unittest

GATE = Path(__file__).resolve().parents[1] / 'bin/li-rustc-gate'
loader = importlib.machinery.SourceFileLoader('memory_gate', str(GATE))
spec = importlib.util.spec_from_loader(loader.name, loader)
gate = importlib.util.module_from_spec(spec)
loader.exec_module(gate)
GIB = 2**30


class MemoryAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.base = self.root/'cgroups'
        self.parent = self.base/'jobs'
        self.child = self.parent/'worker'
        self.child.mkdir(parents=True)
        self.meminfo = self.root/'meminfo'
        self.meminfo.write_text('MemAvailable: 52428800 kB\n')
        self.membership = self.root/'membership'
        self.membership.write_text('0::/jobs/worker\n')
        for p in (self.parent, self.child):
            (p/'memory.current').write_text(str(39*GIB))
            (p/'memory.high').write_text('max')
            (p/'memory.max').write_text('max')
            (p/'memory.stat').write_text(f'inactive_file {35*GIB}\nfile_dirty 0\nfile_writeback 0\n')
        (self.parent/'memory.high').write_text(str(40*GIB))
        (self.parent/'memory.max').write_text(str(44*GIB))

    def admitted(self):
        return gate.enough_memory(10, meminfo=self.meminfo, cgroup=self.membership, base=self.base)

    def test_clean_cache_does_not_deadlock_idle_build(self):
        self.assertTrue(self.admitted())

    def test_real_parent_working_memory_still_blocks(self):
        (self.parent/'memory.stat').write_text('inactive_file 0\nfile_dirty 0\nfile_writeback 0\n')
        self.assertFalse(self.admitted())

    def test_dirty_and_writeback_cache_is_not_credited(self):
        (self.parent/'memory.stat').write_text(f'inactive_file {35*GIB}\nfile_dirty {34*GIB}\nfile_writeback {GIB}\n')
        self.assertFalse(self.admitted())

    def test_missing_stats_fail_conservatively(self):
        (self.parent/'memory.stat').unlink()
        self.assertFalse(self.admitted())

    def test_partial_stats_fail_conservatively(self):
        (self.parent/'memory.stat').write_text(f'inactive_file {35*GIB}\n')
        self.assertFalse(self.admitted())

    def test_host_reserve_remains_required(self):
        self.meminfo.write_text('MemAvailable: 9437184 kB\n')
        self.assertFalse(self.admitted())

    def test_child_hard_limit_remains_required(self):
        (self.child/'memory.max').write_text(str(5*GIB))
        self.assertFalse(self.admitted())


if __name__ == '__main__':
    unittest.main()
