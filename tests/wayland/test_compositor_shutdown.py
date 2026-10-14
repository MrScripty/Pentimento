#!/usr/bin/env python3
"""Protocol/process fixtures, never substitutes for the hosted real Sway gate."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "tests/wayland/compositor-exit.py"
SHELL = ROOT / "tests/wayland/compositor-shutdown.sh"

# Independent framed UNIX peer. Its PID is the actual shell-owned child PID.
SERVER = r'''
import json, os, signal, socket, struct, sys, time
from pathlib import Path
path, mode = sys.argv[1:]
header = struct.Struct("=6sII")
def frame(kind, payload):
    raw = json.dumps(payload).encode()
    return header.pack(b"i3-ipc", len(raw), kind) + raw
def exact(conn, count):
    data = b""
    while len(data) < count:
        chunk = conn.recv(count - len(data))
        if not chunk:
            raise EOFError()
        data += chunk
    return data
with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
    listener.bind(path)
    listener.listen(1)
    Path(path + ".ready").touch()
    conn, _ = listener.accept()
    with conn:
        try:
            request = exact(conn, header.size)
            assert header.unpack(request) == (b"i3-ipc", 0, 7)
            if mode == "disconnect_version":
                sys.exit(0)
            version = {"major": 1, "minor": 9, "patch": 0, "human_readable": "1.9"}
            if mode == "bad_version": version["major"] = True
            conn.sendall(frame(7, version))
            if mode == "bad_version": sys.exit(0)
            request = exact(conn, 18)
            assert request == header.pack(b"i3-ipc", 4, 0) + b"exit"
            Path(path + ".request").write_text(request.hex())
            if mode in ("eof", "nonzero", "signaled"):
                pass
            elif mode == "partial_header": conn.sendall(b"i3-")
            elif mode == "partial_payload": conn.sendall(header.pack(b"i3-ipc", 20, 0) + b"[")
            elif mode == "reject": conn.sendall(frame(0, [{"success": False}]))
            elif mode == "wrong_kind": conn.sendall(frame(7, [{"success": True}]))
            elif mode == "bad_magic": conn.sendall(b"wrong!" + struct.pack("=II", 0, 0))
            elif mode == "oversized": conn.sendall(header.pack(b"i3-ipc", 65537, 0))
            elif mode == "invalid_json": conn.sendall(header.pack(b"i3-ipc", 1, 0) + b"!")
            elif mode == "fragmented":
                for byte in frame(0, [{"success": True}]): conn.sendall(bytes([byte]))
            else: conn.sendall(frame(0, [{"success": True}]))
        except (EOFError, BrokenPipeError, ConnectionResetError):
            sys.exit(0)
if mode == "nonzero": sys.exit(9)
if mode == "signaled": os.kill(os.getpid(), signal.SIGTERM)
if mode == "hang_after_reply": time.sleep(30)
'''


class ShutdownTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.fixture = self.directory / "peer.py"
        self.fixture.write_text(SERVER)
        self.socket = self.directory / "sway.sock"

    def request(self, mode, wrong_pid=False):
        import time
        for suffix in (".ready", ".request"):
            Path(str(self.socket) + suffix).unlink(missing_ok=True)
        peer = subprocess.Popen([sys.executable, str(self.fixture), str(self.socket), mode])
        try:
            deadline = time.monotonic() + 3
            while not Path(str(self.socket) + ".ready").exists():
                if peer.poll() is not None or time.monotonic() > deadline:
                    self.fail("fixture did not start")
                time.sleep(.002)
            result = subprocess.run([sys.executable, str(HELPER), str(self.socket),
                                     str(peer.pid + (100000 if wrong_pid else 0))],
                                    text=True, capture_output=True, timeout=12)
            receipt = json.loads(result.stdout)
            if receipt["status"] == "requested":
                self.assertEqual(receipt["peer"]["pid"], peer.pid)
                self.assertTrue(receipt["exit_request_sent"])
                self.assertEqual(Path(str(self.socket) + ".request").read_text(),
                                 "69332d697063040000000000000065786974" if sys.byteorder == "little"
                                 else "69332d697063000000040000000065786974")
            return result, receipt
        finally:
            if peer.poll() is None:
                peer.terminate()
            peer.wait(timeout=3)

    def test_clean_eof_is_request_receipt_only(self):
        result, receipt = self.request("eof")
        self.assertEqual(result.returncode, 0)
        self.assertEqual(receipt["status"], "requested")
        self.assertEqual(receipt["reply_state"], "eof_before_reply")
        self.assertNotIn("natural_exit_status", receipt)

    def test_success_and_fragmented_success_replies(self):
        for mode in ("reply", "fragmented"):
            with self.subTest(mode=mode):
                result, receipt = self.request(mode)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(receipt["reply_state"], "acknowledged")
                self.socket.unlink()

    def test_partial_frames_and_arbitrary_replies_rejected(self):
        for mode in ("partial_header", "partial_payload", "reject", "wrong_kind",
                     "bad_magic", "oversized", "invalid_json"):
            with self.subTest(mode=mode):
                result, receipt = self.request(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(receipt["status"], "failed")
                self.assertTrue(receipt["exit_request_sent"])
                self.socket.unlink()

    def test_preflight_failures_never_send_exit(self):
        for mode in ("disconnect_version", "bad_version"):
            with self.subTest(mode=mode):
                result, receipt = self.request(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(receipt["exit_request_sent"])
                self.assertFalse(Path(str(self.socket) + ".request").exists())
                self.socket.unlink()

    def test_wrong_peer_pid_never_sends_exit(self):
        result, receipt = self.request("reply", wrong_pid=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(receipt["exit_request_sent"])
        self.assertIn("not the owned", receipt["error"])

    def shell_shutdown(self, mode):
        for suffix in (".ready", ".request"):
            Path(str(self.socket) + suffix).unlink(missing_ok=True)
        # Accelerate only fixture polling, preserving the production iteration
        # bound. Forced test cleanup happens after a failed gate, never success.
        script = r'''
set -euo pipefail
ROOT="$1"
source "$2"
python3 "$3" "$4" "$5" &
owned=$!
trap 'kill -TERM "$owned" 2>/dev/null || true; wait "$owned" 2>/dev/null || true' EXIT
for _ in $(seq 1 1000); do
  if [[ -f "$4.ready" ]]; then break; fi
  command sleep 0.002
done
sleep() { command sleep 0.002; }
shutdown_compositor "$owned" "$4" "$6"
printf '%s\n' passed > "$6/gate-passed"
'''
        return subprocess.run(["bash", "-c", script, "shutdown-test", str(ROOT), str(SHELL),
                               str(self.fixture), str(self.socket), mode, str(self.directory)],
                              text=True, capture_output=True, timeout=15)

    def test_shell_requires_natural_owned_zero_exit(self):
        result = self.shell_shutdown("eof")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue((self.directory / "gate-passed").exists())
        receipt = json.loads((self.directory / "compositor-wait.json").read_text())
        self.assertEqual(receipt["natural_exit_status"], 0)

    def test_shell_rejects_nonzero_and_signal_exit(self):
        for mode, expected in (("nonzero", 9), ("signaled", 143)):
            with self.subTest(mode=mode):
                result = self.shell_shutdown(mode)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse((self.directory / "gate-passed").exists())
                receipt = json.loads((self.directory / "compositor-wait.json").read_text())
                self.assertEqual(receipt["natural_exit_status"], expected)
                self.socket.unlink()

    def test_shell_rejects_hang_without_counting_cleanup_as_exit(self):
        result = self.shell_shutdown("hang_after_reply")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("did not exit naturally", result.stderr)
        self.assertFalse((self.directory / "gate-passed").exists())
        self.assertFalse((self.directory / "compositor-wait.json").exists())


spec = importlib.util.spec_from_file_location("compositor_exit", HELPER)
ipc = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ipc)


class FakeSocket:
    def __init__(self, data, pid=123, error=None):
        self.data = bytearray(data)
        self.pid = pid
        self.error = error
        self.sent = []

    def __enter__(self): return self
    def __exit__(self, *_): pass
    def settimeout(self, value): self.timeout = value
    def connect(self, path): self.path = path
    def getsockopt(self, *_):
        import os, struct
        return struct.pack("=3i", self.pid, os.getuid(), os.getgid())
    def sendall(self, data): self.sent.append(data)
    def recv(self, size):
        if not self.data and self.error:
            raise self.error
        result = bytes(self.data[:min(size, 3)])  # Always fragment frames.
        del self.data[:len(result)]
        return result


def packet(kind, payload):
    raw = json.dumps(payload).encode()
    return ipc.HEADER.pack(ipc.MAGIC, len(raw), kind) + raw


class ShutdownUnitTests(unittest.TestCase):
    version = {"major": 1, "minor": 9, "patch": 0, "human_readable": "1.9"}

    def request(self, response, **kwargs):
        fake = FakeSocket(packet(7, self.version) + response, **kwargs)
        with patch.object(ipc.socket, "socket", return_value=fake):
            result = ipc.request_exit(Path("/owned/sway.sock"), 123)
        return result, fake

    def test_eof_after_complete_authenticated_request_is_not_passed(self):
        result, fake = self.request(b"")
        self.assertEqual(result["status"], "requested")
        self.assertEqual(result["reply_state"], "eof_before_reply")
        self.assertEqual(result["peer"]["pid"], 123)
        self.assertEqual(fake.sent, [ipc.HEADER.pack(ipc.MAGIC, 0, 7),
                                    ipc.HEADER.pack(ipc.MAGIC, 4, 0) + b"exit"])
        self.assertNotIn("natural_exit_status", result)

    def test_fragmented_success_reply(self):
        result, _ = self.request(packet(0, [{"success": True}]))
        self.assertEqual(result["status"], "requested")
        self.assertEqual(result["reply_state"], "acknowledged")

    def test_partial_reset_timeout_and_rejected_reply_fail(self):
        cases = [(b"i3-", None),
                 (ipc.HEADER.pack(ipc.MAGIC, 20, 0) + b"[", None),
                 (b"", ConnectionResetError("reset")),
                 (b"", TimeoutError("timeout")),
                 (packet(0, [{"success": False}]), None),
                 (packet(7, [{"success": True}]), None),
                 (packet(0, []), None),
                 (packet(0, [{"success": 1}]), None)]
        for response, error in cases:
            with self.subTest(response=response, error=error):
                result, _ = self.request(response, error=error)
                self.assertEqual(result["status"], "failed")
                self.assertTrue(result["exit_request_sent"])

    def test_wrong_peer_never_sends_command(self):
        result, fake = self.request(b"", pid=124)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["exit_request_sent"])
        self.assertEqual(fake.sent, [])

    def test_preflight_disconnect_and_deadline_fail(self):
        import time
        fake = FakeSocket(b"")
        with patch.object(ipc.socket, "socket", return_value=fake):
            result = ipc.request_exit(Path("/owned/sway.sock"), 123)
        self.assertEqual(result["status"], "failed")
        self.assertFalse(result["exit_request_sent"])
        with self.assertRaises(TimeoutError):
            ipc.receive(FakeSocket(b""), time.monotonic() - 1, allow_empty_eof=True)

    def test_shell_process_gate_rejects_cleanup_nonzero_signal_and_hang(self):
        # Isolate process ownership/wait logic. Real framed socket tests above
        # remain mandatory hosted checks; this fake helper is not IPC evidence.
        for mode, expected in (("zero", 0), ("nonzero", 9), ("signal", 143), ("hang", None)):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                script = r"""
