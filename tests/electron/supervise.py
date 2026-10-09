"""Linux-only runtime supervisor: no sandbox changes, process-name matching, or downloads."""
import ctypes
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time


def snapshot():
    processes = {}
    for item in Path('/proc').iterdir():
        if not item.name.isdigit():
            continue
        try:
            fields = (item / 'stat').read_text().rsplit(')', 1)[1].split()
            processes[int(item.name)] = {'pid': int(item.name), 'ppid': int(fields[1]),
                                         'start': fields[19], 'state': fields[0]}
        except (FileNotFoundError, ProcessLookupError):
            pass
    return processes


def descendants(processes, parent):
    owned = {parent}
    while True:
        expanded = owned | {pid for pid, value in processes.items() if value['ppid'] in owned}
        if expanded == owned:
            return [value for pid, value in processes.items() if pid in owned and pid != parent]
        owned = expanded


def identity(value):
    return (value['pid'], value['start'])


def supervise(command, phase, evidence, deadline_seconds=150):
    # Reparent orphaned grandchildren here, so double-forks cannot escape the ownership tree.
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:  # PR_SET_CHILD_SUBREAPER
        raise OSError(ctypes.get_errno(), 'cannot establish child subreaper')
    evidence.mkdir(parents=True, exist_ok=True)
    report = {'phase': phase, 'owned': [], 'mainExit': None, 'shutdownSnapshot': [],
              'remaining': [], 'observedExit': False, 'forcedCleanup': False}
    owned = {}
    with tempfile.TemporaryDirectory(prefix='pentimento-process-probe-') as channel:
        env = {**os.environ, 'PENTIMENTO_SHUTDOWN_CHANNEL': channel}
        child = subprocess.Popen(command, env=env)
        finish = time.monotonic() + deadline_seconds
        exit_deadline = None
        acknowledged = False
        while True:
            code = child.poll()
            # Popen owns/reaps the main PID; we reap only after it has been observed exiting.
            if code is not None:
                while True:
                    try:
                        pid, _ = os.waitpid(-1, os.WNOHANG)
                        if pid == 0:
                            break
                    except ChildProcessError:
                        break
                if exit_deadline is None:
                    exit_deadline = time.monotonic() + 10
                report['mainExit'] = code
            processes = snapshot()
            current = descendants(processes, os.getpid())
            for value in current:
                owned[identity(value)] = value
            if not acknowledged and Path(channel, 'ready').exists():
                report['shutdownSnapshot'] = current
                # Persist identities before permitting the app to close its window.
                report['owned'] = list(owned.values())
                (evidence / f'{phase}-processes.json').write_text(json.dumps(report, indent=2))
                Path(channel, 'ack').write_text('captured')
                acknowledged = True
            remaining = [value for value in processes.values() if identity(value) in owned]
            if code is not None and not remaining:
                report['observedExit'] = True
                break
            if time.monotonic() > finish or (exit_deadline and time.monotonic() > exit_deadline):
                report['remaining'] = remaining
                break
            time.sleep(0.02)
        report['owned'] = list(owned.values())
        report['handshake'] = acknowledged
        success = code == 0 and acknowledged and report['observedExit']
        if not report['observedExit']:
            report['forcedCleanup'] = True
            # Only signal identities whose PID/start-time still matches this owned family.
            for sig in (signal.SIGTERM, signal.SIGKILL):
                processes = snapshot()
                for value in descendants(processes, os.getpid()):
                    owned[identity(value)] = value
                for value in processes.values():
                    if identity(value) in owned:
                        try:
                            os.kill(value['pid'], sig)
                        except ProcessLookupError:
                            pass
                time.sleep(0.2)
            child.wait(timeout=5)
            while True:
                try:
                    pid, _ = os.waitpid(-1, os.WNOHANG)
                    if pid == 0:
                        break
                except ChildProcessError:
                    break
        report['success'] = success
        (evidence / f'{phase}-processes.json').write_text(json.dumps(report, indent=2) + '\n')
        return 0 if success else 1


if __name__ == '__main__':
    phase, *command = sys.argv[1:]
    if not command:
        raise SystemExit('usage: supervise.py PHASE COMMAND [ARGS...]')
    raise SystemExit(supervise(command, phase, Path('electron-runtime-evidence')))
