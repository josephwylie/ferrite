#!/usr/bin/env bash
#
# edit-loop.sh — how long an agent waits after a one-line edit: the time
# from a changed function body to a checked, a test-built and an app-built
# tree, so a change to the build is judged by the same number every time.
#
# Builds the three once so the first sample starts warm, then N times
# changes the first line of one function body in FILE and times, in order:
#
#   check    cargo check -p ferrite-cockpit --tests
#   testbin  cargo nextest run -p ferrite-cockpit --no-run
#   app      cargo build -p ferrite --features headless
#
# The edit is `let _ = std::hint::black_box(<n>u32);` with a new n each
# sample, so every sample compiles a body that differs and no signature
# moves. FILE is restored when the script ends, however it ends. Prints
# each command's median, fastest and slowest wall time in seconds, and
# what it compiled in the last sample; every run's output is kept in the
# folder it names.
#
# Usage: scripts/bench/edit-loop.sh FILE [SAMPLES] [LINE]
#   FILE     a Rust file under crates/, from the repo root
#   SAMPLES  edits to time (default 3)
#   LINE     text of the line that opens the body to edit (default: the
#            first line in FILE that opens a non-const `fn` and ends in `{`)
# JOBS (default 3) is passed as cargo's -j and nextest's --build-jobs.

set -euo pipefail

cd "$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
FILE=${1:?usage: scripts/bench/edit-loop.sh FILE [SAMPLES] [LINE]}
SAMPLES=${2:-3}
LINE=${3:-}
JOBS=${JOBS:-3}
[ -f "$FILE" ] || { echo "edit-loop: no file $FILE" >&2; exit 1; }

LOGS="$(mktemp -d "${TMPDIR:-/tmp}/edit-loop.XXXXXX")"
cp "$FILE" "$LOGS/original"
# Restored with a new time, so cargo rebuilds what the edit compiled.
trap 'cp "$LOGS/original" "$FILE"' EXIT

names=(check testbin app)
commands=(
  "cargo check -j $JOBS --locked -p ferrite-cockpit --tests"
  "cargo nextest run --build-jobs $JOBS --locked -p ferrite-cockpit --no-run"
  "cargo build -j $JOBS --locked -p ferrite --features headless"
)

edit() {
  python3 - "$LOGS/original" "$FILE" "$1" "$LINE" <<'EDIT'
import re, sys
original, file, n, line = sys.argv[1:]
lines = open(original).read().split("\n")
opens = re.compile(r"^\s*(pub(\(\w+\))? )?(async |unsafe )*fn .*\{$")
at = next((i for i, l in enumerate(lines) if (l.strip() == line.strip() if line else opens.match(l))), None)
if at is None:
    sys.exit(f"edit-loop: no line in {file} opens a function body" + (f" as {line!r}" if line else ""))
indent = re.match(r"\s*", lines[at]).group() + "    "
lines.insert(at + 1, f"{indent}let _ = std::hint::black_box({n}u32);")
open(file, "w").write("\n".join(lines))
print(f"{file}:{at + 2}: {lines[at].strip()}")
EDIT
}

run() {
  local name=$1 command=$2 log=$3 start end
  start=$(date +%s.%N)
  if ! $command >"$log" 2>&1; then
    echo "edit-loop: $name failed, see $log" >&2
    exit 1
  fi
  end=$(date +%s.%N)
  echo "$end - $start" | bc
}

echo "edit-loop: logs in $LOGS, $(nproc) cores, load $(cut -d' ' -f1-3 /proc/loadavg)"
for i in 0 1 2; do
  took=$(run "${names[$i]}" "${commands[$i]}" "$LOGS/warm-${names[$i]}.log")
  echo "warm ${names[$i]}: $took s"
done

for sample in $(seq 1 "$SAMPLES"); do
  where=$(edit "$sample")
  [ "$sample" = 1 ] && echo "editing $where"
  for i in 0 1 2; do
    run "${names[$i]}" "${commands[$i]}" "$LOGS/$sample-${names[$i]}.log" >>"$LOGS/${names[$i]}.times"
  done
done

echo "seconds over $SAMPLES samples (median, fastest, slowest), and what the last one compiled:"
for i in 0 1 2; do
  name=${names[$i]}
  stats=$(python3 -c 'import statistics, sys
times = [float(t) for t in open(sys.argv[1])]
print(f"{statistics.median(times):6.1f} {min(times):6.1f} {max(times):6.1f}")' "$LOGS/$name.times")
  built=$(sed -n 's/^ *\(Compiling\|Checking\) \([^ ]*\) .*/\2/p' "$LOGS/$SAMPLES-$name.log" | tr '\n' ' ')
  printf '%-8s %s  %s\n' "$name" "$stats" "$built"
done
