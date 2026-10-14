#!/usr/bin/env python3
"""Request exit from the verified owned Sway peer; process wait is a separate gate."""

import json
import os
from pathlib import Path
import socket
import struct
import sys
import time

HEADER = struct.Struct("=6sII")  # Sway IPC uses native endian, fixed-width integers.
MAGIC = b"i3-ipc"
MAX_PAYLOAD = 64 * 1024


def receive(sock, deadline, allow_empty_eof=False):
    def exact(size, allow_eof=False):
        data = bytearray()
        while len(data) < size:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("Sway IPC deadline expired")
            sock.settimeout(remaining)
            chunk = sock.recv(size - len(data))
            if not chunk:
                if not data and allow_eof:
                    return None
                raise RuntimeError("Sway IPC disconnected within a required frame")
            data.extend(chunk)
        return bytes(data)

    header = exact(HEADER.size, allow_empty_eof)
    if header is None:
        return None
    magic, length, kind = HEADER.unpack(header)
    if magic != MAGIC or length > MAX_PAYLOAD:
        raise RuntimeError("Invalid or oversized Sway IPC frame")
    return kind, json.loads(exact(length))


def request_exit(path, expected_pid):
    receipt = {"status": "failed", "owned_compositor_pid": expected_pid,
               "socket": str(path), "exit_request_sent": False}
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(10)
            sock.connect(str(path))
            pid, uid, gid = struct.unpack("=3i", sock.getsockopt(
                socket.SOL_SOCKET, socket.SO_PEERCRED, struct.calcsize("=3i")))
            if pid != expected_pid or uid != os.getuid():
                raise RuntimeError("IPC peer is not the owned compositor")
            receipt["peer"] = {"pid": pid, "uid": uid, "gid": gid}
            deadline = time.monotonic() + 10
            sock.sendall(HEADER.pack(MAGIC, 0, 7))  # GET_VERSION: live peer receipt.
            kind, version = receive(sock, deadline)
            if kind != 7 or not isinstance(version, dict) or not all(
                    type(version.get(key)) is int and version[key] >= 0
                    for key in ("major", "minor", "patch")) or not isinstance(
                    version.get("human_readable"), str) or not version["human_readable"]:
                raise RuntimeError("Invalid Sway version acknowledgment")
            receipt["version"] = version
            sock.sendall(HEADER.pack(MAGIC, 4, 0) + b"exit")
            receipt["exit_request_sent"] = True
            receipt["request"] = {"type": 0, "payload": "exit", "frame_bytes": 18}
            response = receive(sock, deadline, allow_empty_eof=True)
            if response is None:
                # Sway 1.9 may terminate its event loop before flushing the
                # queued command reply. Only zero-byte EOF is this known case.
                receipt["reply_state"] = "eof_before_reply"
            else:
                kind, reply = response
                if kind != 0 or not isinstance(reply, list) or len(reply) != 1 or not isinstance(
                        reply[0], dict) or reply[0].get("success") is not True:
                    raise RuntimeError("Sway rejected or malformed the exit reply")
                receipt["reply_state"] = "acknowledged"
                receipt["reply"] = reply
            receipt["status"] = "requested"
    except (OSError, ValueError, RuntimeError, TypeError) as error:
        receipt["error"] = str(error)
    return receipt


if __name__ == "__main__":
    result = request_exit(Path(sys.argv[1]), int(sys.argv[2]))
    print(json.dumps(result, indent=2))
    if result["status"] != "requested":
        print(result["error"], file=sys.stderr)
        sys.exit(1)
