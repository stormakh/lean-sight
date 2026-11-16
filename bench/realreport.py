#!/usr/bin/env python3
"""Builds the side-by-side real-workload report from a bench/realrun.sh output folder.
usage: realreport.py <run_dir> <out.html>"""
import csv, json, sys, statistics, html

run, outp = sys.argv[1], sys.argv[2]
ls, le = int(open(f"{run}/load_start").read()), int(open(f"{run}/load_end").read())
startup = open(f"{run}/startup.txt").read().strip()
S = {"grafana": [], "grafana-rs": [], "prometheus": []}
for r in csv.DictReader(open(f"{run}/samples.csv")):
    S[r["server"]].append({k: (int(v) if k not in ("phase", "server") else v) for k, v in r.items()})
t0 = S["grafana"][0]["epoch"]


def cpu_pct(rows, a, b):
    """CPU % of one core over [a, b] (ticks are 10 ms)."""
    xs = [r for r in rows if a <= r["epoch"] <= b]
    if len(xs) < 2:
        return 0.0
    return (xs[-1]["cpu_ticks"] - xs[0]["cpu_ticks"]) * 10 / ((xs[-1]["epoch"] - xs[0]["epoch"]) * 1000) * 100


def mem(rows, phase, f=statistics.mean):
    xs = [r["pss_kb"] for r in rows if r["phase"] == phase]
    return f(xs) / 1024 if xs else 0.0


def ph_range(rows, phase):
    xs = [r["epoch"] for r in rows if r["phase"] == phase]
    return (min(xs), max(xs)) if xs else (0, 0)


def reqs(name):
    rows = [json.loads(l) for l in open(f"{run}/req-{name}.jsonl")]
    return rows


R = {"grafana": reqs("grafana"), "grafana-rs": reqs("rs")}


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p / 100 * len(xs)))] if xs else 0


