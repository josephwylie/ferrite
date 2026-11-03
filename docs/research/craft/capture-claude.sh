#!/bin/zsh
# Guided capture of the Claude app for the craft analysis.
# You set up each screen in Claude and press Enter; the script captures the Claude window at 1x.
# Motion steps record the main display for a few seconds while you do the action.
set -u
OUT="${0:A:h}/claude"
mkdir -p "$OUT"

# Find the Claude window id with a tiny Swift helper (compiled once into the output folder).
WID_BIN="$OUT/.wid"
if [[ ! -x "$WID_BIN" ]]; then
  cat > "$OUT/.wid.swift" <<'EOF'
import CoreGraphics
let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
for w in l where (w["kCGWindowOwnerName"] as? String) == "Claude" && (w["kCGWindowLayer"] as? Int) == 0 {
  print(w["kCGWindowNumber"]!); break
}
EOF
  swiftc -O "$OUT/.wid.swift" -o "$WID_BIN" || { echo "swiftc failed"; exit 1; }
fi

free_gb=$(df -g / | awk 'NR==2{print $4}')
if (( free_gb < 5 )); then echo "Only ${free_gb} GB free; stopping."; exit 1; fi

shot() { # shot NAME "what to show"
  print -P "%B$2%b"
  read "?  Set it up in Claude, then press Enter (s = skip) " k
  [[ "$k" == s ]] && return
  local wid=$("$WID_BIN")
  [[ -z "$wid" ]] && { echo "  Claude window not found"; return; }
  screencapture -x -o -l "$wid" "$OUT/$1.png" && echo "  saved $1.png"
}

rec() { # rec NAME SECONDS "what to do"
  print -P "%B$3%b"
  read "?  Get ready, press Enter, then do it within ${2}s (s = skip) " k
  [[ "$k" == s ]] && return
  ffmpeg -v error -y -f avfoundation -capture_cursor 1 -framerate 120 -pixel_format nv12 \
    -i "Capture screen 0:none" -c:v h264_videotoolbox -b:v 20M "$OUT/$1.mp4" 2>/dev/null &
  local pid=$!
  sleep "$2"; kill -INT $pid 2>/dev/null; sleep 1; kill -9 $pid 2>/dev/null
  [[ -s "$OUT/$1.mp4" ]] && echo "  saved $1.mp4" || echo "  recording failed"
}

echo "Use a scratch Claude Code session (not one you care about). Keep the window where it is."
shot new-session      "1. A new, empty Code session (the start screen)."
shot thread           "2. A thread with prose, a list, a code block and a collapsed tool summary line."
shot tools-open       "3. The same thread with a tool summary line expanded (one failed command if you have one)."
shot failed-open      "4. A failed command row expanded (\$ cmd, Exit code, stderr)."
shot model-menu       "5. The model menu open."
shot effort-menu      "6. The Effort submenu open."
shot permission-menu  "7. The permission menu open."
shot plus-menu        "8. The + menu open."
shot slash-picker     "9. Type / in the composer so the picker shows."
shot approval         "10. An approval card waiting (Manual mode, ask it to run any command)."
shot running          "11. A turn running: shimmer line and status line visible."
shot queued           "12. A queued message strip above the composer (Cmd+Enter while running)."
shot hover-actions    "13. Hover your own message so its action row shows."
shot subagents        "14. A 'Ran N agents' row expanded, or an agent open in the side panel."
shot side-panel       "15. The right panel open on the session rail (Context breakdown)."
shot usage-popover    "16. The usage ring popover open."
rec  menu-motion   6  "17. Motion: open and close the model menu, then the permission menu."
rec  send-motion  12  "18. Motion: send a short prompt and let the reply stream."
rec  queue-motion  6  "19. Motion: while a turn runs, Cmd+Enter a message so the queue strip appears."
rec  expand-motion 6  "20. Motion: expand and collapse a tool summary line."
echo "Done. Files in $OUT"
du -sh "$OUT"
