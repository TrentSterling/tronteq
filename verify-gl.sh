#!/bin/bash
# Cycle every GL SHOW mode on the LIVE app and capture each to a PNG for
# AI-vision review. Per mode: stop app (scheduler, no UAC), patch settings.json
# (mode + VIZ tab), relaunch, let the feedback build, PrintWindow-capture.
# The user's original settings are backed up and restored at the end.
# Run from the tronteq repo root AFTER the current build is deployed.
set -u
SET=/c/ProgramData/TrontEq/settings.json
OUT=harness_shots
mkdir -p "$OUT"
cp "$SET" "$SET.verifybak"

# Audio stimulus so the FX have something to react to (loopback feeds the viz).
powershell -Command "(New-Object System.Media.SoundPlayer 'C:\\Windows\\Media\\Ring05.wav').PlayLooping(); Start-Sleep -Seconds 170" &
LOOPER=$!

# Modes: pass as args to verify a subset, default = all.
MODES="${*:-gl_warp gl_flame gl_smoke gl_plasma gl_starfield gl_kaleido gl_tunnel3d gl_metaballs gl_voronoi gl_nebula gl_terrain gl_ripples gl_julia}"
for m in $MODES; do
  schtasks //end //tn TrontEQ >/dev/null 2>&1
  sleep 1
  python - "$m" <<'EOF'
import json, sys, io
p = r"C:\ProgramData\TrontEq\settings.json"
s = json.load(io.open(p, encoding="utf-8"))
s["show_mode"] = sys.argv[1]
s["inspector_tab"] = "viz"
io.open(p, "w", encoding="utf-8").write(json.dumps(s, indent=2))
EOF
  schtasks //run //tn TrontEQ >/dev/null 2>&1
  sleep 7   # launch + feedback buffers fill + a few beats of stimulus
  # Self-screenshot pipe: drop the request, the app ReadPixels its own frame.
  rm -f /c/ProgramData/TrontEq/shot.png
  touch /c/ProgramData/TrontEq/shot.req
  for i in 1 2 3 4 5 6 7 8 9 10; do
    [ -f /c/ProgramData/TrontEq/shot.png ] && break
    sleep 0.4
  done
  if [ -f /c/ProgramData/TrontEq/shot.png ]; then
    cp /c/ProgramData/TrontEq/shot.png "$OUT/live_$m.png"
    echo "captured $m"
  else
    echo "MISSED $m"
  fi
done

# Restore the user's settings + relaunch with them.
schtasks //end //tn TrontEQ >/dev/null 2>&1
sleep 1
cp "$SET.verifybak" "$SET"
schtasks //run //tn TrontEQ >/dev/null 2>&1
kill $LOOPER 2>/dev/null
powershell -Command "(New-Object System.Media.SoundPlayer).Stop()" 2>/dev/null
echo "VERIFY CYCLE COMPLETE"
