import importlib.machinery
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import signal

RUNNER = Path(__file__).resolve().parents[1] / 'bin/li-test-runner'
loader = importlib.machinery.SourceFileLoader('li_test_runner', str(RUNNER))
spec = importlib.util.spec_from_loader(loader.name, loader)
runner = importlib.util.module_from_spec(spec)
loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def run_program(self, code, budget='12'):
        return subprocess.run([sys.executable, str(RUNNER), sys.executable, '-c', code],
                              env=dict(os.environ, LI_TEST_MAX_GB=budget),
                              capture_output=True, text=True, timeout=10)

    def test_preserves_failure(self):
        self.assertEqual(self.run_program('raise SystemExit(7)').returncode, 7)

    def test_preserves_signal_failure(self):
        self.assertEqual(self.run_program('import os,signal; os.kill(os.getpid(),signal.SIGTERM)').returncode, 143)

    def test_invalid_budget_rejected_before_start(self):
        result = self.run_program('print("must not start")', 'nan')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, '')

    def test_disappearing_process_and_comm_parentheses(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root/'1').mkdir()
            (root/'2').mkdir()
            fields = ['S', '1', '123'] + ['0'] * 18 + ['9']
            (root/'1/stat').write_text('1 (name with ) parens) ' + ' '.join(fields))
            self.assertEqual(runner.group_rss_kb(123, root), 9 * os.sysconf('SC_PAGE_SIZE') // 1024)
            self.assertEqual(runner.group_rss_kb(124, root), 0)

    def test_grandchild_memory_is_counted_and_group_killed(self):
        with tempfile.TemporaryDirectory() as tmp:
            pidfile = Path(tmp)/'pid'
            leaf = f'import os,time; open({str(pidfile)!r},"w").write(str(os.getpid())); data=bytearray(80*1024*1024); time.sleep(8)'
            middle = f'import subprocess,sys; subprocess.run([sys.executable,"-c",{leaf!r}])'
            parent = f'import subprocess,sys; subprocess.run([sys.executable,"-c",{middle!r}])'
            result = self.run_program(parent, '0.06')
            self.assertEqual(result.returncode, 137, result.stderr)
            self.assertIn('process group RSS', result.stderr)
            pid = int(pidfile.read_text())
            stat = Path(f'/proc/{pid}/stat')
            try:
                state = stat.read_text().rsplit(')', 1)[1].split()[0]
            except FileNotFoundError:
                state = None
            self.assertIn(state, (None, 'Z'))

    def test_group_is_killed_before_leader_is_reaped(self):
        events = []
        class Child:
            pid = 123456
            def poll(self):
                raise AssertionError('poll reaps the leader before group cleanup')
            def wait(self):
                events.append('reap')
                return 0
        with patch.object(sys, 'argv', [str(RUNNER), 'example']), \
             patch.object(runner.subprocess, 'Popen', return_value=Child()), \
             patch.object(runner.os, 'waitid', return_value=object()) as waitid, \
             patch.object(runner.os, 'killpg', side_effect=lambda *args: events.append('kill')):
            self.assertEqual(runner.main(), 0)
            self.assertTrue(waitid.call_args.args[2] & os.WNOWAIT)
        self.assertEqual(events, ['kill', 'reap'])

    def test_signal_handler_never_reenters_child_wait(self):
        inside_handler = False
        events = []
        class Child:
            pid = 123456
            def wait(self):
                self_test.assertFalse(inside_handler, 'signal handler reentered wait')
                events.append('reap')
                return -signal.SIGKILL
            def poll(self):
                fire_signal()
        self_test = self
        def fire_signal(*args):
            nonlocal inside_handler
            inside_handler = True
            try:
                signal.getsignal(signal.SIGTERM)(signal.SIGTERM, None)
            finally:
                inside_handler = False
            return None
        with patch.object(sys, 'argv', [str(RUNNER), 'example']), \
             patch.object(runner.subprocess, 'Popen', return_value=Child()), \
             patch.object(runner.os, 'waitid', side_effect=fire_signal), \
             patch.object(runner, 'group_rss_kb', return_value=0), \
             patch.object(runner.time, 'sleep'), \
             patch.object(runner.os, 'killpg', side_effect=lambda *args: events.append('kill')):
            self.assertEqual(runner.main(), 143)
        self.assertEqual(events, ['kill', 'reap'])


if __name__ == '__main__':
    unittest.main()
