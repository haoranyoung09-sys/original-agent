#!/bin/sh
# Capture the real full-screen original UI for the README.
# This is a desktop screenshot of the release binary, not a mockup.
#
# Requirements: a built target/release/original, DISPLAY, xfce4-terminal,
# xdotool, and ffmpeg. The program is started with an empty temporary HOME
# and an empty temporary working directory, so it does not read personal
# config, API keys, or existing session archives, and it does not call a model.
#
# Usage, from the repository root:
#   cargo build --locked --release
#   sh docs/assets/capture-readme-demo.sh
#
# Outputs:
#   docs/assets/fullscreen-startup.png
#   docs/assets/fullscreen-help.png
#   docs/assets/fullscreen-help.mp4

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
binary=$root/target/release/original
out=$root/docs/assets
test -x "$binary"
test -n "${DISPLAY:-}"

demo_cwd=$(mktemp -d /tmp/original-readme-demo.XXXXXX)
demo_home=$(mktemp -d /tmp/original-readme-home.XXXXXX)
printf 'empty temporary directory created only for this screenshot\n' > "$demo_cwd/README.txt"

cleanup() {
  if [ -n "${term_pid:-}" ]; then
    kill "$term_pid" 2>/dev/null || true
  fi
  if [ -n "${wid:-}" ]; then
    xdotool windowkill "$wid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

xfce4-terminal --disable-server \
  --hide-menubar --hide-scrollbar --hide-toolbar --hide-borders \
  --font 'JetBrains Mono 14' \
  --geometry 130x36 \
  --title original-readme-demo \
  --color-bg '#1b1b1b' --color-text '#e6e6e6' \
  --working-directory "$demo_cwd" \
  --command "env HOME=$demo_home $binary" \
  >/tmp/original-readme-demo-terminal.log 2>&1 &
term_pid=$!

wid=
i=0
while [ "$i" -lt 50 ]; do
  for candidate in $(xdotool search --name original-readme-demo 2>/dev/null || true); do
    geom=$(xdotool getwindowgeometry --shell "$candidate" 2>/dev/null || true)
    width=$(printf '%s\n' "$geom" | sed -n 's/^WIDTH=//p')
    height=$(printf '%s\n' "$geom" | sed -n 's/^HEIGHT=//p')
    if [ "${width:-0}" -ge 400 ] && [ "${height:-0}" -ge 300 ]; then
      wid=$candidate
      break
    fi
  done
  if [ -n "$wid" ]; then
    break
  fi
  i=$((i + 1))
  sleep 0.1
done
test -n "$wid"

xdotool windowmove "$wid" 20 20
xdotool windowactivate --sync "$wid"
xdotool windowfocus --sync "$wid"
xdotool mousemove 2 2
# The sidebar refreshes from the session archive shortly after startup.
sleep 2.5

shot() {
  eval "$(xdotool getwindowgeometry --shell "$wid")"
  if [ "${WIDTH:-0}" -lt 400 ] || [ "${HEIGHT:-0}" -lt 300 ]; then
    echo "window geometry is too small: ${WIDTH:-0}x${HEIGHT:-0}" >&2
    exit 1
  fi
  ffmpeg -y -loglevel error -f x11grab -video_size "${WIDTH}x${HEIGHT}" \
    -i "${DISPLAY}+${X},${Y}" -frames:v 1 "$1"
}

shot "$out/fullscreen-startup.png"

# Record the real keystrokes that open the built-in help panel.
eval "$(xdotool getwindowgeometry --shell "$wid")"
ffmpeg -y -loglevel error -f x11grab -framerate 12 -video_size "${WIDTH}x${HEIGHT}" \
  -i "${DISPLAY}+${X},${Y}" -t 7 -c:v libx264 -pix_fmt yuv420p -crf 28 \
  "$out/fullscreen-help.mp4" >/tmp/original-readme-demo-ffmpeg.log 2>&1 &
record_pid=$!
sleep 0.5
xdotool windowfocus --sync "$wid"
# Focused X key events reach the real terminal. --window alone did not.
xdotool type --delay 110 '/help'
xdotool key Return
sleep 1.4
shot "$out/fullscreen-help.png"
wait "$record_pid"
if cmp -s "$out/fullscreen-startup.png" "$out/fullscreen-help.png"; then
  echo "help screenshot matches startup; keystrokes were not visible" >&2
  exit 1
fi

xdotool windowfocus --sync "$wid"
xdotool type --delay 40 '/exit'
xdotool key Return
sleep 0.4
