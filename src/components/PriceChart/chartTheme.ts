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
  // axes: larger, slightly heavier labels so prices read at a glance
  const axis = {
    axisLine: { color: border },
    tickLine: { color: border, length: 4 },
    tickText: { color: secondary, family, size: 11.5, weight: 500, marginStart: 5, marginEnd: 5 },
  };
  // crosshair labels on a solid, contrasting chip
  const crosshairText = {
    color: token("text"), backgroundColor: token("blue-soft"), borderColor: blue,
    family, size: 11, weight: 600, paddingLeft: 6, paddingRight: 6, paddingTop: 3, paddingBottom: 3,
  };
  const crosshair = { line: { color: dim, style: "dashed" as const, dashedValue: [4, 3] }, text: crosshairText };

  return {
    // vertical grid fainter than horizontal: price levels guide, time lines stay quiet
    grid: {
      horizontal: { color: grid, style: "solid" },
      vertical: { color: grid, style: "dashed", dashedValue: [2, 4] },
    },
    xAxis: { ...axis, size: 26 },
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
      // last price: coloured tag on the axis + thin dashed line (TradingView style)
      priceMark: {
        show: true,
        high: { show: false },
        low: { show: false },
        last: {
          show: true,
          compareRule: "previous_close",
          upColor: green,
          downColor: red,
          noChangeColor: dim,
          line: { show: true, style: "dashed", dashedValue: [4, 4], size: 1 },
          text: {
            show: true, color: token("canvas"), family, size: 11, weight: 700,
            paddingLeft: 5, paddingRight: 5, paddingTop: 3, paddingBottom: 3, borderRadius: 2,
          },
        },
      },
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
