// Replay scorer: replays recorded UI traffic against a server and compares each response with the recording.
//
// usage: node scripts/replay.mjs [--target http://localhost:3400] [--seed] [--prom <url>] [--since <unix ms>]
//                                [--verbose] [--only <substring>] [file.jsonl ...]
//   --prom: the URL of the seeded data source (default http://localhost:9390; a fixture Prometheus for exact frames).
//   --since: the earliest plausible time for values the server writes (default: scorer start - 120 s).
//   default file: tests-data/recordings/ui-tour-v1.jsonl. Several files replay in the order given, sharing ids and cookies.
//
// The recordings were made with anonymous Admin access (bench/grafana.sh capture-v1), so start the target the same way:
//   GF_AUTH_ANONYMOUS_ENABLED=true GF_AUTH_ANONYMOUS_ORG_ROLE=Admin target/release/lean-sight
// and seed it like the recording (--seed runs bench/seed.sh <target>): data source prom1, dashboard bench1.
//
// A row matches when all of these hold:
//   - the status is equal;
//   - the Content-Type header is equal, and so is the Location header when the recording has one;
//   - the body matches:
//     JSON: parsed by our own parser (numbers keep their text, object keys keep their order and duplicates):
//           same types, same keys in the same order, no duplicate keys, equal values (numbers by text), recursively.
//     HTML with window.grafanaBootData: the boot data lines (user, settings, navTree, assets) compare as JSON above;
//           every other line of the page must be equal.
//     binary (<N bytes> in the recording): the byte length is equal.
//     other text: equal.
// The named substitutions of tests-data/replay-ignore.json are applied to recorded request bodies and to recorded
// strings before a comparison (for example the recording origin http://localhost:3300 becomes the target origin).
//
// Exceptions are rules in tests-data/replay-ignore.json, each with a reason:
//   {route: "METHOD /path/with/*/or/**", path: "a.*.b" or ["a", "key.with.dots"], check, ...params, reason}
// Checks (none of them accepts any value):
//   "time"  {format}: a time the server writes. format: "rfc3339" (offset text equal to the recording's),
//           "unix-s", "unix-ms", "rv" (Kubernetes resourceVersion: a decimal string of unix microseconds).
//           The value must parse, be within [--since, now], and keep the recorded order against every other
//           checked time of the run (recorded a < b => actual a <= b, at the coarser precision). "rv" values are
//           also mapped one-to-one (an equal recorded version means the same object version).
//   "uid"   {kind, format, prefix?}: a generated identifier. When the recorded value matches ^prefix(format)$, the
//           actual value must match it too, and recorded->actual maps
//           one-to-one per kind (users, dashboards, k8s, queries ...). Other recorded values (an explicit uid of the
//           request, "anonymous:0") compare exactly. Mapped values replace whole tokens of later
//           URLs, request bodies and recorded strings.
//   "id"    {kind}: a generated numeric id (number, or digit string when the recording has a string). Recorded 0
//           must be 0; otherwise a positive integer, mapped one-to-one per kind.
//   "expect" {value}: a declared difference: the actual value must be exactly `value`; the recording may lack it.
//   "sortBy" {key}: an array built from a Go map (random order): both sides are sorted by `key`, duplicate keys are
//           rejected, then the complete entries compare.
//   "score" : an approved difference in a relevance score: a positive number; within a response, the recorded
//           order of all scores of the rule is kept (a < b, a = b, a > b stay so).
//   "maxOf" {of}: the value must equal the largest actual value at path `of` of the same response; with no such
//           value (a sorted search returns hits without scores), a positive number.
//   "cost"  : an approved difference in a query cost counter: 0 when the recording has 0, else a positive integer.
// Request headers: the recorded ones are sent (User-Agent too), and no others. Rows recorded without a User-Agent
// (ui-tour-v1, made with Chrome before the recorder saved it) also get `browserHeaders` from the rule file.
// Cookies: the target's own Set-Cookie values are sent on rows that had a cookie in the recording (login flows).
// UPGRADE rows (websocket) and rows outside --only are skipped and counted. Every other row counts, in scope or not.
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const args = process.argv.slice(2);
let target = 'http://localhost:3400';
let seed = false, verbose = false, only = '', promUrl = 'http://localhost:9390', since = Date.now() - 120000;
const files = [];
for (let i = 0; i < args.length; i++) {
  const a = args[i];
  if (a === '--target') target = args[++i];
  else if (a === '--seed') seed = true;
  else if (a === '--verbose' || a === '-v') verbose = true;
  else if (a === '--only') only = args[++i];
  else if (a === '--prom') promUrl = args[++i];
  else if (a === '--since') since = Number(args[++i]);
  else files.push(a);
}
if (!files.length) files.push(path.join(root, 'tests-data/recordings/ui-tour-v1.jsonl'));
target = target.replace(/\/$/, '');

