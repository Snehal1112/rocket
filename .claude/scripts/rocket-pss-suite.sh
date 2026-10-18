#!/usr/bin/env bash
# Guided run of heavy scenarios. Start each action when told, keep doing it for the sample.
# Usage: rocket-pss-suite.sh [sample_seconds]
dir=$(cd "$(dirname "$0")" && pwd)
secs=${1:-60}
out=${ROCKET_PSS_OUT:-/tmp/rocket-pss-suite.txt}
: > "$out"
scenarios=(
  "baseline-idle|Do nothing. Leave the app idle."
  "large-collection|Open your largest collection and click through folders and requests."
  "many-tabs|Open 20+ request tabs and switch between them."
  "load-test|Start a load test and let it run."
  "flow-run|Run a Flow repeatedly."
  "after-release|Stop everything. Close extra tabs. Leave the app idle."
)
for s in "${scenarios[@]}"; do
  name=${s%%|*}; msg=${s#*|}
  echo ">>> NEXT: $name -- $msg"
  echo "    Starting sample in 10 seconds..."
  sleep 10
  "$dir/rocket-pss.sh" "$secs" "$name" | tee -a "$out"
done
echo; echo "=== Summary ==="; cat "$out"
