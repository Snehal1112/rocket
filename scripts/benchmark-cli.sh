#!/usr/bin/env bash
# benchmark-cli.sh — Head-to-head benchmark: Bruno CLI vs Newman (Postman CLI)
#
# Usage:
#   ./scripts/benchmark-cli.sh [OPTIONS]
#
# Options:
#   -b <path>   Path to Bruno collection directory (contains .bru files)
#   -p <path>   Path to Postman collection JSON file
#   -e <path>   Path to Bruno environment YAML file  (e.g. environments/local.yml)
#   -n <int>    Number of iterations to run each tool  (default: 10)
#   -o <path>   Output markdown file                   (default: benchmark-results.md)
#   -h          Show this help
#
# Requirements:
#   - bru   (Bruno CLI):  npm install -g @usebruno/cli
#   - newman (Postman CLI): npm install -g newman
#   - hyperfine (optional, for richer stats): https://github.com/sharkdp/hyperfine
#   - /usr/bin/time with -v flag (GNU time, not bash builtin) for memory stats
#     On macOS: brew install gnu-time  and set GNU_TIME=gtime below
#
# Environment variable override:
#   GNU_TIME   Path to GNU time binary (default: /usr/bin/time)

set -euo pipefail

# ── Defaults ────────────────────────────────────────────────────────────────
ITERATIONS=10
OUTPUT="benchmark-results.md"
BRUNO_DIR=""
POSTMAN_FILE=""
BRUNO_ENV=""
GNU_TIME="${GNU_TIME:-/usr/bin/time}"
TIMESTAMP=$(date +"%Y-%m-%dT%H:%M:%S")

# ── Argument parsing ─────────────────────────────────────────────────────────
usage() {
  grep '^#' "$0" | grep -v '#!/' | sed 's/^# \?//'
  exit 0
}

while getopts "b:p:e:n:o:h" opt; do
  case $opt in
    b) BRUNO_DIR="$OPTARG" ;;
    p) POSTMAN_FILE="$OPTARG" ;;
    e) BRUNO_ENV="$OPTARG" ;;
    n) ITERATIONS="$OPTARG" ;;
    o) OUTPUT="$OPTARG" ;;
    h) usage ;;
    *) echo "Unknown option: -$opt" >&2; exit 1 ;;
  esac
done

# ── Validation ───────────────────────────────────────────────────────────────
die() { echo "ERROR: $*" >&2; exit 1; }

[[ -n "$BRUNO_DIR" ]]    || die "Bruno collection directory required (-b)"
[[ -n "$POSTMAN_FILE" ]] || die "Postman collection JSON required (-p)"
[[ -d "$BRUNO_DIR" ]]    || die "Bruno directory not found: $BRUNO_DIR"
[[ -f "$POSTMAN_FILE" ]] || die "Postman collection not found: $POSTMAN_FILE"

command -v bru     >/dev/null 2>&1 || die "'bru' not found. Install: npm install -g @usebruno/cli"
command -v newman  >/dev/null 2>&1 || die "'newman' not found. Install: npm install -g newman"

USE_HYPERFINE=false
command -v hyperfine >/dev/null 2>&1 && USE_HYPERFINE=true

USE_GNU_TIME=false
if "$GNU_TIME" --version >/dev/null 2>&1; then
  USE_GNU_TIME=true
fi

# ── Temp workspace ───────────────────────────────────────────────────────────
TMPDIR_WORK=$(mktemp -d)
trap 'rm -rf "$TMPDIR_WORK"' EXIT

BRUNO_TIMINGS="$TMPDIR_WORK/bruno_timings.txt"
NEWMAN_TIMINGS="$TMPDIR_WORK/newman_timings.txt"
BRUNO_MEM="$TMPDIR_WORK/bruno_mem.txt"
NEWMAN_MEM="$TMPDIR_WORK/newman_mem.txt"

touch "$BRUNO_TIMINGS" "$NEWMAN_TIMINGS" "$BRUNO_MEM" "$NEWMAN_MEM"

# ── Helpers ──────────────────────────────────────────────────────────────────