// ---- JSON: own parser. Numbers keep their text (class Num), objects keep key order and duplicates (class Obj). ----
class Num { constructor(t) { this.t = t; } }
class Obj { constructor(e) { this.e = e; } get(k) { const p = this.e.find((x) => x[0] === k); return p ? p[1] : undefined; } has(k) { return this.e.some((x) => x[0] === k); } }
class Str { constructor(v, raw) { this.v = v; this.raw = raw; } } // only inside the parser
function parseJsonText(src) {
  let i = 0;
  const ws = () => { while (i < src.length && ' \t\n\r'.includes(src[i])) i++; };
  const fail = (m) => { throw new Error(`${m} at ${i}`); };
  function str() {
    const s = i; i++;
    let out = '';
    for (;;) {
      if (i >= src.length) fail('unterminated string');
      const c = src[i];
      if (c === '"') { i++; break; }
      if (c === '\\') {
        const e = src[i + 1];
        if (e === 'u') { const h = src.slice(i + 2, i + 6); if (!/^[0-9a-fA-F]{4}$/.test(h)) fail('bad \\u'); out += String.fromCharCode(parseInt(h, 16)); i += 6; }
        else { const m = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' }[e]; if (m === undefined) fail('bad escape'); out += m; i += 2; }
      } else { if (c < ' ') fail('control char in string'); out += c; i++; }
    }
    return new Str(out, src.slice(s, i));
  }
  function val() {
    ws();
    const c = src[i];
    if (c === '{') {
      i++; const e = []; ws();
      if (src[i] === '}') { i++; return new Obj(e); }
      for (;;) {
        ws(); if (src[i] !== '"') fail('want key');
        const k = str().v; ws(); if (src[i] !== ':') fail('want :'); i++;
        e.push([k, val()]); ws();
        if (src[i] === ',') { i++; continue; }
        if (src[i] === '}') { i++; return new Obj(e); }
        fail('want , or }');
      }
    }
    if (c === '[') {
      i++; const a = []; ws();
      if (src[i] === ']') { i++; return a; }
      for (;;) {
        a.push(val()); ws();
        if (src[i] === ',') { i++; continue; }
        if (src[i] === ']') { i++; return a; }
        fail('want , or ]');
      }
    }
    if (c === '"') return str();
    if (src.startsWith('true', i)) { i += 4; return true; }
    if (src.startsWith('false', i)) { i += 5; return false; }
    if (src.startsWith('null', i)) { i += 4; return null; }
    const m = /^-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?/.exec(src.slice(i, i + 400));
    if (!m) fail('bad value');
    i += m[0].length;
    return new Num(m[0]);
  }
  const v = val(); ws();
  if (i !== src.length) fail('trailing data');
  return v;
}
const typeOf = (v) => (v === null ? 'null' : Array.isArray(v) ? 'array' : v instanceof Obj ? 'object' : v instanceof Num ? 'number' : v instanceof Str ? 'string' : typeof v);
const text = (v) => (v instanceof Str ? v.v : v instanceof Num ? v.t : String(v));
const show = (v) => {
  if (v === undefined) return 'missing';
  if (v instanceof Str) return JSON.stringify(v.v).slice(0, 80);
  if (v instanceof Num) return v.t;
  if (v instanceof Obj) return '{' + v.e.slice(0, 4).map((x) => x[0]).join(',') + (v.e.length > 4 ? ',...' : '') + '}';
  if (Array.isArray(v)) return `[${v.length} items]`;
  return JSON.stringify(v);
};
// plain JS value (from the rule file) -> parser value, for "expect"
const fromPlain = (p) => (p === null || typeof p === 'boolean' ? p : typeof p === 'number' ? new Num(String(p)) : typeof p === 'string' ? new Str(p, JSON.stringify(p))
  : Array.isArray(p) ? p.map(fromPlain) : new Obj(Object.entries(p).map(([k, v]) => [k, fromPlain(v)])));

// ---- rules ----
const ruleFile = JSON.parse(fs.readFileSync(path.join(root, 'tests-data/replay-ignore.json'), 'utf8'));
const rules = ruleFile.rules || [];
const CHECKS = { time: ['format'], uid: ['kind', 'format'], id: ['kind'], expect: ['value'], sortBy: ['key'], score: [], maxOf: ['of'], cost: [] };
for (const r of rules) {
  if (!r.route || !r.path || !r.check || !r.reason) throw new Error(`replay-ignore.json: rule needs route, path, check, reason: ${JSON.stringify(r)}`);
  if (!CHECKS[r.check]) throw new Error(`replay-ignore.json: bad check ${r.check}`);
  for (const p of CHECKS[r.check]) if (!(p in r)) throw new Error(`replay-ignore.json: check ${r.check} needs ${p}: ${JSON.stringify(r)}`);
  if (r.check === 'time' && !['rfc3339', 'unix-s', 'unix-ms', 'rv'].includes(r.format)) throw new Error(`replay-ignore.json: bad time format ${r.format}`);
  const [m, p] = r.route.split(' ');
  r._method = m;
  r._re = new RegExp('^' + p.split('/').map((s) => (s === '**' ? '.*' : s === '*' ? '[^/]+' : s.replace(/[.+?^${}()|[\]\\]/g, '\\$&'))).join('/') + '$');
  r._path = Array.isArray(r.path) ? r.path.map(String) : r.path.split('.');
  if (r.check === 'uid') r._fmt = new RegExp('^' + (r.prefix || '').replace(/[.+?^${}()|[\]\\]/g, '\\$&') + '(' + r.format + ')$');
  if (r.check === 'expect') r._value = fromPlain(r.value);
  if (r.check === 'maxOf') r._of = r.of.split('.');
  r.used = 0;
}
// named substitutions: every occurrence of `from` in a recorded string (not followed by another digit or host
// character, so :3300 never matches :33001) becomes `to`
const vars = { $TARGET: target, $PROM_URL: promUrl.replace(/\/$/, '') };
const substitutions = (ruleFile.substitutions || []).map((s) => {
  if (!s.name || !s.from || !s.to || !s.reason) throw new Error(`replay-ignore.json: substitution needs name, from, to, reason: ${JSON.stringify(s)}`);
  return { ...s, to: vars[s.to] ?? s.to, re: new RegExp(s.from.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '(?![0-9A-Za-z.-])', 'g') };
});
function substitute(s) {
  for (const x of substitutions) s = s.replace(x.re, x.to);
  return s;
}
const browser = ruleFile.browserHeaders;
if (browser && (!browser.headers || !browser.reason)) throw new Error('replay-ignore.json: browserHeaders needs headers and reason');

// ---- generated values: one-to-one maps per kind, and the times of the run ----
const maps = new Map(); // kind -> {fwd: Map(recorded->actual), back: Map(actual->recorded)}
function mapCheck(kind, rec, act, where) {
  let m = maps.get(kind);
  if (!m) maps.set(kind, (m = { fwd: new Map(), back: new Map() }));
  const f = m.fwd.get(rec), b = m.back.get(act);
  if (f !== undefined && f !== act) return `${where}: ${kind} ${JSON.stringify(act)}, want ${JSON.stringify(f)} (recorded ${JSON.stringify(rec)} was mapped before)`;
  if (b !== undefined && b !== rec) return `${where}: ${kind} ${JSON.stringify(act)} is already the value of recorded ${JSON.stringify(b)}, not ${JSON.stringify(rec)}`;
  m.fwd.set(rec, act); m.back.set(act, rec);
  return null;
}
// replaces whole tokens (delimited by characters other than [A-Za-z0-9_-]) of mapped uid values
function applyMap(s) {
  if (typeof s !== 'string') return s;
  for (const [kind, m] of maps) {
    if (kind.startsWith('#')) continue; // numeric ids and versions are not tokens of URLs
    for (const [from, to] of m.fwd) {
      if (from === to || from.length < 4 || !s.includes(from)) continue;
      s = s.replace(new RegExp(`(?<![A-Za-z0-9_-])${from.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_-])`, 'g'), to);
    }
  }
  return s;
}
const times = []; // {rec, act, prec} in ms
function parseTime(v, format) {
  if (format === 'rfc3339') {
    if (!(v instanceof Str)) return null;
    const m = /^(\d{4})-(\d\d)-(\d\d)T(\d\d):(\d\d):(\d\d)(\.\d{1,9})?(Z|[+-]\d\d:\d\d)$/.exec(v.v);
    if (!m) return null;
    const ms = Date.parse(v.v.replace(/\.(\d{3})\d+/, '.$1'));
    if (!Number.isFinite(ms)) return null;
    return { ms, prec: m[7] ? 1 : 1000, off: m[8] };
  }
  if (format === 'rv') {
    if (!(v instanceof Str) || !/^[1-9]\d{15}$/.test(v.v)) return null;
    return { ms: Number(v.v.slice(0, -3)), prec: 1 };
  }
  if (!(v instanceof Num) || !/^[1-9]\d*$/.test(v.t)) return null;
  if (format === 'unix-s') return v.t.length === 10 ? { ms: Number(v.t) * 1000, prec: 1000 } : null;
  return v.t.length === 13 ? { ms: Number(v.t), prec: 1 } : null; // unix-ms
}
function timeCheck(a, b, r, where) {
  const ta = parseTime(a, r.format);
  if (!ta) return `${where}: the recording's ${r.format} time ${show(a)} does not parse (rule is wrong)`;
  const tb = parseTime(b, r.format);
  if (!tb) return `${where}: ${show(b)} is not a ${r.format} time like ${show(a)}`;
  if (r.format === 'rfc3339' && ta.off !== tb.off) return `${where}: ${show(b)}: offset ${tb.off}, want ${ta.off}`;
  if (r.format === 'rfc3339' && (ta.prec === 1) !== (tb.prec === 1) && !/\.\d+/.test(a.v + b.v)) return `${where}: ${show(b)}: precision differs from ${show(a)}`;
  const now = Date.now();
  if (tb.ms < since - 1000 || tb.ms > now + 1000) return `${where}: ${show(b)} is outside the run (${new Date(since).toISOString()} .. now)`;
  for (const t of times) {
    const p = Math.max(t.prec, tb.prec);
    const ra = Math.floor(t.rec / p), rb = Math.floor(ta.ms / p), xa = Math.floor(t.act / p), xb = Math.floor(tb.ms / p);
    if ((ra < rb && xa > xb) || (ra > rb && xa < xb)) return `${where}: ${show(b)} breaks the recorded time order (recorded ${new Date(t.rec).toISOString()} vs ${new Date(ta.ms).toISOString()}, actual ${new Date(t.act).toISOString()} vs ${new Date(tb.ms).toISOString()})`;
  }
  times.push({ rec: ta.ms, act: tb.ms, prec: tb.prec });
  if (r.format === 'rv') return mapCheck('#rv', a.v, b.v, where);
  return null;
}

// ---- cookies: the target's Set-Cookie values are sent on rows that carried a cookie in the recording ----
const jar = new Map();
function cookieFor(recorded) {
  const names = recorded.split(';').map((p) => p.split('=')[0].trim());
  const out = names.filter((n) => jar.has(n)).map((n) => `${n}=${jar.get(n)}`);
  return out.length ? out.join('; ') : undefined;
}

// ---- comparison ----
const routeKey = (method, url) => {
  const p = url.split('?')[0];
  const norm = p
    .replace(/\/d\/[^/]+(\/[^/]+)?$/, '/d/:uid/:slug')
    .replace(/(dashboards|folders|datasources\/uid|query-history)\/[A-Za-z0-9_-]{6,}(?=\/|$)/, '$1/:uid')
    .replace(/\/plugins\/[a-z0-9-]+\/settings$/, '/plugins/:id/settings');
  return `${method} ${norm}`;
};
function rulesFor(method, url) {
  const p = url.split('?')[0];
  return rules.filter((r) => (r._method === '*' || r._method === method) && r._re.test(p));
}
function pathMatch(pat, jp) {
  if (pat.length !== jp.length) return false;
  for (let i = 0; i < jp.length; i++) if (pat[i] !== '*' && pat[i] !== String(jp[i])) return false;
  return true;
}
function ruleAt(rs, jp) {
  for (const r of rs) if (pathMatch(r._path, jp)) return r;
  return null;
}
// all values at a pattern path (for maxOf)
function collect(v, pat, i = 0, out = []) {
  if (i === pat.length) { out.push(v); return out; }
  const k = pat[i];
  if (Array.isArray(v)) { if (k === '*') v.forEach((x) => collect(x, pat, i + 1, out)); else if (v[k] !== undefined) collect(v[k], pat, i + 1, out); }
  else if (v instanceof Obj) { if (k === '*') v.e.forEach((x) => collect(x[1], pat, i + 1, out)); else if (v.has(k)) collect(v.get(k), pat, i + 1, out); }
  return out;
}

// compares recorded `a` with actual `b` under rules `rs`; ctx collects per-response checks. Returns a difference or null.
function cmp(a, b, rs, jp, ctx) {
  const r = ruleAt(rs, jp);
  const where = '$' + (jp.length ? '.' + jp.join('.') : '');
  if (r) {
    (r._orig || r).used++;
    if (r.check === 'expect') return eq(r._value, b) ? null : `${where}: ${show(b)}, want exactly ${JSON.stringify(r.value)} (declared difference)`;
  }
  if (a === undefined) return `${where}: extra value`;
  if (b === undefined) return `${where}: missing`;
  const ta = typeOf(a), tb = typeOf(b);
  // a recorded value that is not a generated one (an explicit uid from the request, "anonymous:0") compares exactly
  if (r && r.check === 'uid' && a instanceof Str && r._fmt.test(a.v)) {
    if (!(b instanceof Str) || !r._fmt.test(b.v)) return `${where}: ${show(b)} is not a ${r.kind} uid like ${show(a)}`;
    const p = (r.prefix || '').length;
    return mapCheck(r.kind, a.v.slice(p), b.v.slice(p), where);
  }
  if (r && r.check === 'id') {
    if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
    const ka = text(a), kb = text(b);
    if (!/^(0|[1-9]\d*)$/.test(ka)) return `${where}: the recording's ${show(a)} is not an id (rule is wrong)`;
    if (ta === 'string' && ka === '0') return `${where}: the recording's ${show(a)} is not an id (rule is wrong)`;
    if (ka === '0') return kb === '0' ? null : `${where}: ${show(b)}, want 0`;
    if (!/^[1-9]\d*$/.test(kb)) return `${where}: ${show(b)} is not a positive integer id`;
    return mapCheck('#' + r.kind, ka, kb, where);
  }
  if (r && r.check === 'time') return timeCheck(a, b, r, where);
  if (r && r.check === 'score') {
    if (!(b instanceof Num) || !(Number(b.t) > 0) || !Number.isFinite(Number(b.t))) return `${where}: ${show(b)} is not a positive score`;
    if (!(a instanceof Num)) return `${where}: the recording's ${show(a)} is not a score (rule is wrong)`;
    (ctx.scores[r.path] ||= []).push({ a: Number(a.t), b: Number(b.t), where });
    return null;
  }
  if (r && r.check === 'maxOf') {
    if (!(b instanceof Num)) return `${where}: ${show(b)} is not a number`;
    ctx.maxOf.push({ r, b: Number(b.t), where });
    return null;
  }
  if (r && r.check === 'cost') {
    if (!(a instanceof Num) || !(b instanceof Num)) return `${where}: type ${tb}, want ${ta}`;
    if (a.t === '0') return b.t === '0' ? null : `${where}: ${b.t}, want 0`;
    return /^[1-9]\d*$/.test(b.t) ? null : `${where}: ${b.t} is not a positive integer cost`;
  }
  if (ta !== tb) return `${where}: type ${tb}, want ${ta}`;
  if (ta === 'array') {
    if (r && r.check === 'sortBy') {
      const sa = sortedBy(a, r.key), sb = sortedBy(b, r.key);
      if (typeof sa === 'string') return `${where}: recording: ${sa}`;
      if (typeof sb === 'string') return `${where}: ${sb}`;
      a = sa; b = sb;
    }
    if (a.length !== b.length) return `${where}: length ${b.length}, want ${a.length}`;
    for (let i = 0; i < a.length; i++) { const d = cmp(a[i], b[i], rs, [...jp, i], ctx); if (d) return d; }
    return null;
  }
  if (ta === 'object') {
    for (const o of [a, b]) {
      const seen = new Set();
      for (const [k] of o.e) { if (seen.has(k)) return `${where}: duplicate key ${k}${o === a ? ' in the recording' : ''}`; seen.add(k); }
    }
    const isExpect = (k) => ruleAt(rs, [...jp, k])?.check === 'expect';
    const missing = a.e.filter(([k]) => !b.has(k)).map((x) => x[0]);
    if (missing.length) return `${where}: missing keys ${missing.slice(0, 5).join(',')}`;
    const extra = b.e.filter(([k]) => !a.has(k) && !isExpect(k)).map((x) => x[0]);
    if (extra.length) return `${where}: extra keys ${extra.slice(0, 5).join(',')}`;
    const oa = a.e.map((x) => x[0]), ob = b.e.map((x) => x[0]).filter((k) => a.has(k));
    for (let i = 0; i < oa.length; i++) if (oa[i] !== ob[i]) return `${where}: key order ${ob.slice(Math.max(0, i - 2), i + 4).join(',')}..., want ${oa.slice(Math.max(0, i - 2), i + 4).join(',')}...`;
    for (const [k, vb] of b.e) { const d = cmp(a.get(k), vb, rs, [...jp, k], ctx); if (d) return d; }
    return null;
  }
  if (ta === 'number') return a.t === b.t ? null : `${where}: ${b.t}, want ${a.t}`;
  if (ta === 'string') {
    const want = applyMap(substitute(a.v));
    if (want !== b.v) return `${where}: ${JSON.stringify(b.v).slice(0, 100)}, want ${JSON.stringify(want).slice(0, 100)}`;
    return null;
  }
  return a === b ? null : `${where}: ${show(b)}, want ${show(a)}`;
}
function eq(x, y) {
  const tx = typeOf(x);
  if (tx !== typeOf(y)) return false;
  if (tx === 'array') return x.length === y.length && x.every((v, i) => eq(v, y[i]));
  if (tx === 'object') return x.e.length === y.e.length && x.e.every(([k, v], i) => y.e[i][0] === k && eq(v, y.e[i][1]));
  if (tx === 'number') return x.t === y.t;
  if (tx === 'string') return x.v === y.v;
  return x === y;
}
function sortedBy(arr, key) {
  const keyed = [];
  for (const x of arr) {
    const k = x instanceof Obj ? x.get(key) : undefined;
    if (!(k instanceof Str) && !(k instanceof Num)) return `an entry has no ${key}`;
    keyed.push([text(k), x]);
  }
  keyed.sort((p, q) => (p[0] < q[0] ? -1 : p[0] > q[0] ? 1 : 0));
  for (let i = 1; i < keyed.length; i++) if (keyed[i][0] === keyed[i - 1][0]) return `duplicate ${key} ${keyed[i][0]}`;
  return keyed.map((p) => p[1]);
}
// per-response checks after the walk: score order, maxOf
function finish(ctx, actual) {
  for (const list of Object.values(ctx.scores)) {
    for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length; j++) {
      const sa = Math.sign(list[i].a - list[j].a), sb = Math.sign(list[i].b - list[j].b);
      if (sa !== sb) return `${list[j].where}: score order differs from the recording (${list[i].where} ${list[i].a} vs ${list[j].a}, actual ${list[i].b} vs ${list[j].b})`;
    }
  }
  for (const m of ctx.maxOf) {
    const vals = collect(actual, m.r._of).map((v) => Number(text(v)));
    if (!vals.length) { if (!(m.b > 0)) return `${m.where}: ${m.b}, want a positive number (no ${m.r.of} in the response)`; continue; }
    const want = Math.max(...vals);
    if (m.b !== want) return `${m.where}: ${m.b}, want the largest ${m.r.of} ${want}`;
  }
  return null;
}
function cmpJson(a, b, rs) {
  const ctx = { scores: {}, maxOf: [] };
  return cmp(a, b, rs, [], ctx) || finish(ctx, b);
}

