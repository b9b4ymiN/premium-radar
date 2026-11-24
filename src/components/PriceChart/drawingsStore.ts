import type { Overlay } from "klinecharts";

export const POINT_COUNTS = {
  segment: 2,
  rayLine: 2,
  horizontalStraightLine: 1,
  verticalStraightLine: 1,
  parallelStraightLine: 3,
  fibonacciLine: 2,
  simpleAnnotation: 1,
} as const;

export type DrawingTool = keyof typeof POINT_COUNTS;
export type Tool = "cursor" | DrawingTool;
export interface Drawing {
  name: DrawingTool;
  points: { timestamp: number; value: number }[];
  styles: Overlay["styles"];
  text?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function isDrawingName(name: unknown): name is DrawingTool {
  return typeof name === "string" && Object.prototype.hasOwnProperty.call(POINT_COUNTS, name);
}

function storageKey(symbol: string) {
  return `premium-radar:drawings:${symbol.toUpperCase()}`;
}

export function readDrawings(symbol: string): Drawing[] {
  try {
    const stored: unknown = JSON.parse(localStorage.getItem(storageKey(symbol)) ?? "[]");
    if (!Array.isArray(stored)) return [];

    return stored.flatMap((entry: unknown) => {
      if (!isRecord(entry) || !isDrawingName(entry.name) || !Array.isArray(entry.points)
        || entry.points.length !== POINT_COUNTS[entry.name]) return [];
      const points = entry.points.flatMap((point: unknown) => {
        if (!isRecord(point) || typeof point.timestamp !== "number" || !Number.isFinite(point.timestamp)
          || typeof point.value !== "number" || !Number.isFinite(point.value)) return [];
        return [{ timestamp: point.timestamp, value: point.value }];
      });
      if (points.length !== entry.points.length) return [];

      const text = typeof entry.text === "string" ? entry.text.trim().slice(0, 120) : undefined;
      if (entry.name === "simpleAnnotation" && !text) return [];
      return [{ name: entry.name, points, styles: isRecord(entry.styles) ? entry.styles : null, text }];
    });
  } catch {
    return [];
  }
}

export function writeDrawings(symbol: string, drawings: Iterable<Drawing>) {
  try {
    localStorage.setItem(storageKey(symbol), JSON.stringify([...drawings]));
  } catch {
    // The chart keeps the session's drawings in memory when storage is unavailable.
  }
}