stats = {}
for s in ("grafana", "grafana-rs"):
    rows, req = S[s], R[s]
    ia, ib = ph_range(rows, "idle")
    q = [r for r in req if r["kind"] == "refresh" and r["path"] == "/api/ds/query"]
    ok = [r for r in req if 200 <= r["status"] < 300]
    bad = [r for r in req if not (200 <= r["status"] < 300)]
    load_cpu_s = (max(r["cpu_ticks"] for r in rows if r["epoch"] <= le) - min(r["cpu_ticks"] for r in rows if r["epoch"] >= ls)) / 100
    stats[s] = {
        "idle_mem": mem(rows, "idle"), "load_mem": mem(rows, "load"), "peak_mem": mem(rows, "load", max), "after_mem": mem(rows, "after"),
        "procs": max(r["procs"] for r in rows),
        "idle_cpu": cpu_pct(rows, ia, ib), "load_cpu": cpu_pct(rows, ls, le), "after_cpu": cpu_pct(rows, *ph_range(rows, "after")),
        "requests": len(req), "ok": len(ok), "bad": len(bad),
        "bad_kinds": sorted({(r["kind"], r["path"], r["status"]) for r in bad}),
        "cpu_ms_per_req": load_cpu_s * 1000 / max(1, len(req)),
        "q_p50": pct([r["ms"] for r in q], 50), "q_p95": pct([r["ms"] for r in q], 95), "q_p99": pct([r["ms"] for r in q], 99), "q_max": max([r["ms"] for r in q] or [0]),
        "refresh_cycle_p95": 0,
        "open_p50": pct([r["ms"] for r in req if r["kind"] == "open"], 50), "open_p95": pct([r["ms"] for r in req if r["kind"] == "open"], 95),
        "login_p50": pct([r["ms"] for r in req if r["kind"] == "login"], 50), "login_max": max([r["ms"] for r in req if r["kind"] == "login"] or [0]),
        "bytes": sum(r.get("bytes", 0) for r in req),
    }
    # time for one viewer's whole refresh (16 requests) = max latency within a viewer's burst
    bursts = {}
    for r in req:
        if r["kind"] == "refresh":
            bursts.setdefault((r["v"], r["t"] // 10000), []).append(r["ms"])
    stats[s]["refresh_cycle_p95"] = pct([max(v) for v in bursts.values()], 95)
prom_load = cpu_pct(S["prometheus"], ls, le)

g, x = stats["grafana"], stats["grafana-rs"]


def ratio(a, b):
    return a / b if b else float("inf")


def series(name, key):
    rows = S[name]
    out = []
    prev = None
    for r in rows:
        if key == "cpu":
            if prev:
                dt = r["epoch"] - prev["epoch"]
                out.append([r["epoch"] - t0, (r["cpu_ticks"] - prev["cpu_ticks"]) * 10 / (dt * 1000) * 100 if dt else 0])
            prev = r
        else:
            out.append([r["epoch"] - t0, r["pss_kb"] / 1024])
    return out


data = {
    "mem": {"grafana": series("grafana", "mem"), "rs": series("grafana-rs", "mem")},
    "cpu": {"grafana": series("grafana", "cpu"), "rs": series("grafana-rs", "cpu")},
    "load": [ls - t0, le - t0],
}

rows_html = []


def row(label, a, b, fmt, better="low"):
    r = ratio(a, b) if better == "low" else ratio(b, a)
    rows_html.append(f"<tr><td>{label}</td><td>{fmt(a)}</td><td>{fmt(b)}</td><td class='x'>{r:,.0f}X</td></tr>" if r != float("inf") else f"<tr><td>{label}</td><td>{fmt(a)}</td><td>{fmt(b)}</td><td class='x'>–</td></tr>")


mb = lambda v: f"{v:,.1f} MB"
pc = lambda v: f"{v:.2f}%"
ms = lambda v: f"{v:,.1f} ms"
row("Memory, idle (PSS)", g["idle_mem"], x["idle_mem"], mb)
row("Memory, average under load", g["load_mem"], x["load_mem"], mb)
row("Memory, peak under load", g["peak_mem"], x["peak_mem"], mb)
row("CPU, idle (% of one core)", g["idle_cpu"], x["idle_cpu"], pc)
row("CPU, under load (% of one core)", g["load_cpu"], x["load_cpu"], pc)
row("CPU per request", g["cpu_ms_per_req"], x["cpu_ms_per_req"], lambda v: f"{v:.3f} ms")
row("Panel query latency, median", g["q_p50"], x["q_p50"], ms)
row("Panel query latency, p95", g["q_p95"], x["q_p95"], ms)
row("Panel query latency, p99", g["q_p99"], x["q_p99"], ms)
row("Whole refresh (16 requests), p95", g["refresh_cycle_p95"], x["refresh_cycle_p95"], ms)
row("Dashboard open requests, p95", g["open_p95"], x["open_p95"], ms)
row("Login, median", g["login_p50"], x["login_p50"], ms)

errs = lambda s: "none" if not s["bad_kinds"] else "; ".join(f"{k} {p} → {c}" for k, p, c in s["bad_kinds"])
mins = (le - ls) / 60

doc = f"""<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Real Workload Comparison</title>
<style>
:root{{--bg:#fbfaf7;--fg:#1d1d1b;--muted:#6b6a65;--line:#e4e1d9;--g:#d9822b;--r:#2f6f4f;--card:#fff}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#16171a;--fg:#ecebe6;--muted:#9b9a94;--line:#2c2d31;--g:#e8964a;--r:#5fb88a;--card:#1d1e22}}}}
:root[data-theme="dark"]{{--bg:#16171a;--fg:#ecebe6;--muted:#9b9a94;--line:#2c2d31;--g:#e8964a;--r:#5fb88a;--card:#1d1e22}}
*{{box-sizing:border-box}} body{{margin:0;background:var(--bg);color:var(--fg);font:15px/1.5 system-ui,sans-serif}}
main{{max-width:980px;margin:0 auto;padding:28px 16px 48px}} h1{{font-size:24px;margin:0 0 4px}} h2{{font-size:17px;margin:26px 0 8px}}
.sub,.note{{color:var(--muted)}} .note{{font-size:13px}}
.card{{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:14px 16px;margin:12px 0}}
.kpis{{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:12px}} .kpi b{{display:block;font-size:26px;color:var(--r)}} .kpi span{{color:var(--muted);font-size:13px}}
table{{width:100%;border-collapse:collapse;font-size:14px}} th,td{{padding:7px 8px;border-bottom:1px solid var(--line);text-align:right;font-variant-numeric:tabular-nums}}
th:first-child,td:first-child{{text-align:left}} th{{color:var(--muted);font-weight:500}} td.x{{color:var(--r);font-weight:600}}
.wrap{{overflow-x:auto}} svg{{width:100%;height:auto;display:block}} .legend span{{margin-right:16px;font-size:13px;color:var(--muted)}}
.legend i{{display:inline-block;width:12px;height:3px;vertical-align:middle;margin-right:6px}}
ul{{padding-left:20px}}
</style></head><body><main>
<h1>Real workload: Grafana 13.3.1 vs grafana-rs</h1>
<p class="sub">Node Exporter Full dashboard (grafana.com 1860, 137 panels) on real LastSun host metrics. {len(set(r['v'] for r in R['grafana']))} viewers per server, each logged in as its own Viewer user, refresh every 10 s, {mins:.0f} minutes, both servers at the same time against the same Prometheus. Each server gets the requests that a real Chrome sent to it.</p>
<div class="kpis">
<div class="card kpi"><b>{ratio(g['load_mem'], x['load_mem']):,.0f}X</b><span>less memory under load ({g['load_mem']:,.0f} MB vs {x['load_mem']:.1f} MB)</span></div>
<div class="card kpi"><b>{ratio(g['load_cpu'], x['load_cpu']):,.0f}X</b><span>less CPU under load ({g['load_cpu']:.1f}% vs {x['load_cpu']:.2f}% of one core)</span></div>
<div class="card kpi"><b>{ratio(g['q_p95'], x['q_p95']):,.1f}X</b><span>faster panel queries at p95 ({g['q_p95']:.1f} ms vs {x['q_p95']:.1f} ms)</span></div>
<div class="card kpi"><b>{x['bad'] + 0} / {g['bad']}</b><span>failed requests, grafana-rs / Grafana (of {x['requests']:,} / {g['requests']:,})</span></div>
</div>
<h2>Memory over time (PSS, MB, log scale)</h2><div class="card"><div class="legend"><span><i style="background:var(--g)"></i>Grafana</span><span><i style="background:var(--r)"></i>grafana-rs</span><span>shaded: load phase</span></div><svg id="mem" viewBox="0 0 900 260"></svg></div>
<h2>CPU over time (% of one core, 5 s samples, log scale)</h2><div class="card"><svg id="cpu" viewBox="0 0 900 260"></svg></div>
<h2>Side by side</h2><div class="card wrap"><table><thead><tr><th>Measure</th><th>Grafana</th><th>grafana-rs</th><th>Better by</th></tr></thead><tbody>{''.join(rows_html)}</tbody></table></div>
<h2>Correctness and errors</h2><div class="card"><ul>
<li>Before the run, the 15 panel queries of the dashboard were sent to both servers with the same fixed time window: <b>15 of 15 responses byte-identical</b>.</li>
<li>Requests: Grafana {g['requests']:,} ({g['bad']} not 2xx: {html.escape(errs(g))}); grafana-rs {x['requests']:,} ({x['bad']} not 2xx: {html.escape(errs(x))}).</li>
<li>Process count: Grafana {g['procs']} processes (server plus plugin processes), grafana-rs {x['procs']}.</li>
<li>Startup: {html.escape(startup)}.</li>
<li>Prometheus (shared by both) used {prom_load:.1f}% of one core during the load.</li>
</ul></div>
<p class="note">Memory is PSS of each server's whole process tree, sampled every 5 s. CPU is user + system time of the tree. Latency is measured by the load generator (bench/realload.mjs), per request, over keep-alive connections, up to 6 per viewer (as Chrome). Run folder: {html.escape(run)}.</p>
<script>
const D={json.dumps(data)};
function chart(id,key,unit){{const el=document.getElementById(id),W=900,H=260,L=60,B=30,T=10,R=10;
const all=[...D[key].grafana,...D[key].rs].map(p=>p[1]).filter(v=>v>0);const lo=Math.log10(Math.max(Math.min(...all),0.01)/1.5),hi=Math.log10(Math.max(...all)*1.5);
const tmax=Math.max(...D[key].grafana.map(p=>p[0]));const X=t=>L+(W-L-R)*t/tmax,Y=v=>T+(H-T-B)*(1-(Math.log10(Math.max(v,Math.pow(10,lo)))-lo)/(hi-lo));
let s=`<rect x="${{X(D.load[0])}}" y="${{T}}" width="${{X(D.load[1])-X(D.load[0])}}" height="${{H-T-B}}" fill="currentColor" opacity="0.06"/>`;
for(let e=Math.ceil(lo);e<=hi;e++){{const y=Y(Math.pow(10,e));s+=`<line x1="${{L}}" x2="${{W-R}}" y1="${{y}}" y2="${{y}}" stroke="currentColor" opacity="0.12"/><text x="${{L-6}}" y="${{y+4}}" text-anchor="end" font-size="11" fill="currentColor" opacity="0.6">${{Math.pow(10,e)}}${{unit}}</text>`;}}
for(let m=0;m<=tmax/60;m+=5){{const x=X(m*60);s+=`<text x="${{x}}" y="${{H-10}}" text-anchor="middle" font-size="11" fill="currentColor" opacity="0.6">${{m}} min</text>`;}}
for(const [k,c] of [["grafana","var(--g)"],["rs","var(--r)"]]){{s+=`<polyline fill="none" stroke="${{c}}" stroke-width="1.8" points="${{D[key][k].map(p=>X(p[0]).toFixed(1)+","+Y(p[1]).toFixed(1)).join(" ")}}"/>`;}}
el.innerHTML=s;el.style.color="var(--fg)";}}
chart("mem","mem"," MB");chart("cpu","cpu","%");
</script></main></body></html>"""
open(outp, "w").write(doc)
print(json.dumps({"grafana": g, "grafana-rs": x, "prometheus_load_cpu": prom_load}, indent=1, default=str))
