/** Readable axes for the price chart: round price levels every ~42 px and
 *  time labels every ~90 px that always mark a new trading day (intraday),
 *  month (daily) or year (weekly/monthly). Pure functions — the canvas wires
 *  them into KLineChart's `createTicks` hooks. */

import type { AxisCreateTicksParams, AxisTick, Period } from "klinecharts";
import { CHART_TIMEZONE } from "./timeframes";

const PRICE_TICK_PX = 42;
const TIME_TICK_PX = 90;
const EDGE_PX = 6;

/** 1, 2, 2.5, 5 × 10ⁿ — the steps a trader reads at a glance. */
export function niceStep(raw: number): number {
  if (!(raw > 0) || !Number.isFinite(raw)) return 1;
  const exp = Math.floor(Math.log10(raw));
  const base = 10 ** exp;
  const f = raw / base;
  const nice = f <= 1 ? 1 : f <= 2 ? 2 : f <= 2.5 ? 2.5 : f <= 5 ? 5 : 10;
  return nice * base;
}

function decimalsFor(step: number): number {
  for (let d = 0; d <= 6; d++) {
    if (Math.abs(Math.round(step * 10 ** d) - step * 10 ** d) < 1e-6) return d;
  }
  return 6;
}

/** Linear value→pixel map recovered from the library's own ticks. */
function calibrate(ticks: AxisTick[], toNumber: (t: AxisTick) => number | null) {
  const pts = ticks
    .map(t => ({ v: toNumber(t), c: t.coord }))
    .filter((p): p is { v: number; c: number } => p.v != null && Number.isFinite(p.v));
  if (pts.length < 2) return null;
  const a0 = pts[0];
  const a1 = pts[pts.length - 1];
  if (a1.v === a0.v) return null;
  const k = (a1.c - a0.c) / (a1.v - a0.v);
  return (v: number) => a0.c + (v - a0.v) * k;
}

export function priceTicks({ range, bounding, defaultTicks }: AxisCreateTicksParams): AxisTick[] {
  const toCoord = calibrate(defaultTicks, t => Number(t.value));
  const lo = range.displayFrom;
  const hi = range.displayTo;
  if (!toCoord || !(hi > lo)) return defaultTicks;
  const count = Math.min(12, Math.max(5, Math.round(bounding.height / PRICE_TICK_PX)));
  const step = niceStep((hi - lo) / count);
  const decimals = decimalsFor(step);
  const ticks: AxisTick[] = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + step * 1e-9; v += step) {
    const coord = toCoord(v);
    if (coord < EDGE_PX || coord > bounding.height - EDGE_PX) continue;
    const value = Number(v.toFixed(decimals));
    ticks.push({ coord, value, text: value.toLocaleString("en-US", { minimumFractionDigits: decimals, maximumFractionDigits: decimals }) });
  }
  return ticks.length >= 2 ? ticks : defaultTicks;
}

const partsFmt = new Intl.DateTimeFormat("en-US", {
  timeZone: CHART_TIMEZONE, year: "numeric", month: "short", day: "numeric",
  hour: "2-digit", minute: "2-digit", hourCycle: "h23",
});

interface Parts { y: string; mon: string; d: string; hm: string }
function parts(ts: number): Parts {
  const p: Record<string, string> = {};
  for (const x of partsFmt.formatToParts(new Date(ts))) p[x.type] = x.value;
  return { y: p.year, mon: p.month, d: p.day, hm: `${p.hour}:${p.minute}` };
}

type Scale = "intraday" | "daily" | "weekly" | "monthly";
function scaleOf({ type }: Period): Scale {
  if (type === "minute" || type === "hour") return "intraday";
  if (type === "day") return "daily";
  if (type === "week") return "weekly";
  return "monthly";
}

/** Is bar `cur` the first of a new day / month / year for this scale? */
function boundary(scale: Scale, prev: Parts | null, cur: Parts): boolean {
  if (!prev) return false;
  if (scale === "intraday") return prev.d !== cur.d || prev.mon !== cur.mon;
  if (scale === "daily") return prev.mon !== cur.mon;
  return prev.y !== cur.y;
}

function label(scale: Scale, cur: Parts, isBoundary: boolean): string {
  switch (scale) {
    case "intraday": return isBoundary ? `${cur.mon} ${cur.d}` : cur.hm;
    case "daily": return isBoundary ? (cur.mon === "Jan" ? cur.y : cur.mon) : cur.d;
    case "weekly": return isBoundary ? cur.y : `${cur.mon} ${cur.d}`;
    default: return isBoundary ? cur.y : cur.mon;
  }
}

const indexCache = new WeakMap<number[], Map<number, number>>();
function indexOf(timestamps: number[]) {
  let m = indexCache.get(timestamps);
  if (!m) {
    m = new Map(timestamps.map((t, i) => [t, i]));
    indexCache.set(timestamps, m);
  }
  return m;
}

export function timeTicks(
  { range, bounding, defaultTicks }: AxisCreateTicksParams,
  timestamps: number[],
  period: Period,
): AxisTick[] {
  if (timestamps.length < 2) return defaultTicks;
  const index = indexOf(timestamps);
  const toCoord = calibrate(defaultTicks, t => {
    const i = index.get(Number(t.value));
    return i == null ? null : i;
  });
  if (!toCoord) return defaultTicks;
  const barPx = Math.abs(toCoord(1) - toCoord(0));
  if (!(barPx > 0)) return defaultTicks;
  const every = Math.max(1, Math.ceil(TIME_TICK_PX / barPx));
  const scale = scaleOf(period);
  const from = Math.max(0, Math.floor(range.realFrom));
  const to = Math.min(timestamps.length - 1, Math.ceil(range.realTo));
  const ticks: (AxisTick & { b: boolean })[] = [];
  let prev: Parts | null = from > 0 ? parts(timestamps[from - 1]) : null;
  for (let i = from; i <= to; i++) {
    const cur = parts(timestamps[i]);
    const isB = boundary(scale, prev, cur);
    prev = cur;
    if (!isB && i % every !== 0) continue;
    const coord = toCoord(i);
    if (coord < 0 || coord > bounding.width) continue;
    const tick = { coord, value: timestamps[i], text: label(scale, cur, isB), b: isB };
    const last = ticks[ticks.length - 1];
    if (last && coord - last.coord < TIME_TICK_PX * 0.6) {
      // too close: a boundary label wins over a plain one
      if (isB && !last.b) ticks[ticks.length - 1] = tick;
      continue;
    }
    ticks.push(tick);
  }
  return ticks.length >= 2 ? ticks.map(({ coord, value, text }) => ({ coord, value, text })) : defaultTicks;
}