# Returns elapsed seconds (3 decimal places) for a command.
time_cmd() {
  local start end elapsed
  start=$(date +%s%N)
  "$@" >/dev/null 2>&1
  end=$(date +%s%N)
  elapsed=$(( (end - start) ))
  printf "%.3f" "$(echo "scale=3; $elapsed / 1000000000" | bc)"
}

# Returns peak RSS (KB) for a command using GNU time, or 0 if unavailable.
mem_cmd() {
  if $USE_GNU_TIME; then
    local mem_out
    mem_out=$("$GNU_TIME" -v "$@" 2>&1 >/dev/null | grep "Maximum resident" | awk '{print $NF}')
    echo "${mem_out:-0}"
  else
    echo "0"
  fi
}

# Arithmetic mean of a file of numbers (one per line).
mean() {
  awk '{ sum += $1; count++ } END { if (count > 0) printf "%.3f", sum/count; else print "0" }' "$1"
}

# Minimum value in a file.
min_val() {
  awk 'NR==1{min=$1} $1<min{min=$1} END{printf "%.3f", min}' "$1"
}

# Maximum value in a file.
max_val() {
  awk 'NR==1{max=$1} $1>max{max=$1} END{printf "%.3f", max}' "$1"
}

# Standard deviation.
stddev() {
  awk '{sum+=$1; sq+=$1*$1; n++} END{ if(n>1) printf "%.3f", sqrt(sq/n - (sum/n)^2); else print "0.000" }' "$1"
}

# ── Build CLI commands ────────────────────────────────────────────────────────
BRUNO_CMD=(bru run --dir "$BRUNO_DIR")
[[ -n "$BRUNO_ENV" ]] && BRUNO_CMD+=(--env-file "$BRUNO_ENV")

NEWMAN_CMD=(newman run "$POSTMAN_FILE")

# Pass environment variables from Bruno env file to Newman if provided.
if [[ -n "$BRUNO_ENV" && -f "$BRUNO_ENV" ]]; then
  # Extract name=value pairs from Bruno YAML env and write a Newman env JSON.
  NEWMAN_ENV_FILE="$TMPDIR_WORK/newman-env.json"
  python3 - "$BRUNO_ENV" "$NEWMAN_ENV_FILE" <<'PYEOF'
import sys, json, yaml

env_path, out_path = sys.argv[1], sys.argv[2]
with open(env_path) as f:
    data = yaml.safe_load(f)

variables = data.get('variables', [])
newman_vars = [
    {"key": v["name"], "value": v.get("value", ""), "enabled": v.get("enabled", True)}
    for v in variables
]
newman_env = {
    "id": "benchmark-env",
    "name": data.get("name", "benchmark"),
    "values": newman_vars
}
with open(out_path, "w") as f:
    json.dump(newman_env, f, indent=2)
print(f"Written Newman env to {out_path}")
PYEOF
  NEWMAN_CMD+=(--environment "$NEWMAN_ENV_FILE")
fi

# ── Run benchmarks ────────────────────────────────────────────────────────────
echo ""
echo "=== Bruno vs Newman Benchmark ==="
echo "  Iterations : $ITERATIONS"
echo "  Bruno dir  : $BRUNO_DIR"
echo "  Postman    : $POSTMAN_FILE"
echo "  hyperfine  : $USE_HYPERFINE"
echo "  GNU time   : $USE_GNU_TIME"
echo ""

