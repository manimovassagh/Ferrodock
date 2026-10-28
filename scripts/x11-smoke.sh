#!/usr/bin/env bash
# Starts Ferrodock on a virtual X11 screen (Xvfb) and checks that its window
# opens, keeps running, takes a click and a key, then saves a screenshot.
# Needs Xvfb, xdotool and ImageMagick's `import`; rendering goes through
# Mesa's software OpenGL, so no GPU is needed.
#
#   scripts/x11-smoke.sh [path/to/ferrodock] [screenshot.png]
set -euo pipefail

bin=${1:-target/debug/ferrodock}
shot=${2:-x11-smoke.png}
log=${shot%.png}.log
display=:${X11_SMOKE_DISPLAY:-99}

fail() {
    echo "x11-smoke: $*" >&2
    echo "--- app output ---" >&2
    cat "$log" >&2 || true
    exit 1
}

# Its keymap warnings are noise.
Xvfb "$display" -screen 0 1440x900x24 -nolisten tcp >/dev/null 2>&1 &
xvfb=$!
app=
trap 'kill $app $xvfb 2>/dev/null || true' EXIT
export DISPLAY=$display
# X11 only: never Wayland, even when the machine has it.
unset WAYLAND_DISPLAY
export XDG_SESSION_TYPE=x11

for _ in $(seq 50); do
    xdotool getdisplaygeometry >/dev/null 2>&1 && break
    sleep 0.1
done

"$bin" >"$log" 2>&1 &
app=$!

window=
for _ in $(seq 60); do
    window=$(xdotool search --name '^Ferrodock$' 2>/dev/null | head -1 || true)
    [ -n "$window" ] && break
    kill -0 "$app" 2>/dev/null || fail "Ferrodock exited before opening a window"
    sleep 1
done
[ -n "$window" ] || fail "no Ferrodock window after 60 seconds"
echo "window $window: $(xdotool getwindowgeometry "$window" | tail -1 | xargs)"

# Let it draw, then use it: a click on the sidebar's Images and a key.
sleep 5
kill -0 "$app" 2>/dev/null || fail "Ferrodock exited after opening its window"
xdotool windowfocus --sync "$window"
xdotool mousemove 73 185 click 1
sleep 1
xdotool key F5
sleep 3
kill -0 "$app" 2>/dev/null || fail "Ferrodock exited after a click and a key"

import -window root "$shot"
echo "x11-smoke: ok, screenshot in $shot"
