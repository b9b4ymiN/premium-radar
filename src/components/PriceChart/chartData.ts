import type { KLineData } from "klinecharts";

interface Bar {
  time: string;
  open: number | null;
  high: number | null;
  low: number | null;
  close: number | null;
  volume: number | null;
}

export interface History {
  bars: Bar[];
  ma5: (number | null)[];
  ma20: (number | null)[];
  summary: {
    n: number;
    first_close: number | null;
    last_close: number | null;
    high: number | null;
    low: number | null;
    change_pct: number | null;
  };
  timespan: string;
  note: string;
}

export interface ChartBar extends KLineData {
  raw: Bar;
  ma5: number | undefined;
  ma20: number | undefined;
}

function isNumber(value: number | null): value is number {
  return value != null && Number.isFinite(value);
}

export function mapHistory({ bars, ma5, ma20 }: History): ChartBar[] {
  const data = new Map<number, ChartBar>();
  bars.forEach((raw, index) => {
    const timestamp = Date.parse(raw.time);
    const prices = [raw.open, raw.high, raw.low, raw.close].filter(isNumber);
    if (!Number.isFinite(timestamp) || prices.length === 0) return;

    // KLineChart requires finite OHLC for its axis math. Bound a partial
    // candle by observed prices, while the readout still uses the raw bar.
    const anchor = raw.open ?? raw.close ?? prices[0];
    data.set(timestamp, {
      timestamp,
      open: raw.open ?? anchor,
      high: raw.high ?? Math.max(...prices),
      low: raw.low ?? Math.min(...prices),
      close: raw.close ?? anchor,
      volume: raw.volume ?? undefined,
      raw,
      ma5: ma5[index] ?? undefined,
      ma20: ma20[index] ?? undefined,
    });
  });
  return [...data.values()].sort((a, b) => a.timestamp - b.timestamp);
}