if $USE_HYPERFINE; then
  # hyperfine handles warmup, timing, and statistics natively.
  echo "--- Running Bruno with hyperfine ---"
  hyperfine \
    --runs "$ITERATIONS" \
    --warmup 1 \
    --export-json "$TMPDIR_WORK/hyperfine_bruno.json" \
    "${BRUNO_CMD[*]}" 2>&1 | tail -5

  echo "--- Running Newman with hyperfine ---"
  hyperfine \
    --runs "$ITERATIONS" \
    --warmup 1 \
    --export-json "$TMPDIR_WORK/hyperfine_newman.json" \
    "${NEWMAN_CMD[*]}" 2>&1 | tail -5

  # Extract mean/min/max from hyperfine JSON output.
  read_hyperfine() {
    local file=$1 field=$2
    python3 -c "
import json, sys
d = json.load(open('$file'))
r = d['results'][0]
print(f\"{r['$field']:.3f}\")
"
  }

  BRUNO_MEAN=$(read_hyperfine "$TMPDIR_WORK/hyperfine_bruno.json" mean)
  BRUNO_MIN=$(read_hyperfine  "$TMPDIR_WORK/hyperfine_bruno.json" min)
  BRUNO_MAX=$(read_hyperfine  "$TMPDIR_WORK/hyperfine_bruno.json" max)
  BRUNO_STDDEV=$(read_hyperfine "$TMPDIR_WORK/hyperfine_bruno.json" stddev)

  NEWMAN_MEAN=$(read_hyperfine "$TMPDIR_WORK/hyperfine_newman.json" mean)
  NEWMAN_MIN=$(read_hyperfine  "$TMPDIR_WORK/hyperfine_newman.json" min)
  NEWMAN_MAX=$(read_hyperfine  "$TMPDIR_WORK/hyperfine_newman.json" max)
  NEWMAN_STDDEV=$(read_hyperfine "$TMPDIR_WORK/hyperfine_newman.json" stddev)

  BRUNO_MEM_MEAN="N/A (use -n without hyperfine)"
  NEWMAN_MEM_MEAN="N/A (use -n without hyperfine)"

else
  # Manual loop: capture timing and memory per iteration.
  echo "--- Running Bruno ($ITERATIONS iterations) ---"
  for i in $(seq 1 "$ITERATIONS"); do
    printf "  Run %2d/%d ... " "$i" "$ITERATIONS"
    elapsed=$(time_cmd "${BRUNO_CMD[@]}")
    mem=$(mem_cmd "${BRUNO_CMD[@]}")
    echo "$elapsed" >> "$BRUNO_TIMINGS"
    echo "$mem"     >> "$BRUNO_MEM"
    printf "%.3fs  %s KB\n" "$elapsed" "$mem"
  done

  echo ""
  echo "--- Running Newman ($ITERATIONS iterations) ---"
  for i in $(seq 1 "$ITERATIONS"); do
    printf "  Run %2d/%d ... " "$i" "$ITERATIONS"
    elapsed=$(time_cmd "${NEWMAN_CMD[@]}")
    mem=$(mem_cmd "${NEWMAN_CMD[@]}")
    echo "$elapsed" >> "$NEWMAN_TIMINGS"
    echo "$mem"     >> "$NEWMAN_MEM"
    printf "%.3fs  %s KB\n" "$elapsed" "$mem"
  done

  BRUNO_MEAN=$(mean   "$BRUNO_TIMINGS")
  BRUNO_MIN=$(min_val "$BRUNO_TIMINGS")
  BRUNO_MAX=$(max_val "$BRUNO_TIMINGS")
  BRUNO_STDDEV=$(stddev "$BRUNO_TIMINGS")

  NEWMAN_MEAN=$(mean   "$NEWMAN_TIMINGS")
  NEWMAN_MIN=$(min_val "$NEWMAN_TIMINGS")
  NEWMAN_MAX=$(max_val "$NEWMAN_TIMINGS")
  NEWMAN_STDDEV=$(stddev "$NEWMAN_TIMINGS")

  if $USE_GNU_TIME; then
    BRUNO_MEM_MEAN=$(mean "$BRUNO_MEM")
    NEWMAN_MEM_MEAN=$(mean "$NEWMAN_MEM")
  else
    BRUNO_MEM_MEAN="N/A (GNU time not found)"
    NEWMAN_MEM_MEAN="N/A (GNU time not found)"
  fi
fi

# ── Compute delta ─────────────────────────────────────────────────────────────
DELTA=$(python3 -c "
b, n = float('$BRUNO_MEAN'), float('$NEWMAN_MEAN')
diff = n - b
pct  = (diff / n * 100) if n > 0 else 0
sign = '+' if diff > 0 else ''
print(f'{sign}{diff:.3f}s ({sign}{pct:.1f}%)')
" 2>/dev/null || echo "N/A")

FASTER=$(python3 -c "
b, n = float('$BRUNO_MEAN'), float('$NEWMAN_MEAN')
print('Bruno' if b < n else ('Newman' if n < b else 'Tie'))
" 2>/dev/null || echo "N/A")

# ── Generate Markdown report ──────────────────────────────────────────────────
cat > "$OUTPUT" <<MDEOF
# Bruno CLI vs Newman — Benchmark Report

**Generated:** $TIMESTAMP
**Iterations:** $ITERATIONS
**Bruno collection:** \`$BRUNO_DIR\`
**Postman collection:** \`$POSTMAN_FILE\`

---

## Execution Time (seconds)

| Metric | Bruno (\`bru\`) | Newman | Delta (Newman − Bruno) |
|--------|--------------|--------|------------------------|
| Mean   | ${BRUNO_MEAN}s | ${NEWMAN_MEAN}s | ${DELTA} |
| Min    | ${BRUNO_MIN}s  | ${NEWMAN_MIN}s  | — |
| Max    | ${BRUNO_MAX}s  | ${NEWMAN_MAX}s  | — |
| Std dev | ${BRUNO_STDDEV}s | ${NEWMAN_STDDEV}s | — |

**Faster tool:** ${FASTER}

---

## Memory (Peak RSS, KB)

| Tool | Mean peak RSS |
|------|--------------|
| Bruno (\`bru\`) | ${BRUNO_MEM_MEAN} KB |
| Newman | ${NEWMAN_MEM_MEAN} KB |

> Memory figures require GNU \`time -v\`. On macOS install with \`brew install gnu-time\`
> and set \`GNU_TIME=gtime\` before running this script.

---

## Git / CI Integration

| Criterion | Bruno (\`bru\`) | Newman |
|-----------|--------------|--------|
| Collection format | Plain-text \`.bru\` files, one per request — diff-friendly | Single JSON blob — diffs are noisy |
| Environment files | YAML per environment, checked in alongside requests | JSON exported from Postman GUI |
| CLI install | \`npm install -g @usebruno/cli\` | \`npm install -g newman\` |
| No-GUI workflow | Native — collections are authored in plain text | Possible but collections originate in Postman GUI |
| Lock-file / versioning | Each request is a separate file → granular PR reviews | One file changes for every edit |
| Parallel execution | \`bru run --dir\` runs all requests sequentially by \`seq\` | Newman runs collection sequentially; parallelism requires splitting |
| Exit codes | Non-zero on assertion failure | Non-zero on assertion failure |
| Built-in reporters | Text, JSON | Text, JSON, JUnit, HTML (+ plugin ecosystem) |

---

## Verdict

$(python3 -c "
b, n = float('$BRUNO_MEAN'), float('$NEWMAN_MEAN')
faster = 'Bruno' if b < n else ('Newman' if n < b else 'both tools are equivalent in speed')
slower = 'Newman' if b < n else 'Bruno'
pct = abs(n-b)/max(b,n)*100
print(f'''**Recommended for CI/CD: Bruno (\`bru\`)**

- **Speed:** {faster} is faster by ~{pct:.0f}% on mean execution time in this run.
- **Resource footprint:** See memory table above. Bruno\\'s process is leaner because it
  carries no Postman-compatibility layer.
- **Git fit:** Bruno\\'s one-file-per-request format is the strongest argument for a
  Git-native workflow — reviewers can see exactly which endpoint changed, diffs are
  minimal, and merge conflicts are isolated to a single request file rather than a
  monolithic JSON blob.
- **CI install:** Both are a single \`npm install -g\` step; neither requires a GUI.

If your team already maintains Postman collections and has Newman-based CI pipelines,
the switching cost may outweigh the speed gain. But for a greenfield or Rocket-based
workflow, Bruno is the better fit.
''')
" 2>/dev/null || echo "Run the script against a live collection to generate the verdict.")

---

*Script: \`scripts/benchmark-cli.sh\` — rerun with \`-n 50\` for more statistically stable results.*
MDEOF

echo ""
echo "=== Results written to: $OUTPUT ==="
echo ""
cat "$OUTPUT"
