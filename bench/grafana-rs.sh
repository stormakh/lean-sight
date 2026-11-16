#!/bin/bash
# usage: grafana-rs.sh <data_dir> [port=3400] -> starts target/release/grafana-rs, prints "pid startup_ms"
set -e; R=$(cd "$(dirname "$0")/.." && pwd); D=$1; PORT=${2:-3400}; mkdir -p "$D"
t0=$(date +%s%N); GF_PATHS_DATA="$D" GF_SERVER_HTTP_PORT=$PORT setsid nohup "$R/target/release/grafana-rs" > "$D/stdout.log" 2>&1 & pid=$!
until curl -sf localhost:$PORT/api/health >/dev/null; do sleep 0.005; done; echo "$pid $(( ($(date +%s%N)-t0)/1000000 ))"
