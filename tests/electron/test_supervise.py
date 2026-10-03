"""Fixtures test real Linux process ownership without installing or running Electron."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
HANDSHAKE = '''
from pathlib import Path
import os, time
channel = Path(os.environ['PENTIMENTO_SHUTDOWN_CHANNEL'])
(channel / 'ready').write_text(str(os.getpid()))
while not (channel / 'ack').exists(): time.sleep(.01)
'''


class SupervisorTests(unittest.TestCase):
    def run_fixture(self, source):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory, 'fixture.py')
            fixture.write_text(source)
            result = subprocess.run([sys.executable, str(HERE / 'supervise.py'), 'fixture',
                                     sys.executable, str(fixture)], cwd=directory, timeout=20)
            report = json.loads(Path(directory, 'electron-runtime-evidence/fixture-processes.json').read_text())
            return result.returncode, report

    def test_main_and_child_exit_are_observed(self):
        code, report = self.run_fixture('import subprocess, sys\nchild = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(.3)"])\n' + HANDSHAKE + '\nchild.wait()\n')
        self.assertEqual(code, 0)
        self.assertTrue(report['observedExit'])
        self.assertTrue(report['handshake'])
        self.assertGreaterEqual(len(report['shutdownSnapshot']), 2)
        self.assertFalse(report['forcedCleanup'])

    def test_orphan_with_unrelated_command_is_owned_and_rejected(self):
        code, report = self.run_fixture('import subprocess, sys\nchild = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])\n' + HANDSHAKE)
        self.assertEqual(code, 1)
        self.assertEqual(report['mainExit'], 0)
        self.assertFalse(report['observedExit'])
        self.assertTrue(report['remaining'])
        self.assertTrue(report['forcedCleanup'])
        for value in report['remaining']:
            stat = Path('/proc', str(value['pid']), 'stat')
            if stat.exists():
                self.assertNotEqual(stat.read_text().rsplit(')', 1)[1].split()[19], value['start'])

    def test_exit_without_shutdown_handshake_is_rejected(self):
        code, report = self.run_fixture('pass\n')
        self.assertEqual(code, 1)
        self.assertTrue(report['observedExit'])
        self.assertFalse(report['handshake'])

    def test_pid_reuse_is_a_different_identity(self):
        spec = importlib.util.spec_from_file_location('supervise', HERE / 'supervise.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.assertNotEqual(module.identity({'pid': 123, 'start': '1'}), module.identity({'pid': 123, 'start': '2'}))


if __name__ == '__main__':
    unittest.main()
