#!/bin/bash
# usage: load.sh <server_pid> <base_url> <label> -> appends CSV rows to results/load.csv
# Peak tree PSS is the max over the run. For each workload: 5000 requests, 16 at a time. CPU = server tree ticks (1 tick = 10 ms).
D=$(dirname "$0"); P=$1; B=$2; L=$3; N=5000; C=16; A='-a admin:admin'
mkdir -p "$D/results"; F="$D/results/load.csv"; [ -f "$F" ] || echo "label,workload,req_per_s,cpu_ms_per_req,rss_mb_after,p99_ms" > "$F"
NOW=$(date +%s)000
Q="{\"from\":\"$((NOW-900000))\",\"to\":\"$NOW\",\"queries\":[{\"refId\":\"A\",\"datasource\":{\"uid\":\"prom1\"},\"expr\":\"rate(prometheus_http_requests_total[1m])\",\"range\":true,\"intervalMs\":15000,\"maxDataPoints\":800}]}"
run() { name=$1; shift
  read r0 p0 c0 n0 < <("$D/tree.sh" $P)
  out=$(~/.cargo/bin/oha --no-tui -n $N -c $C --output-format json "$@")
  read r1 p1 c1 n1 < <("$D/tree.sh" $P)
  rps=$(echo "$out" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(round(d["summary"]["requestsPerSec"]))')
  p99=$(echo "$out" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(round(d["latencyPercentiles"]["p99"]*1000,2))')
  ok=$(echo "$out" | python3 -c 'import json,sys;d=json.load(sys.stdin);print(d["statusCodeDistribution"])')
  echo "$L,$name,$rps,$(python3 -c "print(round(($c1-$c0)*10/$N,3))"),$((p1/1024)),$p99" | tee -a "$F"; echo "  status: $ok"; [ "$ok" = "{'200': $N}" ] || { echo "FATAL: non-200 responses in $name"; exit 1; }
}
run health $B/api/health
run search $A "$B/api/search?query="
run dashboard $A $B/api/dashboards/uid/bench1
run ds_query $A -m POST -H 'content-type: application/json' -d "$Q" $B/api/ds/query