const BOOT_KEYS = ['user', 'settings', 'navTree', 'assets'];
// splits a page into lines; boot data lines become {key, json text}
function pageParts(html) {
  return html.split('\n').map((line) => {
    const m = /^(\s*)(user|settings|navTree|assets): (.*?),?$/.exec(line);
    return m && BOOT_KEYS.includes(m[2]) ? { key: m[2], json: m[3] } : line;
  });
}
function cmpPage(recorded, actual, rs) {
  const pa = pageParts(recorded), pb = pageParts(actual);
  if (pa.filter((x) => typeof x !== 'string').length !== BOOT_KEYS.length) return 'the recording has no complete window.grafanaBootData';
  if (pb.filter((x) => typeof x !== 'string').length !== BOOT_KEYS.length) return 'no complete window.grafanaBootData in the page';
  if (pa.length !== pb.length) {
    const i = pa.findIndex((x, k) => typeof x === 'string' && x !== pb[k]);
    return `page has ${pb.length} lines, want ${pa.length} (first difference at line ${i + 1}: ${JSON.stringify(String(pb[i])).slice(0, 80)}, want ${JSON.stringify(pa[i]).slice(0, 80)})`;
  }
  for (let i = 0; i < pa.length; i++) {
    const x = pa[i], y = pb[i];
    if (typeof x === 'string') {
      const want = substitute(x);
      if (typeof y !== 'string' || want !== y) return `line ${i + 1}: ${JSON.stringify(typeof y === 'string' ? y : y.key).slice(0, 100)}, want ${JSON.stringify(want).slice(0, 100)}`;
      continue;
    }
    if (typeof y === 'string' || y.key !== x.key) return `line ${i + 1}: want boot data ${x.key}`;
    let va, vb;
    try { va = parseJsonText(x.json); } catch (e) { return `recorded boot ${x.key} is not JSON: ${e.message}`; }
    try { vb = parseJsonText(y.json); } catch (e) { return `boot ${x.key} is not JSON: ${e.message}`; }
    const d = cmpJson(va, vb, rs.map((r) => r).filter((r) => r._path[0] === x.key).map((r) => ({ ...r, _path: r._path.slice(1), _orig: r })));
    if (d) return `bootData.${x.key}${d.slice(1)}`;
  }
  return null;
}

