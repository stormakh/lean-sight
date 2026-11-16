// Replay scorer: replays recorded UI traffic against a server and compares each response with the recording.
//
// usage: node scripts/replay.mjs [--target http://localhost:3400] [--seed] [--verbose] [--only <substring>] [file.jsonl ...]
//   default file: tests-data/recordings/ui-tour-v1.jsonl. Several files replay in the order given, sharing ids and cookies.
//
// The recordings were made with anonymous Admin access (bench/grafana.sh capture-v1), so start the target the same way:
//   GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/grafana-rs
// and seed it like the recording (--seed does it, with admin:admin): data source uid prom1 -> http://localhost:9390,
// dashboard uid bench1 from bench/dash.json.
//
// A row matches when the status is equal and the body matches:
//   JSON: same value types, same object keys in the same order, equal values, recursively.
//   HTML: the window.grafanaBootData parts (user, settings, navTree, assets) have the same structure
//         (keys, key order, types, array lengths); values are not compared.
//   other text: equal.
// Exceptions live in tests-data/replay-ignore.json, each with a reason. Rule modes:
// A rule: {route: "METHOD /path/with/*/or/**", path: "a.*.b" or ["a", "key.with.dots"], mode, reason}.
//   "value": the value may differ, the type must not.          "any": the value and type may differ (or be missing).
//   "map":   a generated id: the first recorded->actual pair is remembered and replaced in later URLs and bodies,
//            and later comparisons of that recorded value expect the mapped value.
// Cookies: the target's own Set-Cookie values are sent on rows that had a cookie in the recording (login flows).
// UPGRADE rows (websocket) are skipped. Every other row counts, in scope or not.
import fs from 'node:fs';
import path from 'node:path';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const args = process.argv.slice(2);
let target = 'http://localhost:3400';
let seed = false, verbose = false, only = '';
const files = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '--target') target = args[++i];
  else if (a === '--seed') seed = true;
  else if (a === '--verbose' || a === '-v') verbose = true;
  else if (a === '--only') only = args[++i];
  else files.push(a);
}
if (!files.length) files.push(path.join(root, 'tests-data/recordings/ui-tour-v1.jsonl'));
target = target.replace(/\/$/, '');

const ignoreFile = path.join(root, 'tests-data/replay-ignore.json');
const rules = fs.existsSync(ignoreFile) ? JSON.parse(fs.readFileSync(ignoreFile, 'utf8')).rules || [] : [];
for (const r of rules) {
  if (!r.route || !r.path || !r.mode || !r.reason) throw new Error(`replay-ignore.json: rule needs route, path, mode, reason: ${JSON.stringify(r)}`);
  if (!['value', 'any', 'map'].includes(r.mode)) throw new Error(`replay-ignore.json: bad mode ${r.mode}`);
  const [m, p] = r.route.split(' ');
  r._method = m;
  r._re = new RegExp('^' + p.split('/').map((s) => (s === '**' ? '.*' : s === '*' ? '[^/]+' : s.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))).join('/') + '$');
  r._path = Array.isArray(r.path) ? r.path.map(String) : r.path.split('.');
}

// ---- id mapping ----
const idMap = new Map(); // recorded string -> actual string
function applyMap(s) {
  if (typeof s !== 'string' || !idMap.size) return s;
  for (const [from, to] of idMap) if (from.length >= 4) s = s.split(from).join(to);
  return s;
}

// ---- cookies: the target's Set-Cookie values are sent on rows that carried a cookie in the recording ----
const jar = new Map();
function cookieFor(recorded) {
  const names = recorded.split(';').map((p) => p.split('=')[0].trim());
  const out = names.filter((n) => jar.has(n)).map((n) => `${n}=${jar.get(n)}`);
  return out.length ? out.join('; ') : undefined;
}

// ---- helpers ----
const routeKey = (method, url) => {
  const p = url.split('?')[0];
  const norm = p
    .replace(/\/d\/[^/]+(\/[^/]+)?$/, '/d/:uid/:slug')
    .replace(/(dashboards|folders|datasources\/uid|query-history)\/[A-Za-z0-9_-]{6,}(?=\/|$)/, '$1/:uid')
    .replace(/\/plugins\/[a-z0-9-]+\/settings$/, '/plugins/:id/settings');
  return `${method} ${norm}`;
};
const typeOf = (v) => (v === null ? 'null' : Array.isArray(v) ? 'array' : typeof v);

