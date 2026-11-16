#!/bin/bash
# usage: grafana.sh <default|trimmed|capture|capture-v1|login-v1> <data_dir> [port=3300] -> starts real Grafana 13.3.1, prints "pid startup_ms"
set -e; R=$(cd "$(dirname "$0")/.." && pwd); H=$R/upstream-bin/grafana-13.3.1; M=$1; D=$2; PORT=${3:-3300}; mkdir -p "$D"
export GF_SERVER_HTTP_PORT=$PORT GF_PATHS_DATA="$D" GF_PATHS_LOGS="$D/log" GF_PATHS_PLUGINS="$H/data/plugins" \
  GF_ANALYTICS_REPORTING_ENABLED=false GF_ANALYTICS_CHECK_FOR_UPDATES=false GF_ANALYTICS_CHECK_FOR_PLUGIN_UPDATES=false
if [ "$M" = trimmed ]; then
  # Only what grafana-rs ports: no alerting, no live, no plugin installs, no other data-source plugins.
  export GF_UNIFIED_ALERTING_ENABLED=false GF_LIVE_MAX_CONNECTIONS=0 GF_PLUGINS_PREINSTALL_DISABLED=true \
    GF_PLUGINS_DISABLE_PLUGINS=grafana-postgresql-datasource,grafana-pyroscope-datasource,influxdb,jaeger,loki,mssql,mysql,opentsdb,tempo,zipkin,elasticsearch,cloudwatch,stackdriver,grafana-azure-monitor-datasource,graphite,grafana-testdata-datasource
fi
case "$M" in capture*) export GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin;; esac
# capture-v1: the classic dashboard API path that grafana-rs implements
EXTRA=""; case "$M" in capture-v1|login-v1) EXTRA="--config $R/bench/grafana-v1.ini";; esac
t0=$(date +%s%N); cd "$H"; setsid nohup ./bin/grafana server --homepath . $EXTRA > "$D/stdout.log" 2>&1 & pid=$!
until curl -sf localhost:$PORT/api/health >/dev/null; do sleep 0.05; done; echo "$pid $(( ($(date +%s%N)-t0)/1000000 ))"
