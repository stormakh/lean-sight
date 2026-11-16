#!/bin/bash
# usage: baseline.sh <label> <start command printing "pid startup_ms"> <base_url>
# Fresh start -> seed -> 2 min warm-up -> 5 min idle samples -> load test. Results in bench/results/<label>-*.
D=$(cd "$(dirname "$0")" && pwd); L=$1; START=$2; B=$3; O=$D/results
read pid ms < <(eval "$START"); echo "$L startup_ms=$ms pid=$pid" | tee "$O/$L-startup.txt"
"$D/seed.sh" $B; sleep 120
"$D/sample.sh" $pid 300 "$O/$L-idle.csv"
python3 - "$O/$L-idle.csv" <<'PY' | tee "$O/$L-idle-summary.txt"
import csv,sys; r=list(csv.DictReader(open(sys.argv[1]))); a,b=r[0],r[-1]; s=int(b["t"])-int(a["t"])
print(f"idle: pss_mb={int(b['pss_kb'])//1024} rss_mb={int(b['rss_kb'])//1024} procs={b['procs']} cpu_pct_of_core={(int(b['cpu_ticks'])-int(a['cpu_ticks']))/s:.3f} over {s}s")
PY
( while kill -0 $pid 2>/dev/null; do "$D/tree.sh" $pid | cut -d' ' -f2; sleep 1; done ) > "$O/$L-load-pss.txt" & mon=$!
"$D/load.sh" $pid $B $L; kill $mon
echo "peak_load_pss_mb=$(( $(sort -n "$O/$L-load-pss.txt" | tail -1) / 1024 ))" | tee -a "$O/$L-idle-summary.txt"
kill $pid; sleep 5
