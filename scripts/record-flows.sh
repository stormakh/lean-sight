#!/bin/bash
# Records API flows the UI tour does not cover, through the recording proxy. usage: record-flows.sh <proxy_base_url>
# Uses the default local admin:admin account of a throwaway test Grafana.
B=$1; J=$(mktemp); H='content-type: application/json'; c() { curl -s -o /dev/null -b $J -c $J "$@"; }
NOW=$(date +%s)000; FROM=$((NOW-3600000))
# login and session
c $B/login; c -XPOST $B/login -H "$H" -d '{"user":"admin","password":"wrong"}'
c -XPOST $B/login -H "$H" -d '{"user":"admin","password":"admin"}'
c $B/api/user; c $B/api/user/orgs; c $B/api/user/teams; c $B/api/user/preferences; c $B/api/org; c $B/api/org/users
c -XPUT $B/api/user/preferences -H "$H" -d '{"theme":"light","timezone":"utc","weekStart":"monday"}'; c $B/api/user/preferences
c -XPOST $B/api/user/stars/dashboard/uid/bench1; c $B/api/user/stars; c -XDELETE $B/api/user/stars/dashboard/uid/bench1
c $B/api/users/search; c $B/api/access-control/user/actions; c $B/profile; c $B/api/login/ping; c $B/api/frontend/settings
# data sources
c -XPOST $B/api/datasources -H "$H" -d '{"name":"Prom2","uid":"prom2","type":"prometheus","url":"http://localhost:9390","access":"proxy","jsonData":{"httpMethod":"GET"},"secureJsonData":{"basicAuthPassword":"x"}}'
c $B/api/datasources/uid/prom2; c $B/api/datasources/name/Prom2; c $B/api/datasources/uid/prom2/health; c $B/api/datasources/id/Prom2
c -XPUT $B/api/datasources/uid/prom2 -H "$H" -d '{"name":"Prom2","uid":"prom2","type":"prometheus","url":"http://localhost:9390","access":"proxy","jsonData":{"httpMethod":"POST","timeInterval":"10s"},"version":1}'
c -XPOST $B/api/datasources -H "$H" -d '{"name":"Prom2","type":"prometheus","url":"http://x"}'
c $B/api/datasources; c -XDELETE $B/api/datasources/uid/prom2; c $B/api/datasources/uid/prom2; c $B/api/datasources/uid/prom1/health
# prometheus plugin
for p in 'api/v1/labels' 'api/v1/label/job/values' 'api/v1/label/__name__/values' "api/v1/series?match%5B%5D=up&start=$((FROM/1000))&end=$((NOW/1000))" 'api/v1/metadata' 'api/v1/status/buildinfo'; do c "$B/api/datasources/uid/prom1/resources/$p"; done
q() { c -XPOST "$B/api/ds/query?ds_type=prometheus" -H "$H" -d "{\"from\":\"$FROM\",\"to\":\"$NOW\",\"queries\":[$1]}"; }
q '{"refId":"A","datasource":{"type":"prometheus","uid":"prom1"},"expr":"up","instant":true,"range":false,"intervalMs":15000,"maxDataPoints":500}'
q '{"refId":"A","datasource":{"type":"prometheus","uid":"prom1"},"expr":"rate(prometheus_http_requests_total[1m])","range":true,"legendFormat":"{{handler}}","intervalMs":15000,"maxDataPoints":500},{"refId":"B","datasource":{"type":"prometheus","uid":"prom1"},"expr":"sum(up)","range":true,"instant":true,"intervalMs":15000,"maxDataPoints":500}'
q '{"refId":"A","datasource":{"type":"prometheus","uid":"prom1"},"expr":"sum(rate(x[1m]","range":true,"intervalMs":15000,"maxDataPoints":500}'
q '{"refId":"A","datasource":{"type":"prometheus","uid":"prom1"},"expr":"up","range":true,"interval":"1m","intervalMs":15000,"maxDataPoints":100,"format":"table"}'
q '{"refId":"A","datasource":{"type":"prometheus","uid":"prom1"},"expr":"prometheus_http_request_duration_seconds_bucket","range":true,"format":"heatmap","intervalMs":15000,"maxDataPoints":200}'
q '{"refId":"A","datasource":{"type":"prometheus","uid":"nope"},"expr":"up","range":true,"intervalMs":15000,"maxDataPoints":100}'
# folders, dashboards, versions, conflicts, search, tags
c -XPOST $B/api/folders -H "$H" -d '{"uid":"fold1","title":"Team A"}'; c $B/api/folders; c $B/api/folders/fold1
c $B/apis/folder.grafana.app/v1/namespaces/default/folders/fold1; c $B/apis/folder.grafana.app/v1/namespaces/default/folders/fold1/parents; c $B/apis/folder.grafana.app/v1/namespaces/default/folders/fold1/access
c -XPOST $B/api/dashboards/db -H "$H" -d '{"dashboard":{"uid":"dash2","title":"Second","tags":["team","prod"],"panels":[]},"folderUid":"fold1"}'
c -XPOST $B/api/dashboards/db -H "$H" -d '{"dashboard":{"uid":"dash2","title":"Second v2","tags":["team"],"panels":[],"version":1},"folderUid":"fold1","message":"edit"}'
c -XPOST $B/api/dashboards/db -H "$H" -d '{"dashboard":{"uid":"dash2","title":"Stale","panels":[],"version":1},"folderUid":"fold1"}'
c -XPOST $B/api/dashboards/db -H "$H" -d '{"dashboard":{"title":"Second v2","panels":[]},"folderUid":"fold1"}'
c $B/api/dashboards/uid/dash2; c $B/api/dashboards/uid/dash2/versions; c $B/api/dashboards/uid/dash2/versions/1; c $B/api/dashboards/uid/missing
c $B/apis/dashboard.grafana.app/v1/namespaces/default/dashboards/dash2; c $B/apis/dashboard.grafana.app/v1/namespaces/default/dashboards/dash2/dto
c $B/apis/dashboard.grafana.app/v1/namespaces/default/dashboards; c $B/apis/dashboard.grafana.app/v1/namespaces/default/dashboards/missing/dto
c $B/api/dashboards/tags; c "$B/api/search?tag=team"; c "$B/api/search?query=sec&type=dash-db"; c "$B/api/search?folderUIDs=fold1"
c "$B/apis/dashboard.grafana.app/v0alpha1/namespaces/default/search?query=sec&limit=50"; c "$B/apis/dashboard.grafana.app/v0alpha1/namespaces/default/search?tag=team&limit=50"; c "$B/apis/dashboard.grafana.app/v0alpha1/namespaces/default/search?folder=fold1&limit=50"
c $B/api/dashboards/home
# annotations and query history
c -XPOST $B/api/annotations -H "$H" -d "{\"dashboardUID\":\"bench1\",\"panelId\":1,\"time\":$FROM,\"text\":\"deploy\",\"tags\":[\"x\"]}"
c "$B/api/annotations?from=$FROM&to=$NOW&dashboardUID=bench1"; c "$B/api/annotations?from=$FROM&to=$NOW&tags=x&type=annotation"
c -XPOST $B/api/query-history -H "$H" -d '{"datasourceUid":"prom1","queries":[{"refId":"A","expr":"up"}]}'; c "$B/api/query-history?datasourceUid=prom1"
# delete and logout
c -XDELETE $B/api/dashboards/uid/dash2; c -XDELETE $B/api/folders/fold1; c $B/logout; c $B/api/user
rm -f $J