set -euo pipefail
ROOT="$1"
source "$2"
mode="$3"
evidence="$4"
case "$mode" in
  zero) (command sleep .05; exit 0) & ;;
  nonzero) (command sleep .05; exit 9) & ;;
  signal) (command sleep .05; kill -TERM "$BASHPID") & ;;
  hang) command sleep 30 & ;;
esac
owned=$!
trap 'kill -TERM "$owned" 2>/dev/null || true; wait "$owned" 2>/dev/null || true' EXIT
python3() { printf '%s\n' '{"status":"requested"}'; }
sleep() { command sleep .002; }
shutdown_compositor "$owned" /fixture/socket "$evidence"
printf '%s\n' passed > "$evidence/gate-passed"
"""
                run = subprocess.run(["bash", "-c", script, "process-test", str(ROOT), str(SHELL),
                                      mode, directory], text=True, capture_output=True, timeout=5)
                evidence = Path(directory)
                self.assertEqual((evidence / "gate-passed").exists(), mode == "zero")
                if mode == "zero": self.assertEqual(run.returncode, 0, run.stderr)
                else: self.assertNotEqual(run.returncode, 0)
                if expected is None:
                    self.assertFalse((evidence / "compositor-wait.json").exists())
                    self.assertIn("did not exit naturally", run.stderr)
                else:
                    wait = json.loads((evidence / "compositor-wait.json").read_text())
                    self.assertEqual(wait["natural_exit_status"], expected)


if __name__ == "__main__":
    unittest.main()