// node:http, not fetch: fetch adds headers of its own (accept-language: *, accept, sec-fetch-mode) that change answers
function call(method, url, headers, body) {
  return new Promise((resolve) => {
    const u = new URL(target + url);
    const send = body === undefined || method === 'GET' || method === 'HEAD' ? undefined : Buffer.from(body);
    const h = { ...headers };
    if (send) h['content-length'] = String(send.length);
    const req = http.request({ host: u.hostname, port: u.port, path: u.pathname + u.search, method, headers: h, agent }, (res) => {
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => {
        clearTimeout(timer);
        const buf = Buffer.concat(chunks);
        for (const c of res.headers['set-cookie'] || []) {
          const [nv, ...attrs] = c.split(';');
          const i = nv.indexOf('=');
          const name = nv.slice(0, i).trim(), value = nv.slice(i + 1).trim();
          const gone = attrs.some((a) => /^\s*max-age=(0|-)/i.test(a)) || value === '';
          if (gone) jar.delete(name); else jar.set(name, value);
        }
        resolve({ status: res.statusCode, ct: res.headers['content-type'] ?? null, location: res.headers.location ?? null, text: buf.toString('utf8'), bytes: buf.length });
      });
    });
    const timer = setTimeout(() => req.destroy(new Error('timeout')), 15000);
    req.on('error', (e) => { clearTimeout(timer); resolve({ status: 0, ct: null, location: null, text: String(e.code || e.message), bytes: 0 }); });
    req.end(send);
  });
}
const agent = new http.Agent({ keepAlive: true, maxSockets: 1 });

