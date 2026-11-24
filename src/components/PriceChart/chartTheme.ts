import type { Chart } from "klinecharts";
import type { ChartBar } from "./chartData";

export function chartTheme(element: HTMLElement): Parameters<Chart["setStyles"]>[0] {
  const css = getComputedStyle(element);
  const token = (name: string) => css.getPropertyValue(`--${name}`).trim();
  const noInk = token("pc-no-ink");
  const canvas = token("canvas");
  const border = token("border");
  const grid = token("border-soft");
  const dim = token("dim");
  const secondary = token("secondary");
  const blue = token("blue");
  const green = token("green");
  const red = token("red");
  const family = token("sans");
  const text = { color: token("text"), backgroundColor: token("surface3"), family, size: 10 };
  const axis = {
    axisLine: { color: border },
    tickLine: { color: border },
    tickText: { color: secondary, family, size: 10 },
  };
  const crosshair = { line: { color: dim }, text };

  return {
    grid: { horizontal: { color: grid, style: "solid" }, vertical: { color: grid, style: "solid" } },
    xAxis: { ...axis, size: 22 },
    yAxis: axis,
    crosshair: { horizontal: crosshair, vertical: crosshair },
    separator: { color: border, activeBackgroundColor: token("surface2") },
    candle: {
      bar: ({ current }) => {
        const raw = (current as ChartBar | undefined)?.raw;
        const open = raw?.open;
        const close = raw?.close;
        if (open == null || close == null) return { color: noInk, borderColor: noInk, wickColor: dim };
        const color = close > open ? green : close < open ? red : dim;
        return { color, borderColor: color, wickColor: color };
      },
      priceMark: { show: false },
      tooltip: { showRule: "none" },
    },
    indicator: {
      lines: [{ color: token("amber"), size: 1 }, { color: blue, size: 1 }],
      bars: [{ upColor: green, downColor: red, noChangeColor: dim }],
      tooltip: { showRule: "none" },
    },
    overlay: {
      line: { color: blue },
      point: { color: canvas, borderColor: blue, activeColor: blue, activeBorderColor: canvas },
      polygon: { color: blue },
      circle: { color: blue },
      text,
    },
  };
}
