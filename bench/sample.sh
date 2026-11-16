#!/bin/bash
# usage: sample.sh <pid> <seconds> <outfile> -> CSV every 5 s over the whole process tree
D=$(dirname "$0"); echo "t,rss_kb,pss_kb,cpu_ticks,procs" > "$3"
for ((i=0;i<$2;i+=5)); do [ -d /proc/$1 ] || break; echo "$i,$("$D/tree.sh" $1 | tr ' ' ',')" >> "$3"; sleep 5; done
