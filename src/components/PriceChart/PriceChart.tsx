import { Fragment, lazy, Suspense, useEffect, useLayoutEffect, useMemo, useState } from "react";
import { SkelPanel } from "../shared/Skeleton";
import { mapHistory, type History } from "./chartData";
import { DEFAULT_TIMEFRAME, readTimeframe, TIMEFRAME_ORDER, TIMEFRAMES, writeTimeframe, type Timeframe } from "./timeframes";

const ChartSurface = lazy(() => import("./ChartSurface"));
const barCount = new Intl.NumberFormat("th-TH");
type Result = {
  symbol: string;
  timespan: Timeframe;
  displayed?: { timespan: Timeframe; history: History };
  error?: string;
};

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export default function PriceChart({ symbol }: { symbol: string }) {
  const [timespan, setTimespan] = useState(readTimeframe);
  const [result, setResult] = useState<Result | null>(null);
  const forSymbol = result?.symbol === symbol ? result : null;
  const displayed = forSymbol?.displayed;
  const history = displayed?.history;
  const loading = hasTauri && forSymbol?.timespan !== timespan;
  const error = hasTauri
    ? (loading ? undefined : forSymbol?.error)
    : "เปิดแอปเดสก์ท็อปเพื่อดูกราฟราคาย้อนหลัง";
  const data = useMemo(() => history ? mapHistory(history) : [], [history]);

  useLayoutEffect(() => { setResult(null); }, [symbol]);

  function selectTimeframe(next: Timeframe) {
    setTimespan(next);
    writeTimeframe(next);
  }

  useEffect(() => {
    if (!hasTauri) return;
    if (forSymbol?.timespan === timespan) return;
    let live = true;
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<History>("get_stock_history", { symbol, timespan }))
      .then(history => {
        if (live) setResult({ symbol, timespan, displayed: { timespan, history } });
      })
      .catch(error => {
        if (live) setResult(previous => ({
          symbol,
          timespan,
          displayed: previous?.symbol === symbol ? previous.displayed : undefined,
          error: String(error),
        }));
      });
    return () => { live = false; };
  }, [symbol, timespan, forSymbol?.timespan]);

  const summary = history?.summary;
  return (
    <div className="pricechart">
      <div className="pc-head">
        <div className="pc-title">
          {symbol} · ราคาปิด
          {summary?.last_close != null && (
            <strong className={summary.change_pct != null && summary.change_pct < 0 ? "neg" : "pos"}>
              {" "}${summary.last_close.toFixed(2)}{" "}
              ({summary.change_pct != null ? `${summary.change_pct >= 0 ? "+" : ""}${summary.change_pct.toFixed(1)}%` : "--"})
            </strong>
          )}
          {displayed && summary && (
            <small>{TIMEFRAMES[displayed.timespan].label} · {barCount.format(summary.n)} แท่ง · สูงสุด {summary.high?.toFixed(2) ?? "--"} · ต่ำสุด {summary.low?.toFixed(2) ?? "--"}</small>
          )}
        </div>
        <div className="pc-timeframes" role="group" aria-label="กรอบเวลาของกราฟ">
          {TIMEFRAME_ORDER.map((timeframe, index) => (
            <Fragment key={timeframe}>
              {timeframe === "D" && <span className="pc-timeframe-divider" aria-hidden="true" />}
              <button type="button" className="pc-range" aria-pressed={timeframe === timespan}
                title={`${TIMEFRAMES[timeframe].label} · กด ${index + 1} เมื่อโฟกัสที่กราฟ`}
                onClick={() => selectTimeframe(timeframe)}>{TIMEFRAMES[timeframe].button}</button>
            </Fragment>
          ))}
        </div>
      </div>
      {history?.note && <small className="pc-note">{history.note}</small>}
      {loading && history && <div className="pc-status pc-loading" role="status">กำลังโหลด…</div>}
      {error && <div className="pc-status err" role="status">{error}</div>}
      {history && data.length <= 1 && <div className="pc-status">ข้อมูลช่วงนี้ยังไม่พอ (ได้ {data.length} แท่ง)</div>}
      {hasTauri && (
        <Suspense fallback={!error && <SkelPanel lines={4} />}>
          {loading && !history && <SkelPanel lines={4} />}
          <ChartSurface symbol={symbol} data={data} timespan={displayed?.timespan ?? DEFAULT_TIMEFRAME}
            onTimeframe={selectTimeframe} />
        </Suspense>
      )}
    </div>
  );
}
