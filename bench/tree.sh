#!/bin/bash
# usage: tree.sh <pid> -> "rss_kb pss_kb cpu_ticks procs" summed over pid and all its descendants
all=$1; q=$1
while [ -n "$q" ]; do q=$(pgrep -P "$(echo $q | tr ' ' ',')" | tr '\n' ' '); all="$all $q"; done
r=0; p=0; c=0; n=0
for x in $all; do
  [ -r /proc/$x/stat ] || continue
  n=$((n+1))
  r=$((r + $(awk '/^VmRSS/{print $2}' /proc/$x/status)))
  p=$((p + $(awk '/^Pss:/{print $2}' /proc/$x/smaps_rollup)))
  c=$((c + $(awk '{print $14+$15}' /proc/$x/stat)))
done
echo "$r $p $c $n"
