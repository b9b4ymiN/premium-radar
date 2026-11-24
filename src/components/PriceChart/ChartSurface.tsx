import { memo, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Chart, Crosshair } from "klinecharts";
import type { ChartBar } from "./chartData";
import { DrawingToolbar, drawingTitle } from "./DrawingToolbar";
import type { Tool } from "./drawingsStore";
import { createPriceCanvas, INITIAL_DRAWING_STATE, type PriceCanvas } from "./priceCanvas";

const volumeFormat = new Intl.NumberFormat("en-US");
const price = (value: number | null | undefined) => value?.toFixed(2) ?? "—";

const ChartReadout = memo(function ChartReadout({ chart, host, data }: {
  chart: Chart | undefined;
  host: HTMLElement | null;
  data: ChartBar[];
}) {
  const [hover, setHover] = useState<{ index: number; data: ChartBar[] } | null>(null);

  useEffect(() => {
    if (!chart || !host) return;
    let lastIndex: number | undefined;
    const crosshair = (event?: unknown) => {
      const point = event as Crosshair | undefined;
      const converted = point?.x == null ? [] : chart.convertFromPixel([{ x: point.x }]);
      const index = point?.realDataIndex ?? (Array.isArray(converted) ? converted[0]?.dataIndex : converted.dataIndex);
      const nextIndex = index !== undefined && data[index] ? index : undefined;
      if (lastIndex === nextIndex) return;
      lastIndex = nextIndex;
      setHover(nextIndex === undefined ? null : { index: nextIndex, data });
    };
    const leave = () => crosshair();
    chart.subscribeAction("onCrosshairChange", crosshair);
    host.addEventListener("pointerleave", leave);
    return () => {
      chart.unsubscribeAction("onCrosshairChange", crosshair);
      host.removeEventListener("pointerleave", leave);
    };
  }, [chart, data, host]);

  const index = hover?.data === data ? hover.index : data.length - 1;
  const bar = data[index];
  const previous = data[index - 1]?.raw.close;
  const close = bar?.raw.close;
  const change = previous != null && previous !== 0 && close != null ? (close / previous - 1) * 100 : null;

  return (
    <div className="pc-readout" aria-label="ราคาประจำแท่งที่เลือก">
      <time dateTime={bar?.raw.time}>{bar?.raw.time.slice(0, 10)}</time>
      <span>เปิด <b>{price(bar?.raw.open)}</b></span>
      <span>สูง <b>{price(bar?.raw.high)}</b></span>
      <span>ต่ำ <b>{price(bar?.raw.low)}</b></span>
      <span>ปิด <b>{price(close)}</b></span>
      <span>ปริมาณ <b>{bar?.volume == null ? "—" : volumeFormat.format(bar.volume)}</b></span>
      <span>เปลี่ยน <b className={change == null || change === 0 ? undefined : change > 0 ? "pos" : "neg"}>
        {change == null ? "—" : `${change > 0 ? "+" : ""}${change.toFixed(2)}%`}
      </b></span>
      <span className="pc-ma5">MA5 {price(bar?.ma5)}</span>
      <span className="pc-ma20">MA20 {price(bar?.ma20)}</span>
    </div>
  );
});

