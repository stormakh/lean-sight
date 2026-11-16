#!/bin/bash
# Runs the frozen Grafana baselines one after the other, each from a fresh data dir, on :3300.
D=$(cd "$(dirname "$0")" && pwd); mkdir -p $D/run
for m in default trimmed; do rm -rf "${D:?}/run/base-$m"; $D/baseline.sh grafana-$m "$D/grafana.sh $m $D/run/base-$m 3300" http://localhost:3300; done
echo BASELINES_DONE
