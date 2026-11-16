#!/bin/bash
# usage: seed.sh <base_url> -> creates data source prom1 (Prometheus on :9390) and dashboard bench1, as admin:admin
D=$(dirname "$0"); B=$1
curl -sf -u admin:admin -XPOST $B/api/datasources -H 'content-type: application/json' -d '{"name":"Prometheus","uid":"prom1","type":"prometheus","url":"http://localhost:9390","access":"proxy","isDefault":true,"jsonData":{"httpMethod":"POST"}}' >/dev/null || echo "seed: datasource failed"
curl -sf -u admin:admin -XPOST $B/api/dashboards/db -H 'content-type: application/json' -d @"$D/dash.json" >/dev/null || echo "seed: dashboard failed"