export default function ChartSurface({ symbol, data }: { symbol: string; data: ChartBar[] }) {
  const host = useRef<HTMLDivElement>(null);
  const controller = useRef<PriceCanvas | null>(null);
  const [chart, setChart] = useState<Chart>();
  const [drawing, setDrawing] = useState(INITIAL_DRAWING_STATE);
  const [expanded, setExpanded] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [draft, setDraft] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const ready = data.length > 1;

  useLayoutEffect(() => () => {
    controller.current?.destroy();
    controller.current = null;
  }, []);

  useLayoutEffect(() => {
    const element = host.current;
    if (!element) return;
    if (ready && !controller.current) {
      controller.current = createPriceCanvas(element, setDrawing);
      setChart(controller.current?.chart);
      setFailed(!controller.current);
    }
    controller.current?.setData(symbol, data);
    setDraft(null);
    setConfirm(false);
  }, [symbol, data, ready]);

  useEffect(() => {
    if (!expanded) return;
    const escape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setExpanded(false);
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [expanded]);

  function focusChart() {
    host.current?.focus({ preventScroll: true });
  }

  function selectTool(tool: Tool) {
    setConfirm(false);
    if (tool === "simpleAnnotation") {
      controller.current?.setTool("cursor");
      setDraft("");
    } else {
      setDraft(null);
      controller.current?.setTool(tool);
      if (tool !== "cursor") focusChart();
    }
  }

  function canvasKey(event: KeyboardEvent<HTMLDivElement>) {
    if (event.shiftKey && (event.key === "ArrowUp" || event.key === "ArrowDown")
      && controller.current?.selectDrawing(event.key === "ArrowDown" ? 1 : -1)) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && controller.current?.deleteSelected()) {
      event.preventDefault();
      event.stopPropagation();
    } else if (event.key === "Escape") {
      controller.current?.setTool("cursor");
      controller.current?.clearKeyboardSelection();
      setConfirm(false);
      setDraft(null);
    }
  }

  return (
    <>
      {failed && ready && <div className="pc-status err" role="status">ไม่สามารถเปิดกราฟราคาได้</div>}
      <div className={`pc-content${expanded ? " expanded" : ""}`} hidden={!ready || failed}>
        <div className="pc-chart-head">
          <ChartReadout chart={chart} host={host.current} data={data} />
          <div className="pc-chart-actions">
            <button type="button" className="pc-range" title="แสดงข้อมูลทั้งหมดในช่วง"
              aria-label="แสดงข้อมูลทั้งหมดในช่วง" onClick={() => controller.current?.fit()}>⤢ Fit</button>
            <button type="button" className="pc-range pc-expand" title={expanded ? "ย่อกราฟ (Esc)" : "ขยายกราฟ"}
              aria-label={expanded ? "ย่อกราฟ (Esc)" : "ขยายกราฟ"} aria-pressed={expanded}
              onClick={() => setExpanded(value => !value)}>⛶</button>
          </div>
        </div>
        {confirm && (
          <div className="pc-inline" role="group" aria-label="ยืนยันการลบทั้งหมด">
            <span>ลบเส้นและข้อความทั้งหมดของ {symbol} หรือไม่</span>
            <button type="button" className="pc-range"
              onClick={() => { controller.current?.clear(); setConfirm(false); focusChart(); }}>ลบทั้งหมด</button>
            <button type="button" className="pc-range" onClick={() => setConfirm(false)}>ยกเลิก</button>
          </div>
        )}
        {draft !== null && (
          <form className="pc-inline" onSubmit={event => {
            event.preventDefault();
            const note = draft.trim();
            if (!note) return;
            controller.current?.setTool("simpleAnnotation", note);
            setDraft(null);
            focusChart();
          }}>
            <input value={draft} maxLength={120} autoFocus aria-label="ข้อความกำกับ"
              placeholder="ข้อความกำกับ" onChange={event => setDraft(event.target.value)} />
            <button type="submit" className="pc-range" disabled={!draft.trim()}>วางบนกราฟ</button>
            <button type="button" className="pc-range" onClick={() => setDraft(null)}>ยกเลิก</button>
          </form>
        )}
        <div className="pc-chart-body">
          <DrawingToolbar tool={draft !== null ? "simpleAnnotation" : drawing.tool}
            locked={drawing.locked} magnet={drawing.magnet} canDelete={drawing.selected !== null && !drawing.locked}
            hasDrawings={drawing.hasDrawings} onTool={selectTool}
            onLock={() => { controller.current?.toggleLock(); setDraft(null); setConfirm(false); }}
            onMagnet={() => controller.current?.toggleMagnet()}
            onDelete={() => { controller.current?.deleteSelected(); focusChart(); }}
            onClear={() => { selectTool("cursor"); setConfirm(true); }} />
          <div className="pc-viewport">
            <div className="pc-canvas" ref={host} tabIndex={0} role="region" aria-label="กราฟราคาและปริมาณซื้อขาย"
              title="Shift + ↑/↓ เพื่อเลือกเส้นหรือข้อความ · Delete เพื่อลบ"
              aria-keyshortcuts="Shift+ArrowUp Shift+ArrowDown Delete Backspace"
              onPointerDownCapture={() => { controller.current?.clearKeyboardSelection(); focusChart(); }}
              onKeyDownCapture={canvasKey} />
            {drawing.tool !== "cursor" && <div className="pc-draw-shield" aria-hidden="true" />}
          </div>
        </div>
        <span className="pc-sr-status" role="status">
          {drawing.selected && `เลือก${drawingTitle(drawing.selected.name)} รายการที่ ${drawing.selected.position} จาก ${drawing.selected.total} · ${drawing.selected.detail}`}
        </span>
      </div>
    </>
  );
}
