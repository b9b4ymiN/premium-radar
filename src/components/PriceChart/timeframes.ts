import type { Period } from "klinecharts";

export const TIMEFRAMES = {
  M5: { button: "5m", label: "5 นาที", period: { type: "minute", span: 5 } },
  M15: { button: "15m", label: "15 นาที", period: { type: "minute", span: 15 } },
  M30: { button: "30m", label: "30 นาที", period: { type: "minute", span: 30 } },
  M60: { button: "1h", label: "1 ชั่วโมง", period: { type: "hour", span: 1 } },
  M240: { button: "4h", label: "4 ชั่วโมง", period: { type: "hour", span: 4 } },
  D: { button: "D", label: "รายวัน", period: { type: "day", span: 1 } },
  W: { button: "W", label: "รายสัปดาห์", period: { type: "week", span: 1 } },
  M: { button: "M", label: "รายเดือน", period: { type: "month", span: 1 } },
} satisfies Record<string, { button: string; label: string; period: Period }>;

export type Timeframe = keyof typeof TIMEFRAMES;
export const TIMEFRAME_ORDER = Object.keys(TIMEFRAMES) as Timeframe[];
export const DEFAULT_TIMEFRAME: Timeframe = "D";
export const CHART_TIMEZONE = "America/New_York";

const STORAGE_KEY = "premium-radar:chart-timeframe";

export function readTimeframe(): Timeframe {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return TIMEFRAME_ORDER.find(timeframe => timeframe === saved) ?? DEFAULT_TIMEFRAME;
  } catch {
    return DEFAULT_TIMEFRAME;
  }
}

export function writeTimeframe(timeframe: Timeframe) {
  try {
    localStorage.setItem(STORAGE_KEY, timeframe);
  } catch {
    // The selection still works when persistent storage is unavailable.
  }
}
