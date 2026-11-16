// Real-workload load generator: N viewers keep one dashboard open with auto-refresh, replaying the requests that a
// real Chrome sent to the same server (recorded with scripts/record-proxy.mjs).
// usage: node bench/realload.mjs <recording.jsonl> <base_url> <users.json> <viewers> <minutes> <out.jsonl> [refresh_s=10]
//   users.json: [{"login":..., "password":...}, ...] (one per viewer; Viewer role)
// Per viewer: POST /login (session cookie), then the recorded "open" requests (page, flags, dashboard, variables,
// annotations, rules, ...), then every refresh_s seconds the recorded refresh set (the 15 /api/ds/query panel queries
// and the annotations query), with the time window moved to now. At most 6 connections per viewer, as Chrome over
// HTTP/1.1. A 401 asking for a token rotation runs POST /api/user/auth-tokens/rotate and retries once, as the UI does.
// Output: one JSON line per request {t, v, kind, method, path, status, ms, bytes}.
import http from 'node:http';
import fs from 'node:fs';

const [recFile, base, usersFile, viewersArg, minutesArg, outFile, refreshArg = '10'] = process.argv.slice(2);
const VIEWERS = +viewersArg, MINUTES = +minutesArg, REFRESH_MS = +refreshArg * 1000;
const url = new URL(base);
const users = JSON.parse(fs.readFileSync(usersFile, 'utf8'));
const out = fs.createWriteStream(outFile);

// --- the recorded mix: first 42 s of the recording, no websocket, no proxy artifacts, no curl requests
const rows = fs.readFileSync(recFile, 'utf8').trim().split('\n').map((l) => JSON.parse(l))
  .filter((r) => (r.reqHeaders?.['user-agent'] || '').startsWith('Mozilla') && r.method !== 'UPGRADE' && r.url !== '/api/login/ping');
const t0 = rows[0].t;
const first = rows.filter((r) => r.t - t0 < 42000);
const isRefresh = (r) => r.url.startsWith('/api/ds/query') || r.url.startsWith('/api/annotations');
const qt = first.filter((r) => r.url.startsWith('/api/ds/query')).map((r) => r.t);
const cycle1 = qt.length ? qt[0] : t0; // the first panel-query burst ends the "open" phase
const open = first.filter((r) => r.t < cycle1 + 2000 && !(isRefresh(r) && r.t >= cycle1 - 500));
// one refresh set: the requests of the first burst
const refresh = first.filter((r) => isRefresh(r) && r.t >= cycle1 - 500 && r.t < cycle1 + 3000);
const SPAN = (() => { const q = refresh.find((r) => r.reqBody?.includes('"from"')); if (!q) return 86400000; const b = JSON.parse(q.reqBody); return +b.to - +b.from; })();

function retime(r, now) {
  const from = String(now - SPAN), to = String(now);
  let path = r.url, body = r.reqBody || null;
  if (path.startsWith('/api/annotations')) path = path.replace(/from=\d+/, `from=${from}`).replace(/to=\d+/, `to=${to}`);
  if (body && path.startsWith('/api/ds/query')) { const b = JSON.parse(body); b.from = from; b.to = to; body = JSON.stringify(b); }
  return { method: r.method, path, body, ct: r.reqHeaders['content-type'] };
}

