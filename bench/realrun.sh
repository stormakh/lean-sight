#!/bin/bash
# Side-by-side real-workload run: real Grafana and grafana-rs at the same time, same Prometheus, same dashboard.
# usage: realrun.sh <out_dir> <grafana_data_dir> <rs_data_dir> <rec_grafana.jsonl> <rec_rs.jsonl> <users.json> [viewers=20] [minutes=30]
# Both servers restart fresh on their data dirs (anonymous Viewer on, as in the recording), warm up 60 s, idle 60 s,
# then <viewers> viewers per server for <minutes>, then 120 s after the load. Every 5 s it samples both process trees
# and Prometheus: epoch, phase, rss_kb, pss_kb, cpu_ticks, procs.
set -u
D=$(cd "$(dirname "$0")" && pwd); O=$1; G=$2; X=$3; RG=$4; RX=$5; U=$6; V=${7:-20}; M=${8:-30}
mkdir -p "$O"
export GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Viewer
read gp gms < <("$D/grafana.sh" default "$G" 3300); read xp xms < <("$D/grafana-rs.sh" "$X" 3400)
echo "grafana pid=$gp startup_ms=$gms; grafana-rs pid=$xp startup_ms=$xms" | tee "$O/startup.txt"
echo "$gp $xp" > "$O/pids"
PP=$(pgrep -f "prometheus --config.file=prom.yml" | head -1)
phase=warmup; echo $phase > "$O/phase"
( echo "epoch,phase,server,rss_kb,pss_kb,cpu_ticks,procs"
  while [ -d /proc/$gp ] && [ -d /proc/$xp ]; do e=$(date +%s); ph=$(cat "$O/phase")
    for s in "grafana $gp" "grafana-rs $xp" "prometheus $PP"; do set -- $s; echo "$e,$ph,$1,$("$D/tree.sh" $2 | tr ' ' ',')"; done
    sleep 5; done ) > "$O/samples.csv" &
smp=$!
sleep 60; echo idle > "$O/phase"; sleep 60
echo load > "$O/phase"; date +%s > "$O/load_start"
node "$D/realload.mjs" "$RG" http://localhost:3300 "$U" $V $M "$O/req-grafana.jsonl" 2> "$O/load-grafana.log" &
l1=$!
node "$D/realload.mjs" "$RX" http://localhost:3400 "$U" $V $M "$O/req-rs.jsonl" 2> "$O/load-rs.log" &
l2=$!
wait $l1 $l2; date +%s > "$O/load_end"
echo after > "$O/phase"; sleep 120
kill $smp; echo done > "$O/phase"; kill $gp $xp; wait $gp $xp 2>/dev/null
echo RUN_DONE
