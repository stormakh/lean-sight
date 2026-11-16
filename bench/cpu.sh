#!/bin/bash
# usage: cpu.sh <server_pid> <base_url> [requests=20000] -> user and kernel CPU ms per request for the 4 benchmark workloads
# Same requests as load.sh (oha, 16 at a time, basic auth admin:admin). Quick to run during development; load.sh is the frozen one.
P=$1; B=$2; N=${3:-20000}; NOW=$(date +%s)000
Q="{\"from\":\"$((NOW-900000))\",\"to\":\"$NOW\",\"queries\":[{\"refId\":\"A\",\"datasource\":{\"uid\":\"prom1\"},\"expr\":\"rate(prometheus_http_requests_total[1m])\",\"range\":true,\"intervalMs\":15000,\"maxDataPoints\":800}]}"
st() { awk '{print $14, $15}' /proc/$P/stat; }
one() { n=$1; shift; read u0 s0 < <(st); ~/.cargo/bin/oha --no-tui -n $N -c 16 "$@" >/dev/null; read u1 s1 < <(st)
  python3 -c "u=($u1-$u0)*10/$N; s=($s1-$s0)*10/$N; print(f'$n total={u+s:.4f} user={u:.4f} kernel={s:.4f} ms/req')"; }
one health $B/api/health
one search -a admin:admin "$B/api/search?query="
one dashboard -a admin:admin $B/api/dashboards/uid/bench1
one ds_query -a admin:admin -m POST -H 'content-type: application/json' -d "$Q" $B/api/ds/query
