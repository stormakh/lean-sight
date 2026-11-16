#!/bin/bash
# usage: seed.sh <base_url> [prometheus_url=http://localhost:9390] -> creates data source prom1 and dashboard bench1, as admin:admin
D=$(dirname "$0"); B=$1; P=${2:-http://localhost:9390}
curl -sf -u admin:admin -XPOST $B/api/datasources -H 'content-type: application/json' -d '{"name":"Prometheus","uid":"prom1","type":"prometheus","url":"'$P'","access":"proxy","isDefault":true,"jsonData":{"httpMethod":"POST"}}' >/dev/null || echo "seed: datasource failed"
curl -sf -u admin:admin -XPOST $B/api/dashboards/db -H 'content-type: application/json' -d @"$D/dash.json" >/dev/null || echo "seed: dashboard failed"