// Seeds the target exactly as the recordings were seeded: bench/seed.sh (prom1 and bench1, as admin:admin).
async function doSeed() {
  const { spawnSync } = await import('node:child_process');
  const r = spawnSync('bash', [path.join(root, 'bench/seed.sh'), target, promUrl], { encoding: 'utf8' });
  console.log(`seed: bench/seed.sh ${target} -> exit ${r.status}${(r.stdout + r.stderr).trim() ? ': ' + (r.stdout + r.stderr).trim() : ''}`);
  // ui-tour-v1 was recorded on the instance of ui-tour (v2), which had saved dashboard gsshcd "Recorded save"
  if (!files.some((f) => path.basename(f).startsWith('ui-tour'))) return;
  const h = { authorization: 'Basic ' + Buffer.from('admin:admin').toString('base64'), 'content-type': 'application/json' };
  const g = await call('POST', '/api/dashboards/db', h, JSON.stringify({ dashboard: { uid: 'gsshcd', title: 'Recorded save', panels: [], schemaVersion: 42 } }));
  console.log(`seed: POST /api/dashboards/db (gsshcd) -> ${g.status} ${g.text.slice(0, 120)}`);
}

function compareRow(row, res) {
  if (res.status !== row.status) return `status ${res.status}, want ${row.status}${res.status === 0 ? ` (${res.text})` : ''}`;
  const rct = row.resHeaders?.['content-type'] || '';
  if ((res.ct || '') !== rct) return `Content-Type ${JSON.stringify(res.ct)}, want ${JSON.stringify(rct)}`;
  // ui-tour-v1 was recorded before the recorder saved Location; its redirects have Go's body <a href="LOCATION">
  const rloc = row.resHeaders?.location ?? (row.status >= 300 && row.status < 400 ? /^<a href="([^"]*)">/.exec(row.resBody || '')?.[1]?.replace(/&amp;/g, '&') : undefined);
  if (rloc !== undefined) {
    const want = applyMap(substitute(rloc));
    if (res.location !== want) return `Location ${JSON.stringify(res.location)}, want ${JSON.stringify(want)}`;
  } else if (res.location) return `Location ${JSON.stringify(res.location)}, want none`;
  const rs = rulesFor(row.method, row.url.split('?')[0]);
  const body = row.resBody ?? '';
  const bin = /^<(\d+) bytes>$/.exec(body);
  if (bin) return Number(bin[1]) === res.bytes ? null : `body has ${res.bytes} bytes, want ${bin[1]}`;
  if (rct.includes('json')) {
    let a, b;
    try { a = parseJsonText(body); } catch { return body === res.text ? null : `body ${JSON.stringify(res.text.slice(0, 80))}, want ${JSON.stringify(body.slice(0, 80))}`; }
    try { b = parseJsonText(res.text); } catch (e) { return `body is not JSON (${e.message}): ${res.text.slice(0, 80)}`; }
    return cmpJson(a, b, rs);
  }
  if (rct.includes('html') && body.includes('grafanaBootData')) return cmpPage(body, res.text, rs);
  return body === res.text ? null : `body ${JSON.stringify(res.text.slice(0, 80))}, want ${JSON.stringify(body.slice(0, 80))}`;
}