function rulesFor(method, url) {
  const p = url.split('?')[0];
  return rules.filter((r) => (r._method === '*' || r._method === method) && r._re.test(p));
}
function ruleAt(rs, jpath) {
  for (const r of rs) {
    if (r._path.length !== jpath.length) continue;
    let ok = true;
    for (let i = 0; i < jpath.length; i++) if (r._path[i] !== '*' && r._path[i] !== String(jpath[i])) { ok = false; break; }
    if (ok) return r;
  }
  return null;
}

// compares recorded `a` with actual `b`; returns the first difference as a string, or null
function cmp(a, b, rs, jp, structOnly) {
  const r = ruleAt(rs, jp);
  const where = '$' + (jp.length ? '.' + jp.join('.') : '');
  if (r && r.mode === 'any') return null;
  const ta = typeOf(a), tb = typeOf(b);
  if (r && r.mode === 'map') {
    if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
    const ka = String(a), kb = String(b);
    if (idMap.has(ka)) return idMap.get(ka) === kb ? null : `${where}: ${JSON.stringify(b)}, want mapped ${JSON.stringify(idMap.get(ka))}`;
    if (ka !== kb) idMap.set(ka, kb);
    return null;
  }
  if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
  if (ta === 'array') {
    if (a.length !== b.length && !(r && r.mode === 'value')) return `${where}: length ${b.length}, want ${a.length}`;
    const n = Math.min(a.length, b.length);
    for (let i = 0; i < n; i++) { const d = cmp(a[i], b[i], rs, [...jp, i], structOnly); if (d) return d; }
    return null;
  }
  if (ta === 'object') {
    const ka = Object.keys(a), kb = Object.keys(b);
    const missing = ka.filter((k) => !(k in b) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
    if (missing.length) return `${where}: missing keys ${missing.slice(0, 5).join(',')}`;
    const extra = kb.filter((k) => !(k in a) && !(ruleAt(rs, [...jp, k])?.mode === 'any'));
    if (extra.length) return `${where}: extra keys ${extra.slice(0, 5).join(',')}`;
    const oa = ka.filter((k) => k in b), ob = kb.filter((k) => k in a);
    for (let i = 0; i < oa.length; i++) if (oa[i] !== ob[i]) return `${where}: key order ${ob.slice(0, 6).join(',')}..., want ${oa.slice(0, 6).join(',')}...`;
    for (const k of oa) { const d = cmp(a[k], b[k], rs, [...jp, k], structOnly); if (d) return d; }
    return null;
  }
  if (structOnly || (r && r.mode === 'value')) return null;
  const am = typeof a === 'string' ? applyMap(a) : a;
  if (am !== b) return `${where}: ${JSON.stringify(b)?.slice(0, 80)}, want ${JSON.stringify(am)?.slice(0, 80)}`;
  return null;
}

function bootData(html) {
  const out = {};
  for (const k of ['user', 'settings', 'navTree', 'assets']) {
    const m = html.match(new RegExp(`\\n\\s*${k}: (.*?),?\\n`));
    if (!m) return null;
    try { out[k] = JSON.parse(m[1].replace(/,$/, '')); } catch { return null; }
  }
  return out;
}
function parseJson(t) { try { return { ok: true, v: JSON.parse(t) }; } catch { return { ok: false }; } }

async function call(method, url, headers, body) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), 15000);
  try {
    const res = await fetch(target + url, { method, headers, body: body === undefined || method === 'GET' || method === 'HEAD' ? undefined : body, redirect: 'manual', signal: ctrl.signal });
    const text = await res.text();
    for (const c of res.headers.getSetCookie?.() || []) {
      const [nv, ...attrs] = c.split(';');
      const i = nv.indexOf('=');
      const name = nv.slice(0, i).trim(), value = nv.slice(i + 1).trim();
      const gone = attrs.some((a) => /^\s*max-age=(0|-)/i.test(a)) || value === '';
      if (gone) jar.delete(name); else jar.set(name, value);
    }
    return { status: res.status, ct: res.headers.get('content-type') || '', text };
  } catch (e) {
    return { status: 0, ct: '', text: String(e.cause?.code || e.message) };
  } finally { clearTimeout(timer); }
}

