#!/usr/bin/env bash
# Samples CPU and PSS of the Rocket app and its WebKit helpers.
# Usage: rocket-pss.sh [seconds] [label]
secs=${1:-30}
label=${2:-sample}
root=$(pgrep -x rocket | head -1)
[ -z "$root" ] && { echo "rocket is not running"; exit 1; }
tree() { echo "$root"; pgrep -P "$root" -a | awk '{print $1}'; }
ticks() { for p in $(tree); do awk '{print $14+$15}' /proc/$p/stat 2>/dev/null; done | paste -sd+ | bc; }
pss() { for p in $(tree); do awk '/^Pss:/{print $2}' /proc/$p/smaps_rollup 2>/dev/null; done | paste -sd+ | bc; }
hz=$(getconf CLK_TCK)
n=0; sumcpu=0; peakcpu=0; sumpss=0; peakpss=0
prev=$(ticks)
for _ in $(seq "$secs"); do
  sleep 1
  cur=$(ticks); cpu=$(echo "($cur-$prev)*100/$hz" | bc -l); prev=$cur
  m=$(pss)
  n=$((n+1))
  sumcpu=$(echo "$sumcpu+$cpu" | bc -l); sumpss=$((sumpss+m))
  (( $(echo "$cpu>$peakcpu" | bc -l) )) && peakcpu=$cpu
  (( m>peakpss )) && peakpss=$m
done
printf "%s: %ss, CPU avg %.1f%% peak %.1f%% (100%% = one core), PSS avg %d MB peak %d MB\n" \
  "$label" "$n" "$(echo "$sumcpu/$n" | bc -l)" "$peakcpu" "$((sumpss/n/1024))" "$((peakpss/1024))"
