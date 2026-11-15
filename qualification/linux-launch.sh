#!/bin/sh
# Launch the release binary on a virtual X display (Xvfb) and prove the window
# appears and draws. Used by release-binaries.yml inside old-distribution
# containers (AlmaLinux 8, Ubuntu 20.04).
# Usage: linux-launch.sh EXE OUT_DIR   (WGPU_BACKEND etc. pass through)
set -eu
exe=$1; out=$2; here=$(dirname "$0")
mkdir -p "$out"
# -fbdir: Xvfb keeps its screen in an XWD file, so no screenshot tool is needed.
mkdir -p "$out/fb"
Xvfb :99 -screen 0 1280x800x24 -nolisten tcp -fbdir "$out/fb" >"$out/xvfb.log" 2>&1 &
xvfb=$!
export DISPLAY=:99
i=0; until xdpyinfo >/dev/null 2>&1; do i=$((i+1)); [ $i -lt 50 ] || { echo "Xvfb did not start"; cat "$out/xvfb.log"; exit 1; }; sleep 0.2; done
"$exe" >"$out/app.log" 2>&1 &
app=$!
found=
i=0; while [ $i -lt 60 ]; do
  if ! kill -0 $app 2>/dev/null; then echo "the app exited early:"; cat "$out/app.log"; kill $xvfb; exit 1; fi
  if xwininfo -root -tree | grep -q '"rust-mirror GUI"'; then found=1; break; fi
  i=$((i+1)); sleep 0.5
done
[ -n "$found" ] || { echo 'no "rust-mirror GUI" window after 30 s'; cat "$out/app.log"; kill $app $xvfb; exit 1; }
sleep 5   # let it draw a few frames
kill -0 $app || { echo "the app exited after mapping its window"; cat "$out/app.log"; kill $xvfb; exit 1; }
xwininfo -root -tree | grep '"rust-mirror GUI"' | tee "$out/window.txt"
status=0
python3 "$here/xwd2png.py" "$out/fb/Xvfb_screen0" "$out/screen.png" --min-colors 16 || status=1
kill $app; wait $app 2>/dev/null || true
kill $xvfb; wait $xvfb 2>/dev/null || true
rm -rf "$out/fb"
echo "app log:"; cat "$out/app.log"
exit $status
