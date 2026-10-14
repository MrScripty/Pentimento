#!/usr/bin/env bash
# Sourced by the smoke harness, which owns the compositor child and cleanup.
shutdown_compositor() {
  local owned_pid="$1" socket_path="$2" evidence="$3" exit_status
  if ! python3 "$ROOT/tests/wayland/compositor-exit.py" "$socket_path" "$owned_pid" \
    > "$evidence/compositor-exit.json"; then
    return 1
  fi
  for _ in $(seq 1 100); do
    if ! kill -0 "$owned_pid" 2>/dev/null; then break; fi
    sleep 0.2
  done
  if kill -0 "$owned_pid" 2>/dev/null; then
    echo 'Owned compositor did not exit naturally after verified IPC exit request' >&2
    return 1
  fi
  if wait "$owned_pid"; then exit_status=0; else exit_status=$?; fi
  printf '{"owned_compositor_pid":%s,"natural_exit_status":%s}\n' \
    "$owned_pid" "$exit_status" > "$evidence/compositor-wait.json"
  if [[ "$exit_status" != 0 ]]; then
    echo "Owned compositor exited with status $exit_status" >&2
    return 1
  fi
}