// ---- main ----
if (seed) await doSeed();
const results = new Map(); // route -> {ok, total, fails: []}
let matched = 0, total = 0, skippedWs = 0, skippedOnly = 0;
for (const f of files) {
  const rows = fs.readFileSync(f, 'utf8').split('\n').filter(Boolean).map((l) => JSON.parse(l));
  for (const row of rows) {
    if (row.method === 'UPGRADE') { skippedWs++; continue; }
    if (only && !row.url.includes(only)) { skippedOnly++; continue; }
    const url = applyMap(row.url);
    const headers = {};
    for (const [k, v] of Object.entries(row.reqHeaders || {})) {
      if (v === undefined || k === 'authorization' || k === 'cookie') continue;
      headers[k] = applyMap(v);
    }
    if (!row.reqHeaders?.['user-agent'] && browser) Object.assign(headers, browser.headers);
    if (row.reqHeaders?.cookie) { const c = cookieFor(row.reqHeaders.cookie); if (c) headers.cookie = c; }
    const res = await call(row.method, url, headers, row.reqBody === undefined ? undefined : applyMap(substitute(row.reqBody)));
    const diff = compareRow(row, res);
    const key = routeKey(row.method, row.url);
    const e = results.get(key) || { ok: 0, total: 0, fails: [] };
    e.total++; total++;
    if (!diff) { e.ok++; matched++; } else e.fails.push(`${row.url.slice(0, 90)}: ${diff}`);
    results.set(key, e);
    if (verbose) console.log(`${diff ? 'FAIL' : 'ok  '} ${row.method} ${row.url.slice(0, 100)}${diff ? '\n       ' + diff : ''}`);
  }
}
console.log('');
for (const k of [...results.keys()].sort()) {
  const e = results.get(k);
  console.log(`${e.ok === e.total ? 'PASS' : 'FAIL'} ${String(e.ok).padStart(3)}/${String(e.total).padEnd(3)} ${k}`);
  if (e.fails.length) console.log(`        ${e.fails[0]}`);
}
if (verbose && !only) for (const r of rules) if (!r.used) console.log(`unused rule: ${r.route} ${r.path}`);
console.log(`\nskipped: ${skippedWs} websocket rows, ${skippedOnly} rows outside --only`);
console.log(`SCORE ${matched}/${total}`);
