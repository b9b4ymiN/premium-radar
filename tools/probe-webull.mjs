#!/usr/bin/env node
/**
 * tools/probe-webull.mjs — Webull TH endpoint/host matrix probe.
 *
 * ใช้ signer logic ของ vcpSystem (trade-service/src/connector/signer.ts) ตรงๆ
 * เพื่อกันข้ออ้างเรื่อง signing — ถ้า probe นี้ก็ 401 แปลว่าเป็นเรื่อง
 * key/environment ไม่ใช่ตัวแอป PREMIUM RADAR
 *
 * Usage:
 *   node tools/probe-webull.mjs [envFile]
 *
 * envFile = ไฟล์ KEY=VALUE เช่น ../vcpSystem/trade-service/.env.uat
 * รับชื่อ APP_KEY/APP_SECRET/ACCESS_TOKEN หรือ WEBULL_APP_KEY/...
 * ค่า key จะไม่ถูกพิมพ์ออกทาง output เด็ดขาด (โชว์แค่ความยาว)
 */
import { createHash, createHmac, randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';

// ---------- load env ----------
const args = process.argv.slice(2);
const env = { ...process.env };
if (args[0]) {
  for (const line of readFileSync(args[0], 'utf8').split('\n')) {
    if (line.trim().startsWith('#')) continue;
    const m = line.match(/^\s*([A-Za-z0-9_]+)\s*=\s*(.*?)\s*$/);
    if (m) env[m[1]] = m[2];
  }
}
const APP_KEY = env.WEBULL_APP_KEY || env.APP_KEY || '';
const APP_SECRET = env.WEBULL_APP_SECRET || env.APP_SECRET || '';
const ACCESS_TOKEN = env.WEBULL_ACCESS_TOKEN || env.ACCESS_TOKEN || '';
if (!APP_KEY || !APP_SECRET) {
  console.error('ต้องมี APP_KEY + APP_SECRET (env หรือ envFile)');
  process.exit(1);
}
console.log(
  `key: ${APP_KEY.slice(0, 4)}…(len ${APP_KEY.length}) · secret: (len ${APP_SECRET.length}) · token: ${ACCESS_TOKEN ? `(len ${ACCESS_TOKEN.length})` : 'ไม่มี'}`,
);

// ---------- signer: verbatim port of vcpSystem signer.ts ----------
const ALG = 'HMAC-SHA1';
const rfc3986 = (v) =>
  encodeURIComponent(v).replace(
    /[!'()*]/g,
    (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`,
  );
function signRequest({ path, host, query = {}, body, appKey, appSecret, timestamp, nonce }) {
  const combined = {
    host,
    'x-app-key': appKey,
    'x-signature-algorithm': ALG,
    'x-signature-nonce': nonce,
    'x-signature-version': '1.0',
    'x-timestamp': timestamp,
    ...query,
  };
  const sorted = Object.keys(combined)
    .sort()
    .map((k) => `${k}=${combined[k]}`)
    .join('&');
  const parts = [path, sorted];
  if (body) parts.push(createHash('md5').update(body, 'utf8').digest('hex').toUpperCase());
  const canonical = parts.join('&');
  return createHmac('sha1', `${appSecret}&`).update(rfc3986(canonical), 'utf8').digest('base64');
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function call(host, path, { query = {}, token = null, body = null } = {}) {
  const timestamp = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
  const nonce = randomUUID().replace(/-/g, '');
  const sig = signRequest({ path, host, query, body, appKey: APP_KEY, appSecret: APP_SECRET, timestamp, nonce });
  const headers = {
    'x-app-key': APP_KEY,
    'x-app-secret': APP_SECRET,
    'x-timestamp': timestamp,
    'x-signature-nonce': nonce,
    'x-signature-version': '1.0',
    'x-signature-algorithm': ALG,
    'x-version': 'v2',
    'x-signature': sig,
    accept: 'application/json',
  };
  if (body) headers['content-type'] = 'application/json';
  if (token) headers['x-access-token'] = token;
  const url = new URL(`https://${host}${path}`);
  for (const [k, v] of Object.entries(query)) url.searchParams.set(k, v);
  try {
    const r = await fetch(url, { method: body ? 'POST' : 'GET', headers, body: body ?? undefined });
    const text = (await r.text()).slice(0, 200).replace(/\s+/g, ' ');
    return `${r.status}  ${text}`;
  } catch (e) {
    return `NETWORK-ERROR ${e.message}`;
  }
}

// ---------- probe matrix ----------
const HOSTS = ['api.webull.co.th', 'th-api.uat.webullbroker.com'];
const SNAP_Q = {
  symbols: 'AAPL',
  category: 'US_STOCK',
  extend_hour_required: 'false',
  overnight_required: 'false',
};

for (const host of HOSTS) {
  console.log(`\n=== ${host} ===`);
  console.log(
    '1) instrument/stock/list (signing เท่านั้น)   :',
    await call(host, '/openapi/instrument/stock/list', { query: { category: 'US_STOCK' } }),
  );
  await sleep(1200);
  console.log(
    '2) market-data stock/snapshot (signing เท่านั้น):',
    await call(host, '/openapi/market-data/stock/snapshot', { query: SNAP_Q }),
  );
  await sleep(1200);
  if (ACCESS_TOKEN) {
    console.log(
      '3) market-data snapshot + x-access-token      :',
      await call(host, '/openapi/market-data/stock/snapshot', { query: SNAP_Q, token: ACCESS_TOKEN }),
    );
    await sleep(1200);
    console.log(
      '4) auth/token/check (ไม่แนบ header token)     :',
      await call(host, '/openapi/auth/token/check', { body: JSON.stringify({ token: ACCESS_TOKEN }) }),
    );
    await sleep(1200);
    console.log(
      '5) account/list + token                       :',
      await call(host, '/openapi/account/list', { token: ACCESS_TOKEN }),
    );
  }
}
console.log('\nเสร็จ — ดูว่าบรรทัดไหน 200 บน host ไหน = นั่นคือ environment ที่ key นี้ใช้ได้จริง');

// ---------- discovery probe (added after the 504 investigation) ----------
// Mirrors fetch_window's exact params for /openapi/instrument/option/contracts
if (process.argv.includes('--contracts')) {
  const host = 'th-api.uat.webullbroker.com';
  const today = new Date();
  const iso = (d) => d.toISOString().slice(0, 10);
  const a = iso(today);
  const b = iso(new Date(today.getTime() + 14 * 86400000));
  const q = {
    underlying_symbols: 'NVDA',
    category: 'US_OPTION',
    status: 'LISTING',
    start_date: a,
    end_date: b,
  };
  console.log(`\n=== contracts probe ${host} (window ${a}..${b}, มี gte/lte กับไม่มี) ===`);
  const t0 = Date.now();
  console.log('A) ไม่มี strike band  :', await call(host, '/openapi/instrument/option/contracts', { query: q }), `( ${(Date.now() - t0) / 1000}s )`);
  const t1 = Date.now();
  console.log('B) มี strike band     :', await call(host, '/openapi/instrument/option/contracts', { query: { ...q, strike_price_gte: '127.00', strike_price_lte: '237.00' } }), `( ${(Date.now() - t1) / 1000}s )`);
  process.exit(0);
}
