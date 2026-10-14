#!/usr/bin/env bash
# A real isolated compositor observes the vendored SCTK client-decoration path.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EVIDENCE="${PENTIMENTO_WAYLAND_EVIDENCE:?Set an absolute evidence directory}"
CLIENT="${PENTIMENTO_WAYLAND_CLIENT:?Set the built Wayland smoke executable}"
mkdir -p "$EVIDENCE"
EVIDENCE="$(cd "$EVIDENCE" && pwd)"
runtime="$(mktemp -d)"
export XDG_RUNTIME_DIR="$runtime"
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1
export PENTIMENTO_WAYLAND_CONTROL="$runtime/title-control"
app_id=io.github.pentimento.wayland-title-smoke
client_pid=""; compositor_pid=""

cleanup() {
  status=$?
  if [[ -n "$client_pid" ]] && kill -0 "$client_pid" 2>/dev/null; then
    kill -TERM "$client_pid" 2>/dev/null || true
    wait "$client_pid" 2>/dev/null || true
  fi
  if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
    kill -TERM "$compositor_pid" 2>/dev/null || true
    wait "$compositor_pid" 2>/dev/null || true
  fi
  rm -rf "$runtime"
  exit "$status"
}
trap cleanup EXIT

cat > "$runtime/sway.conf" <<'CONF'
output HEADLESS-1 resolution 1280x800 scale 1
output * bg #202020 solid_color
default_border none
default_floating_border none
for_window [app_id="io.github.pentimento.wayland-title-smoke"] floating enable
for_window [app_id="io.github.pentimento.wayland-title-smoke"] border none
CONF
sway -c "$runtime/sway.conf" > "$EVIDENCE/compositor.log" 2>&1 &
compositor_pid=$!

for _ in $(seq 1 150); do
  kill -0 "$compositor_pid"
  socket="$(find "$runtime" -maxdepth 1 -type s -name 'sway-ipc.*.sock' -print -quit)"
  display="$(find "$runtime" -maxdepth 1 -type s -name 'wayland-*' -print -quit)"
  if [[ -n "$socket" && -n "$display" ]]; then break; fi
  sleep 0.2
done
test -n "${socket:-}"; test -n "${display:-}"
export SWAYSOCK="$socket" WAYLAND_DISPLAY="$(basename "$display")"

wait_title() {
  expected="$1"
  for _ in $(seq 1 150); do
    kill -0 "$client_pid"
    swaymsg -t get_tree > "$runtime/tree.json"
    if jq -e --arg id "$app_id" --arg title "$expected" \
      '.. | objects | select(.app_id? == $id and .name? == $title)' \
      "$runtime/tree.json" >/dev/null \
      && grep -Fq "compositor frame acknowledged title=$expected" "$EVIDENCE/client.log"; then
      return
    fi
    sleep 0.2
  done
  echo "No acknowledged compositor title: $expected" >&2
  return 1
}

snapshot() {
  name="$1"
  swaymsg -t get_tree > "$EVIDENCE/$name-tree.json"
  swaymsg -t get_outputs > "$EVIDENCE/$name-outputs.json"
  grim -o HEADLESS-1 "$EVIDENCE/$name.png"
}

wait_width() {
  expected="$1"
  for _ in $(seq 1 150); do
    swaymsg -t get_tree > "$runtime/tree.json"
    if jq -e --arg id "$app_id" --argjson width "$expected" \
      '.. | objects | select(.app_id? == $id and .rect.width? == $width)' \
      "$runtime/tree.json" >/dev/null; then return; fi
    kill -0 "$client_pid"
    sleep 0.2
  done
  echo "No configured compositor width: $expected" >&2; return 1
}

"$CLIENT" > "$EVIDENCE/client.log" 2>&1 &
client_pid=$!
wait_title 'Pentimento AV To é Ω 日本 😀'
snapshot initial
printf '%s\n' 'Pentimento WWW iii AV To é Ω 日本 😀' > "$PENTIMENTO_WAYLAND_CONTROL"
wait_title 'Pentimento WWW iii AV To é Ω 日本 😀'
snapshot updated

swaymsg "[app_id=\"$app_id\"] resize set width 720 height 360" > "$EVIDENCE/wide-command.json"
printf '%s\n' 'Pentimento wide AV To é Ω 日本 😀' > "$PENTIMENTO_WAYLAND_CONTROL"
wait_title 'Pentimento wide AV To é Ω 日本 😀'
wait_width 720
snapshot wide
swaymsg "[app_id=\"$app_id\"] resize set width 280 height 210" > "$EVIDENCE/narrow-command.json"
printf '%s\n' 'Pentimento narrow AV To é Ω 日本 😀 WWWWWWWW iiiiiiiii overlong title must clip before controls' > "$PENTIMENTO_WAYLAND_CONTROL"
wait_title 'Pentimento narrow AV To é Ω 日本 😀 WWWWWWWW iiiiiiiii overlong title must clip before controls'
wait_width 280
snapshot narrow

swaymsg "[app_id=\"$app_id\"] resize set width 480 height 240" > "$EVIDENCE/resize-command.json"
swaymsg 'output HEADLESS-1 scale 2' > "$EVIDENCE/scale-command.json"
printf '%s\n' 'Pentimento scaled AV To é Ω 日本 😀' > "$PENTIMENTO_WAYLAND_CONTROL"
wait_title 'Pentimento scaled AV To é Ω 日本 😀'
for _ in $(seq 1 100); do
  if grep -Fq 'output scale=2' "$EVIDENCE/client.log"; then break; fi
  sleep 0.2
done
grep -Fq 'output scale=2' "$EVIDENCE/client.log"
wait_width 480
snapshot scaled
python3 "$ROOT/tests/wayland/title-pixels.py" "$EVIDENCE"

swaymsg "[app_id=\"$app_id\"] kill" > "$EVIDENCE/close-command.json"
for _ in $(seq 1 100); do
  if ! kill -0 "$client_pid" 2>/dev/null; then break; fi
  sleep 0.2
done
if kill -0 "$client_pid" 2>/dev/null; then
  echo 'Client did not exit after compositor close request' >&2; exit 1
fi
wait "$client_pid"
client_pid=""
grep -Fq 'compositor requested close' "$EVIDENCE/client.log"
grep -Fq 'closed cleanly' "$EVIDENCE/client.log"
swaymsg -t get_tree > "$EVIDENCE/after-close-tree.json"
python3 "$ROOT/tests/wayland/title-pixels.py" --closed "$EVIDENCE/after-close-tree.json"
swaymsg exit > "$EVIDENCE/compositor-exit.json"
wait "$compositor_pid"
compositor_pid=""
printf '%s\n' '{"status":"passed","renderer":"pixman","client_close_observed":true,"compositor_exit_observed":true}' \
  > "$EVIDENCE/process-result.json"
