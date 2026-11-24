import { lazy, Suspense, useEffect, useLayoutEffect, useMemo, useState } from "react";
import { SkelPanel } from "../shared/Skeleton";
import { mapHistory, type History } from "./chartData";

const ChartSurface = lazy(() => import("./ChartSurface"));
const RANGES = ["1M", "3M", "6M", "1Y", "5Y"] as const;
type Range = (typeof RANGES)[number];
type Result = { symbol: string; range: Range } & (
  | { history: History; error?: never }
  | { history?: never; error: string }
);

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export default function PriceChart({ symbol }: { symbol: string }) {
  const [range, setRange] = useState<Range>("6M");
  const [result, setResult] = useState<Result | null>(null);
  const forSymbol = result?.symbol === symbol ? result : null;
  const history = forSymbol?.history;
  const loading = hasTauri && forSymbol?.range !== range;
  const error = hasTauri
    ? (loading ? undefined : forSymbol?.error)
    : "เปิดแอปเดสก์ท็อปเพื่อดูกราฟราคาย้อนหลัง";
  const data = useMemo(() => history ? mapHistory(history) : [], [history]);

  useLayoutEffect(() => { setResult(null); }, [symbol]);

  useEffect(() => {
    if (!hasTauri) return;
    let live = true;
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<History>("get_stock_history", { symbol, range }))
      .then(history => { if (live) setResult({ symbol, range, history }); })
      .catch(error => { if (live) setResult({ symbol, range, error: String(error) }); });
    return () => { live = false; };
  }, [symbol, range]);

  const summary = history?.summary;
  return (
    <div className="pricechart">
      <div className="pc-head">
        <div className="pc-title">
          {symbol} · ราคาปิดรายวัน
          {summary?.last_close != null && (
            <strong className={summary.change_pct != null && summary.change_pct < 0 ? "neg" : "pos"}>
              {" "}${summary.last_close.toFixed(2)}{" "}
              ({summary.change_pct != null ? `${summary.change_pct >= 0 ? "+" : ""}${summary.change_pct.toFixed(1)}%` : "--"})
            </strong>
          )}
          {summary && <small> {summary.n} แท่ง · สูงสุด {summary.high?.toFixed(2) ?? "--"} · ต่ำสุด {summary.low?.toFixed(2) ?? "--"}</small>}
        </div>
        <div className="pc-ranges" role="group" aria-label="ช่วงเวลาของกราฟ">
          {RANGES.map(r => (
            <button key={r} type="button" className="pc-range" aria-pressed={r === range} onClick={() => setRange(r)}>{r}</button>
          ))}
        </div>
      </div>
      {loading && history && <div className="pc-status" role="status">กำลังโหลด…</div>}
      {error && <div className="pc-status err" role="status">{error}</div>}
      {history && data.length <= 1 && <div className="pc-status">ข้อมูลช่วงนี้ยังไม่พอ (ได้ {data.length} แท่ง)</div>}
      {hasTauri && (
        <Suspense fallback={!error && <SkelPanel lines={4} />}>
          {loading && !history && <SkelPanel lines={4} />}
          <ChartSurface symbol={symbol} data={data} />
        </Suspense>
      )}
      {history?.note && <small className="pc-note">{history.note}</small>}
    </div>
  );
}