class Viewer {
  constructor(i, user) {
    this.i = i; this.user = user; this.cookie = '';
    this.agent = new http.Agent({ keepAlive: true, maxSockets: 6 });
  }
  send(method, path, body, ct, kind, retried = false) {
    return new Promise((resolve) => {
      const start = performance.now();
      const headers = { 'accept-encoding': 'gzip', 'user-agent': 'Mozilla/5.0 realload', 'x-grafana-org-id': '1' };
      if (this.cookie) headers.cookie = this.cookie;
      if (body) { headers['content-type'] = ct || 'application/json'; headers['content-length'] = Buffer.byteLength(body); }
      const req = http.request({ host: url.hostname, port: url.port, method, path, headers, agent: this.agent }, (res) => {
        let bytes = 0; const chunks = [];
        res.on('data', (c) => { bytes += c.length; if (res.statusCode === 401) chunks.push(c); });
        res.on('end', async () => {
          const sc = res.headers['set-cookie'];
          if (sc) this.mergeCookies(sc);
          const ms = performance.now() - start;
          if (res.statusCode === 401 && !retried && kind !== 'login' && kind !== 'rotate') {
            const ok = await this.send('POST', '/api/user/auth-tokens/rotate', null, null, 'rotate', true);
            if (ok === 200) return resolve(await this.send(method, path, body, ct, kind, true));
          }
          out.write(JSON.stringify({ t: Date.now(), v: this.i, kind, method, path: path.split('?')[0], status: res.statusCode, ms: +ms.toFixed(2), bytes }) + '\n');
          resolve(res.statusCode);
        });
      });
      req.on('error', (e) => {
        out.write(JSON.stringify({ t: Date.now(), v: this.i, kind, method, path: path.split('?')[0], status: 0, ms: +(performance.now() - start).toFixed(2), err: e.code || String(e) }) + '\n');
        resolve(0);
      });
      if (body) req.write(body);
      req.end();
    });
  }
  mergeCookies(list) {
    const jar = new Map(this.cookie ? this.cookie.split('; ').map((c) => c.split(/=(.*)/s).slice(0, 2)) : []);
    for (const c of list) { const [kv] = c.split(';'); const [k, v] = kv.split(/=(.*)/s); if (v === '' || /max-age=0|expires=thu, 01 jan 1970/i.test(c)) jar.delete(k); else jar.set(k, v); }
    this.cookie = [...jar].map(([k, v]) => `${k}=${v}`).join('; ');
  }
  async run(stopAt) {
    // a failed login is retried up to 5 times, 2 s apart, as a person would (every attempt is in the output)
    let st = 0;
    for (let a = 0; a < 6 && st !== 200; a++) {
      if (a > 0) await new Promise((r) => setTimeout(r, 2000));
      st = await this.send('POST', '/login', JSON.stringify({ user: this.user.login, password: this.user.password }), 'application/json', 'login');
    }
    if (st !== 200) { console.error(`viewer ${this.i}: login ${st} after 6 attempts`); return; }
    // open: requests in recorded order, in parallel as the browser sent them (grouped by 50 ms)
    let i = 0;
    while (i < open.length) {
      const g = [open[i]]; while (i + g.length < open.length && open[i + g.length].t - open[i].t < 50) g.push(open[i + g.length]);
      await Promise.all(g.map((r) => { const x = retime(r, Date.now()); return this.send(x.method, x.path, x.body, x.ct, 'open'); }));
      i += g.length;
    }
    let next = Date.now();
    while (true) {
      next += REFRESH_MS;
      if (next > stopAt) break;
      await new Promise((r) => setTimeout(r, Math.max(0, next - Date.now())));
      const now = Date.now();
      await Promise.all(refresh.map((r) => { const x = retime(r, now); return this.send(x.method, x.path, x.body, x.ct, 'refresh'); }));
    }
    this.agent.destroy();
  }
}

console.error(`mix: ${open.length} open requests, ${refresh.length} per refresh (${refresh.filter((r) => r.url.startsWith('/api/ds/query')).length} panel queries), window ${SPAN / 3600000} h`);
const stopAt = Date.now() + MINUTES * 60000;
const vs = [];
for (let i = 0; i < VIEWERS; i++) {
  const v = new Viewer(i, users[i % users.length]);
  // viewers arrive spread over one refresh interval, as people open the dashboard at different times
  vs.push(new Promise((r) => setTimeout(() => v.run(stopAt).then(r), (i * REFRESH_MS) / VIEWERS)));
}
await Promise.all(vs);
out.end();
console.error('done');
