import { dispose, getOverlayClass, init, registerOverlay, type Overlay, type OverlayCreate } from "klinecharts";
import type { ChartBar } from "./chartData";
import { chartTheme } from "./chartTheme";
import { isDrawingName, POINT_COUNTS, readDrawings, writeDrawings, type Drawing, type DrawingTool, type Tool } from "./drawingsStore";

const PRICE = "candle_pane";
const VOLUME = "pc_volume_pane";
const GROUP = "price-chart-drawings";
const ANNOTATION = "pcTextAnnotation";
const MIN_BAR_SPACE = 0.1;
const MAX_BAR_SPACE = 50;

// The built-in annotation ignores all pointer events. Use its one-point
// placement with a selectable label and an anchor that stays at the price.
const Annotation = getOverlayClass("simpleAnnotation");
registerOverlay({
  name: ANNOTATION,
  totalStep: Annotation ? new Annotation().totalStep : 2,
  needDefaultPointFigure: true,
  createPointFigures: ({ overlay, coordinates: [point], bounding }) => {
    const align = point.x > bounding.width / 2 ? "right" : "left";
    const offset = align === "right" ? -8 : 8;
    return [
      { type: "circle", attrs: { x: point.x, y: point.y, r: 3 } },
      {
        type: "text",
        attrs: { x: point.x + offset, y: point.y, text: overlay.extendData, align, baseline: "middle" },
      },
    ];
  },
});

export interface DrawingState {
  tool: Tool;
  locked: boolean;
  magnet: boolean;
  selected: { name: DrawingTool; position: number; total: number; detail: string } | null;
  hasDrawings: boolean;
}

export const INITIAL_DRAWING_STATE: DrawingState = {
  tool: "cursor", locked: false, magnet: false, selected: null, hasDrawings: false,
};

