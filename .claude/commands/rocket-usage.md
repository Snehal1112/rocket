---
description: Show the CPU and memory used by the running Rocket app (backend and WebKit helpers)
argument-hint: "[seconds]"
allowed-tools: Bash
---

# Rocket usage

Arguments: $ARGUMENTS

Report the CPU and memory use of the running Rocket desktop app. Run the one script below as a single Bash call. Do not use subagents. The sample window is the first argument in seconds (default 5, at most 60).

```bash
SECS="${ARGUMENTS:-5}"
case "$SECS" in ''|*[!0-9]*) SECS=5 ;; esac
[ "$SECS" -gt 60 ] && SECS=60
[ "$SECS" -lt 1 ] && SECS=1

# The app binary is "rocket". Its WebKit helpers are children of that process.
ROOT=$(pgrep -x rocket | head -1)
if [ -z "$ROOT" ]; then echo "Rocket is not running."; exit 0; fi
PIDS=$( (echo "$ROOT"; pgrep -P "$ROOT") )
LIST=$(echo "$PIDS" | paste -sd, -)

echo "Sampling $SECS s. Processes: $LIST"
# The second top frame is the live CPU over the window. The first is an average since start.
top -b -n 2 -d "$SECS" -p "$LIST" | awk '/^top/{n++} n==2' | sed -n '7,$p'
echo
printf '%-8s %-22s %9s %9s\n' PID NAME RSS_MB ANON_MB
TMP=$(mktemp)
echo "$PIDS" | while read -r p; do
  [ -r /proc/$p/status ] || continue
  NAME=$(cat /proc/$p/comm)
  RSS=$(awk '/^VmRSS/{print int($2/1024)}' /proc/$p/status)
  ANON=$(awk '/^RssAnon/{print int($2/1024)}' /proc/$p/status)
  printf '%-8s %-22s %9s %9s\n' "$p" "$NAME" "$RSS" "$ANON"
  echo "$RSS $ANON" >> "$TMP"
done
awk '{r+=$1; a+=$2} END{printf "%-8s %-22s %9d %9d\n", "TOTAL", "", r, a}' "$TMP"
rm -f "$TMP"
echo "Cores: $(nproc). CPU is per core, so 100% is one full core."
```

After the script, give a short summary in plain sentences:

- The total CPU and the total memory. RSS counts shared pages in every process, so name the anonymous total as the closer figure for private cost.
- Which process is the largest.
- A debug build (`target/debug/rocket`) uses more than a release build. Say so when the binary path contains `debug` (check with `readlink /proc/<pid>/exe`).
- The dev server (`vite`, `esbuild`) is not part of Rocket. Mention it only if the user asks.
