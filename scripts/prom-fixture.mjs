// Fixture Prometheus: answers the Prometheus HTTP API with the data that real Grafana showed in the recordings, so
// a replay can compare data frames exactly (live Prometheus data changes between runs).
//
// usage: node scripts/prom-fixture.mjs [--port 9391] [file.jsonl ...]   (default: both v1 recordings)
// To use it, seed prom1 with url http://localhost:9391 instead of :9390 (bench/seed.sh <url> after editing the url).
//
// For each recorded POST /api/ds/query, every result frame is turned back into the Prometheus answer it came from
// (matrix or vector, labels, "NaN"/"+Inf"/"-Inf" from `entities`; an `error` row becomes a 400 bad_data answer).
// A query_range is answered with the recorded series of the same expression and step whose samples fit in
// [start, end]; an instant query with the recorded vector at that time. Recorded resource calls
// (/api/datasources/uid/*/resources/api/v1/...) are served by path. Unknown requests get 404.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const args = process.argv.slice(2);
let port = 9391;
const files = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === '--port') port = Number(args[++i]);
  else files.push(args[i]);
}
if (!files.length) for (const f of ['ui-tour-v1', 'api-flows-v1']) files.push(path.join(root, `tests-data/recordings/${f}.jsonl`));

const queries = []; // {expr, kind: 'range'|'instant', stepMs, times: [ms], body}
const resources = new Map(); // "METHOD path?query" and "METHOD path" -> {status, body}

function promValue(v, idx, ent) {
  if (ent) {
    if (ent.NaN?.includes(idx)) return 'NaN';
    if (ent.Inf?.includes(idx)) return '+Inf';
    if (ent.NegInf?.includes(idx)) return '-Inf';
  }
  return String(v);
}

function toPrometheus(frames) {
  let resultType = 'matrix';
  const result = [];
  for (const f of frames) {
    const rt = f.schema?.meta?.custom?.resultType;
    if (rt) resultType = rt;
    const fields = f.schema?.fields || [];
    if (fields.length < 2) continue;
    const metric = fields[1].labels || {};
    const times = f.data.values[0], vals = f.data.values[1], ent = f.data.entities?.[1];
    const pairs = times.map((t, i) => [t / 1000, promValue(vals[i], i, ent)]);
    result.push(resultType === 'vector' ? { metric, value: pairs[0] } : { metric, values: pairs });
  }
  return { status: 'success', data: { resultType, result } };
}

for (const f of files) {
  for (const line of fs.readFileSync(f, 'utf8').split('\n')) {
    if (!line) continue;
    const row = JSON.parse(line);
    const p = row.url.split('?')[0];
    if (row.method === 'POST' && p === '/api/ds/query' && row.reqBody && row.resBody) {
      let req, res;
      try { req = JSON.parse(row.reqBody); res = JSON.parse(row.resBody); } catch { continue; }
      for (const q of req.queries || []) {
        const r = res.results?.[q.refId || 'A'];
        if (!r) continue;
        if (r.error) {
          const m = r.error.match(/^([a-z_]+): (.*)$/s);
          const body = m ? { status: 'error', errorType: m[1], error: m[2] } : { status: 'error', errorType: 'internal', error: r.error };
          queries.push({ expr: q.expr, kind: q.instant && !q.range ? 'instant' : 'range', stepMs: null, times: [], status: r.status || 400, body });
          continue;
        }
        const byType = { matrix: [], vector: [] };
        for (const fr of r.frames || []) {
          const rt = fr.schema?.meta?.custom?.resultType;
          if (rt === 'matrix' || rt === 'vector') byType[rt].push(fr);
          else if (!rt && (fr.schema?.fields || []).length === 0) byType[q.instant && !q.range ? 'vector' : 'matrix'].push(fr);
        }
        for (const [rt, frs] of Object.entries(byType)) {
          if (!frs.length) continue;
          const step = frs[0].schema.fields[0]?.config?.interval ?? null;
          const times = frs.flatMap((fr) => fr.data.values[0] || []);
          const body = toPrometheus(frs);
          body.data.resultType = rt;
          queries.push({ expr: q.expr, kind: rt === 'vector' ? 'instant' : 'range', stepMs: step, times, status: 200, body });
        }
      }
    }
    const m = p.match(/^\/api\/datasources\/uid\/[^/]+\/resources\/(.*)$/);
    if (m && row.resBody !== undefined) {
      const q = row.url.includes('?') ? row.url.slice(row.url.indexOf('?') + 1) : '';
      const v = { status: row.status, body: row.resBody };
      resources.set(`${row.method} /${m[1]}?${q}`, v);
      if (!resources.has(`${row.method} /${m[1]}`)) resources.set(`${row.method} /${m[1]}`, v);
      // GET-only resources may be asked with POST when the data source uses POST
      if (!resources.has(`POST /${m[1]}`)) resources.set(`POST /${m[1]}`, v);
    }
  }
}

function pick(expr, kind, start, end, stepMs) {
  const c = queries.filter((q) => q.expr === expr && q.kind === kind);
  const err = c.find((q) => q.status !== 200);
  if (err) return err;
  let best = null, bestScore = Infinity;
  for (const q of c) {
    if (kind === 'range' && stepMs !== null && q.stepMs !== null && q.stepMs !== stepMs) continue;
    if (!q.times.length) { if (!best) best = q; continue; }
    const lo = Math.min(...q.times), hi = Math.max(...q.times);
    if (lo < start - 1 || hi > end + 1) continue;
    const score = Math.abs(lo - start) + Math.abs(hi - end);
    if (score < bestScore) { best = q; bestScore = score; }
  }
  return best;
}

function send(res, status, obj) {
  const b = typeof obj === 'string' ? obj : JSON.stringify(obj);
  res.writeHead(status, { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(b) });
  res.end(b);
}

const server = http.createServer((req, res) => {
  let body = '';
  req.on('data', (c) => (body += c));
  req.on('end', () => {
    const u = new URL(req.url, 'http://x');
    const params = new URLSearchParams(u.search);
    if (req.method === 'POST' && (req.headers['content-type'] || '').includes('form')) for (const [k, v] of new URLSearchParams(body)) params.append(k, v);
    const expr = params.get('query');
    if (u.pathname === '/api/v1/query_range') {
      const start = Number(params.get('start')) * 1000, end = Number(params.get('end')) * 1000, step = Number(params.get('step')) * 1000;
      const q = pick(expr, 'range', start, end, step);
      if (!q) return send(res, 200, { status: 'success', data: { resultType: 'matrix', result: [] } });
      return send(res, q.status, q.body);
    }
    if (u.pathname === '/api/v1/query') {
      if (expr === '1+1') return send(res, 200, { status: 'success', data: { resultType: 'scalar', result: [Number(params.get('time')), '2'] } });
      const t = Number(params.get('time')) * 1000;
      const q = pick(expr, 'instant', t, t, null);
      if (!q) return send(res, 200, { status: 'success', data: { resultType: 'vector', result: [] } });
      return send(res, q.status, q.body);
    }
    const r = resources.get(`${req.method} ${u.pathname}?${u.search.slice(1)}`) || resources.get(`${req.method} ${u.pathname}`);
    if (r) return send(res, r.status, r.body);
    send(res, 404, { status: 'error', errorType: 'not_found', error: `fixture has no ${req.method} ${u.pathname}` });
  });
});
server.listen(port, '127.0.0.1', () => console.log(`prom-fixture: ${queries.length} recorded answers, ${resources.size} resources, on :${port}`));