export function createPriceCanvas(element: HTMLElement, onDrawingChange: (state: DrawingState) => void) {
  const instance = init(element, {
    styles: chartTheme(element),
    timezone: "UTC",
    zoomAnchor: "cursor",
    layout: { barSpaceLimit: { min: MIN_BAR_SPACE, max: MAX_BAR_SPACE } },
  });
  if (!instance) return null;
  const chart = instance;
  const root = chart.getDom();
  if (root) root.tabIndex = -1;

  let bars: ChartBar[] = [];
  let ticker: string | null = null;
  let drawingSymbol: string | null = null;
  let pendingId: string | null = null;
  let selectedId: string | null = null;
  let keyboardId: string | null = null;
  let tool: Tool = "cursor";
  let locked = false;
  let magnet = false;
  let autoFit = true;
  const bySymbol = new Map<string, Drawing[]>();
  const drawings = new Map<string, Drawing>();

  chart.setDataLoader({ getBars: ({ type, callback }) => callback(type === "init" ? bars : [], false) });
  chart.setPeriod({ span: 1, type: "day" });
  chart.createIndicator({
    name: "MA",
    paneId: PRICE,
    calcParams: [5, 20],
    calc: data => data.map(bar => ({ ma1: (bar as ChartBar).ma5, ma2: (bar as ChartBar).ma20 })),
  }, true);
  chart.createIndicator({
    name: "VOL",
    paneId: VOLUME,
    calcParams: [],
    calc: data => data.map(({ open, close, volume }) => ({ open, close, volume })),
  });
  chart.setPaneOptions({ id: VOLUME, height: 52, minHeight: 44, dragEnabled: false });

  function fit() {
    if (bars.length <= 1) return;
    // The first layout can resize the price scale. Use the measured plot
    // width again so its tick labels don't push the earliest bar offscreen.
    for (let pass = 0; pass < 2; pass++) {
      const width = chart.getSize(PRICE, "main")?.width ?? 0;
      if (width <= 0) return;
      const space = Math.min(MAX_BAR_SPACE, Math.max(MIN_BAR_SPACE, width / (bars.length + 1)));
      chart.setBarSpace(space);
      chart.setOffsetRightDistance(Math.min(space, 12));
    }
    autoFit = true;
  }

  function leaveFit() { autoFit = false; }
  chart.subscribeAction("onZoom", leaveFit);
  chart.subscribeAction("onScroll", leaveFit);

  let resizeFrame = 0;
  const observer = new ResizeObserver(() => {
    cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => {
      if (element.clientWidth === 0 || element.clientHeight === 0) return;
      chart.resize();
      if (autoFit) fit();
    });
  });
  observer.observe(element);

  function publish() {
    const drawing = selectedId === null ? undefined : drawings.get(selectedId);
    const point = drawing?.points[0];
    const selected = selectedId !== null && drawing && point
      ? {
          name: drawing.name,
          position: [...drawings.keys()].indexOf(selectedId) + 1,
          total: drawings.size,
          detail: drawing.text ?? `${new Date(point.timestamp).toLocaleDateString("en-CA", { timeZone: "UTC" })} · ${point.value.toFixed(2)}`,
        }
      : null;
    onDrawingChange({ tool, locked, magnet, selected, hasDrawings: drawings.size > 0 });
  }

  function highlight(id: string, active: boolean) {
    const saved = drawings.get(id)?.styles;
    const defaults = chart.getStyles().overlay;
    chart.overrideOverlay({
      id,
      styles: {
        ...saved,
        line: { ...saved?.line, size: active ? 2 : saved?.line?.size ?? defaults.line.size },
        text: { ...saved?.text, color: active ? defaults.line.color : saved?.text?.color ?? defaults.text.color },
      },
    });
  }

  function clearKeyboardSelection() {
    if (keyboardId === null) return;
    highlight(keyboardId, false);
    keyboardId = null;
    selectedId = null;
    publish();
  }

  function selectByPointer({ overlay }: { overlay: Overlay }) {
    if (!drawings.has(overlay.id)) return;
    clearKeyboardSelection();
    if (selectedId !== overlay.id) {
      selectedId = overlay.id;
      publish();
    }
  }

  function save() {
    if (drawingSymbol === null) return;
    const snapshot = [...drawings.values()];
    bySymbol.set(drawingSymbol, snapshot);
    writeDrawings(drawingSymbol, snapshot);
  }

  function saveOverlay(overlay: Overlay) {
    const name = overlay.name === ANNOTATION ? "simpleAnnotation" : overlay.name;
    if (!isDrawingName(name)) return false;
    const points = overlay.points.flatMap(({ timestamp, value }) =>
      timestamp != null && Number.isFinite(timestamp) && value != null && Number.isFinite(value)
        ? [{ timestamp, value }]
        : []);
    if (points.length !== POINT_COUNTS[name] || overlay.paneId !== PRICE) return false;
    const text = typeof overlay.extendData === "string" ? overlay.extendData : undefined;
    const styles = drawings.get(overlay.id)?.styles
      ?? { ...overlay.styles, line: { size: chart.getStyles().overlay.line.size, ...overlay.styles?.line } };
    drawings.set(overlay.id, { name, points, styles, text });
    save();
    return true;
  }

  function overlayOptions(name: Drawing["name"], text?: string): OverlayCreate {
    return {
      name: name === "simpleAnnotation" ? ANNOTATION : name,
      groupId: GROUP,
      paneId: PRICE,
      lock: locked,
      mode: magnet ? "strong_magnet" : "normal",
      extendData: text,
      onDrawEnd: ({ overlay, chart }) => {
        pendingId = null;
        tool = "cursor";
        if (!saveOverlay(overlay)) chart.removeOverlay({ id: overlay.id });
        publish();
      },
      onPressedMoveEnd: ({ overlay }) => {
        if (drawings.has(overlay.id)) saveOverlay(overlay);
      },
      onClick: selectByPointer,
      onSelected: selectByPointer,
      onDeselected: ({ overlay }) => {
        if (selectedId !== overlay.id) return;
        selectedId = null;
        publish();
      },
      onRightClick: event => {
        if (event.overlay.id !== pendingId) event.preventDefault?.();
      },
      onRemoved: ({ overlay }) => {
        if (overlay.id !== pendingId) return;
        pendingId = null;
        tool = "cursor";
        publish();
      },
    };
  }

  function activate(symbol: string) {
    drawingSymbol = symbol;
    const saved = bySymbol.get(symbol) ?? readDrawings(symbol);
    bySymbol.set(symbol, saved);
    if (saved.length === 0) return;
    const ids = chart.createOverlay(saved.map(drawing => ({
      ...overlayOptions(drawing.name, drawing.text),
      points: drawing.points,
      styles: drawing.styles ?? undefined,
    })));
    if (Array.isArray(ids)) ids.forEach((id, index) => { if (id) drawings.set(id, saved[index]); });
  }

  function cancelTool() {
    if (pendingId !== null) chart.removeOverlay({ id: pendingId });
  }

  return {
    chart,
    fit,
    setData(symbol: string, data: ChartBar[]) {
      cancelTool();
      clearKeyboardSelection();
      if (ticker !== symbol) {
        drawingSymbol = null;
        selectedId = null;
        chart.removeOverlay({ groupId: GROUP });
        drawings.clear();
      }
      bars = data.length > 1 ? data : [];
      if (bars.length) chart.resize();
      if (ticker !== symbol) {
        ticker = symbol;
        chart.setSymbol({ ticker: symbol });
      } else {
        chart.resetData();
      }
      if (bars.length) {
        fit();
        if (drawingSymbol === null) activate(symbol);
      }
      publish();
    },
    setTool(next: Tool, text?: string) {
      cancelTool();
      if (next !== "cursor" && !locked && bars.length) {
        const id = chart.createOverlay(overlayOptions(next, text));
        if (typeof id === "string") {
          pendingId = id;
          tool = next;
        }
      }
      publish();
    },
    toggleLock() {
      cancelTool();
      clearKeyboardSelection();
      locked = !locked;
      chart.overrideOverlay({ groupId: GROUP, lock: locked });
      publish();
    },
    toggleMagnet() {
      magnet = !magnet;
      chart.overrideOverlay({ groupId: GROUP, mode: magnet ? "strong_magnet" : "normal" });
      publish();
    },
    clearKeyboardSelection,
    selectDrawing(step: 1 | -1) {
      if (locked || pendingId !== null || drawings.size === 0) return false;
      const ids = [...drawings.keys()];
      const index = selectedId !== null ? ids.indexOf(selectedId) : (step === 1 ? -1 : 0);
      const next = ids[(index + step + ids.length) % ids.length];
      clearKeyboardSelection();
      selectedId = keyboardId = next;
      highlight(next, true);
      publish();
      return true;
    },
    deleteSelected() {
      if (locked || selectedId === null) return false;
      chart.removeOverlay({ id: selectedId });
      drawings.delete(selectedId);
      selectedId = null;
      keyboardId = null;
      save();
      publish();
      return true;
    },
    clear() {
      if (locked) return;
      cancelTool();
      chart.removeOverlay({ groupId: GROUP });
      drawings.clear();
      selectedId = null;
      keyboardId = null;
      save();
      publish();
    },
    destroy() {
      observer.disconnect();
      cancelAnimationFrame(resizeFrame);
      chart.unsubscribeAction("onZoom", leaveFit);
      chart.unsubscribeAction("onScroll", leaveFit);
      dispose(element);
    },
  };
}

export type PriceCanvas = NonNullable<ReturnType<typeof createPriceCanvas>>;
