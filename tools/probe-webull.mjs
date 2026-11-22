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
import { readFileSync, writeFileSync } from 'node:fs';

// ---------- load env ----------
const args = process.argv.slice(2);
const XVER = args.includes('--v3') ? 'v3' : 'v2';
const env = { ...process.env };
const envFile = args.find((a) => !a.startsWith('--'));
if (envFile) {
  for (const line of readFileSync(envFile, 'utf8').split('\n')) {
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

// ---------- access-token flow (--token-create / --token-check) ----------
// ยืนยัน response shape กับ UAT โดย vcpSystem token-manager.ts:
//   POST /openapi/auth/token/create (no body)       → {token, expires, status}
//   POST /openapi/auth/token/check  {token} in body → รูปเดียวกัน
// token value ถูกเขียนลง .access-token.txt เท่านั้น — ไม่พิมพ์ออกทาง output
const SETTINGS_JSON = 'C:/Users/bfipa/AppData/Roaming/premium-radar/radar_settings.json';
const TOKEN_FILE = 'C:/Programing/PersonalAI/option2/.access-token.txt';
if (args.includes('--token-create') || args.includes('--token-check')) {
  // ให้ settings.json ของแอป (key ใหม่จาก ⚙) ชนะค่า env ไฟล์เก่า
  try {
    const s = JSON.parse(readFileSync(SETTINGS_JSON, 'utf8'));
    if (s.webull_app_key && s.webull_app_secret) {
      env.WEBULL_APP_KEY = s.webull_app_key;
      env.WEBULL_APP_SECRET = s.webull_app_secret;
    }
  } catch {
    console.error('อ่าน settings.json ไม่ได้ — ลองกดบันทึกใน ⚙ ก่อน');
    process.exit(1);
  }
  // re-read effective creds (declared with const at top → reassign via wrapper object instead)
  const K = env.WEBULL_APP_KEY || env.APP_KEY || '';
  const S = env.WEBULL_APP_SECRET || env.APP_SECRET || '';
  const host = 'api.webull.co.th';
  const sign2 = signRequest; // same signer, local key/secret closure below
  // local signer with K/S override
  function signedCall(path, { body = null, token = null } = {}) {
    const timestamp = new Date().toISOString().replace(/\.\d{3}Z$/, 'Z');
    const nonce = randomUUID().replace(/-/g, '');
    const sig = sign2({ path, host, query: {}, body, appKey: K, appSecret: S, timestamp, nonce });
    const headers = {
      'x-app-key': K,
      'x-app-secret': S,
      'x-timestamp': timestamp,
      'x-signature-nonce': nonce,
      'x-signature-version': '1.0',
      'x-signature-algorithm': ALG,
      'x-version': XVER,
      'x-signature': sig,
      accept: 'application/json',
    };
    if (body) headers['content-type'] = 'application/json';
    if (token) headers['x-access-token'] = token;
    return fetch(`https://${host}${path}`, { method: 'POST', headers, body: body ?? undefined });
  }
  if (args.includes('--token-create')) {
    const r = await signedCall('/openapi/auth/token/create');
    const text = await r.text();
    try {
      const j = JSON.parse(text);
      if (j.token) writeFileSync(TOKEN_FILE, j.token, { mode: 0o600 });
      console.log(`create -> HTTP ${r.status} status=${j.status} token=${j.token ? `(len ${j.token.length}) เขียนลบ .access-token.txt แล้ว` : 'ไม่มี'}`);
      if (j.status === 'PENDING') console.log('SMS ส่งไปเบอร์ที่ผูกบัญชีแล้ว — ยืนยันในแอป Webull ภายใน 5 นาที แล้วรัน --token-check');
    } catch {
      console.log(`create -> HTTP ${r.status} ${text.slice(0, 160)}`);
    }
    process.exit(0);
  }
  // --token-check
  const tok = readFileSync(TOKEN_FILE, 'utf8').trim();
  const r = await signedCall('/openapi/auth/token/check', { body: JSON.stringify({ token: tok }) });
  const text = await r.text();
  try {
    const j = JSON.parse(text);
    console.log(`check -> HTTP ${r.status} status=${j.status} expires=${j.expires ?? '?'}`);
  } catch {
    console.log(`check -> HTTP ${r.status} ${text.slice(0, 160)}`);
  }
  process.exit(0);
}

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
    'x-version': XVER,
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