async function doSeed() {
  const auth = 'Basic ' + Buffer.from('admin:admin').toString('base64');
  const h = { authorization: auth, 'content-type': 'application/json' };
  const ds = await call('POST', '/api/datasources', h, JSON.stringify({ name: 'prometheus', type: 'prometheus', uid: 'prom1', url: 'http://localhost:9390', access: 'proxy', isDefault: true }));
  console.log(`seed: POST /api/datasources -> ${ds.status} ${ds.text.slice(0, 120)}`);
  const dash = fs.readFileSync(path.join(root, 'bench/dash.json'), 'utf8');
  const d = await call('POST', '/api/dashboards/db', h, dash);
  console.log(`seed: POST /api/dashboards/db -> ${d.status} ${d.text.slice(0, 120)}`);
  // ui-tour-v1 was recorded on the instance of ui-tour (v2), which had saved dashboard gsshcd "Recorded save"
  if (!files.some((f) => path.basename(f).startsWith('ui-tour'))) return;
  const g = await call('POST', '/api/dashboards/db', h, JSON.stringify({ dashboard: { uid: 'gsshcd', title: 'Recorded save', panels: [], schemaVersion: 42 } }));
  console.log(`seed: POST /api/dashboards/db (gsshcd) -> ${g.status} ${g.text.slice(0, 120)}`);
}

// ---- main ----
if (seed) await doSeed();
const results = new Map(); // route -> {ok, total, fails: []}
let matched = 0, total = 0, skipped = 0;
for (const f of files) {
  const rows = fs.readFileSync(f, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
  for (const row of rows) {
    if (row.method === 'UPGRADE') { skipped++; continue; }
    if (only && !row.url.includes(only)) continue;
    const url = applyMap(row.url);
    const headers = {};
    for (const [k, v] of Object.entries(row.reqHeaders || {})) {
      if (v === undefined || k === 'authorization' || k === 'cookie' || k === 'user-agent') continue;
      headers[k] = applyMap(v);
    }
    if (row.reqHeaders?.cookie) { const c = cookieFor(row.reqHeaders.cookie); if (c) headers.cookie = c; }
    const res = await call(row.method, url, headers, row.reqBody === undefined ? undefined : applyMap(row.reqBody));
    let diff = null;
    if (res.status !== row.status) diff = `status ${res.status}, want ${row.status}${res.status === 0 ? ` (${res.text})` : ''}`;
    else {
      const rs = rulesFor(row.method, row.url.split('?')[0]);
      const rct = row.resHeaders?.['content-type'] || '';
      const body = row.resBody ?? '';
      if (rct.includes('json')) {
        const a = parseJson(body), b = parseJson(res.text);
        if (!a.ok) diff = cmp(body, res.text, rs, [], false);
        else if (!b.ok) diff = `body is not JSON: ${res.text.slice(0, 80)}`;
        else diff = cmp(a.v, b.v, rs, [], false);
      } else if (rct.includes('html') && body.includes('grafanaBootData')) {
        const a = bootData(body), b = bootData(res.text);
        if (!b) diff = 'no window.grafanaBootData in the page';
        else diff = cmp(a, b, rs, [], true);
      } else if (/^<\d+ bytes>$/.test(body)) {
        diff = null; // binary in the recording: status only
      } else {
        diff = cmp(body, res.text, rs, [], false);
      }
    }
    const key = routeKey(row.method, row.url);
    const e = results.get(key) || { ok: 0, total: 0, fails: [] };
    e.total++; total++;
    if (!diff) { e.ok++; matched++; } else e.fails.push(`${row.url.slice(0, 90)}: ${diff}`);
    results.set(key, e);
    if (verbose) console.log(`${diff ? 'FAIL' : 'ok  '} ${row.method} ${row.url.slice(0, 100)}${diff ? '\n       ' + diff : ''}`);
  }
}
console.log('');
const keys = [...results.keys()].sort();
for (const k of keys) {
  const e = results.get(k);
  console.log(`${e.ok === e.total ? 'PASS' : 'FAIL'} ${String(e.ok).padStart(3)}/${String(e.total).padEnd(3)} ${k}`);
  if (e.fails.length) console.log(`        ${e.fails[0]}`);
}
console.log(`\n${skipped} websocket rows skipped`);
console.log(`SCORE ${matched}/${total}`);
