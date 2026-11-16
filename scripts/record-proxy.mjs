// Recording proxy: forwards :3301 -> :3300 and writes every non-static request and response as JSONL.
// usage: node scripts/record-proxy.mjs <out.jsonl> [listen=3301] [target=3300]
import http from 'node:http';
import fs from 'node:fs';
const [out, listen = 3301, target = 3300] = process.argv.slice(2);
const log = fs.createWriteStream(out, { flags: 'a' });
const isStatic = (u) => u.startsWith('/public/') || u.startsWith('/avatar/') || u === '/favicon.ico';
http.createServer((req, res) => {
  const chunks = [];
  req.on('data', (c) => chunks.push(c));
  req.on('end', () => {
    const reqBody = Buffer.concat(chunks);
    const headers = { ...req.headers, host: `localhost:${target}`, 'accept-encoding': 'identity' };
    const up = http.request({ host: '127.0.0.1', port: target, method: req.method, path: req.url, headers }, (ur) => {
      const rc = [];
      ur.on('data', (c) => rc.push(c));
      ur.on('end', () => {
        const body = Buffer.concat(rc);
        res.writeHead(ur.statusCode, ur.headers); res.end(body);
        if (isStatic(req.url)) return;
        const ct = ur.headers['content-type'] || '';
        const text = ct.includes('json') || ct.includes('text') || ct.includes('html');
        log.write(JSON.stringify({
          t: Date.now(), method: req.method, url: req.url,
          reqHeaders: { 'content-type': req.headers['content-type'], cookie: req.headers.cookie, authorization: req.headers.authorization ? '<redacted>' : undefined, 'user-agent': req.headers['user-agent'], 'x-grafana-org-id': req.headers['x-grafana-org-id'], 'x-datasource-uid': req.headers['x-datasource-uid'], 'x-plugin-id': req.headers['x-plugin-id'] },
          reqBody: reqBody.length ? reqBody.toString() : undefined,
          status: ur.statusCode, resHeaders: { 'content-type': ct, 'cache-control': ur.headers['cache-control'], location: ur.headers.location, 'set-cookie': ur.headers['set-cookie'] },
          resBody: text ? body.toString() : `<${body.length} bytes>`,
        }) + '\n');
      });
    });
    up.on('error', (e) => { res.writeHead(502); res.end(String(e)); });
    up.end(reqBody);
  });
}).on('upgrade', (req, sock) => { log.write(JSON.stringify({ t: Date.now(), method: 'UPGRADE', url: req.url }) + '\n'); sock.end('HTTP/1.1 501 Not Implemented\r\n\r\n'); })
  .listen(listen, () => console.log(`recording :${listen} -> :${target} into ${out}`));
