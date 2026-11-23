//! กราฟราคาหุ้น (PLAN-APP2 P15.4) — series/MA/สรุปมาจาก Rust ทั้งหมด
//! component เรนเดอร์อย่างเดียว: candles (≤300 แท่ง) หรือเส้น close (ยาวกว่านั้น)

import { useEffect, useState } from "react";
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface Bar { time: string; open: number | null; high: number | null; low: number | null; close: number | null; volume: number | null }
interface Summary { n: number; first_close: number | null; last_close: number | null; high: number | null; low: number | null; change_pct: number | null }
interface History { bars: Bar[]; ma5: (number | null)[]; ma20: (number | null)[]; summary: Summary; range: string; note: string }

const RANGES = ["1M", "3M", "6M", "1Y", "5Y"] as const;
const W = 760, H = 210, VOL_H = 44, PAD = 8;
const CHART_H = H - VOL_H - PAD * 2;

export default function PriceChart({ symbol }: { symbol: string }) {
  const [range, setRange] = useState<(typeof RANGES)[number]>("6M");
  const [h, setH] = useState<History | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!hasTauri) {
      setErr("เปิดแอปเดสก์ท็อปเพื่อดูกราฟราคาย้อนหลัง");
      return;
    }
    let live = true;
    setLoading(true); setErr(null);
    invokeCmd<History>("get_stock_history", { symbol, range })
      .then(r => { if (live) { setH(r); } })
      .catch(e => { if (live) { setH(null); setErr(String(e)); } })
      .finally(() => { if (live) setLoading(false); });
    return () => { live = false; };
  }, [symbol, range]);

  const s = h?.summary;
  return (
    <div className="pricechart">
      <div className="pc-head">
        <div className="pc-title">
          {symbol} · ราคาปิดรายวัน
          {s?.last_close != null && (
            <strong className={s.change_pct != null && s.change_pct < 0 ? "neg" : "pos"}>
              {" "}${s.last_close.toFixed(2)}{" "}
              ({s.change_pct != null ? `${s.change_pct >= 0 ? "+" : ""}${s.change_pct.toFixed(1)}%` : "--"})
            </strong>
          )}
          {s && <small> {s.n} แท่ง · สูงสุด {s.high?.toFixed(2) ?? "--"} · ต่ำสุด {s.low?.toFixed(2) ?? "--"}</small>}
        </div>
        <div className="pc-ranges">
          {RANGES.map(r => (
            <button key={r} className={`pc-range ${r === range ? "on" : ""}`} onClick={() => setRange(r)}>{r}</button>
          ))}
        </div>
      </div>
      {loading && <div className="pc-status">กำลังโหลด…</div>}
      {err && <div className="pc-status err">{err}</div>}
      {h && h.bars.length > 1 && <ChartSvg h={h} />}
      {h && h.bars.length <= 1 && <div className="pc-status">ข้อมูลช่วงนี้ยังไม่พอ (ได้ {h.bars.length} แท่ง)</div>}
      <small className="pc-note">{h?.note}</small>
    </div>
  );
}

function ChartSvg({ h }: { h: History }) {
  const { bars, ma5, ma20 } = h;
  // ≤300 แท่ง = candles; มากกว่านั้น = เส้น close (rendering choice — ค่าทุกตัวยังมาจาก Rust)
  const candleMode = bars.length <= 300;
  const lows = bars.map(b => b.low ?? b.close ?? 0);
  const highs = bars.map(b => b.high ?? b.close ?? 0);
  const minP = Math.min(...lows);
  const maxP = Math.max(...highs);
  const maxV = Math.max(...bars.map(b => b.volume ?? 0), 1);
  const x = (i: number) => PAD + (i / Math.max(bars.length - 1, 1)) * (W - PAD * 2);
  const y = (p: number) => PAD + (1 - (p - minP) / (maxP - minP || 1)) * CHART_H;
  const vy = (v: number) => H - PAD - (v / maxV) * VOL_H;
  const line = (vals: (number | null)[]) =>
    vals.map((v, i) => (v == null ? null : `${x(i)},${y(v)}`)).filter(Boolean).join(" ");
  const last = bars[bars.length - 1]?.close ?? null;
  const up = last != null && bars[0].close != null && last >= (bars[0].close as number);
  const stroke = up ? "#24c99a" : "#f4788c";

  return (
    <svg viewBox={`0 0 ${W} ${H}`} style={{ width: "100%", height: "auto" }} role="img" aria-label="price chart">
      {/* volume */}
      {bars.map((b, i) =>
        b.volume != null ? (
          <rect key={`v${i}`} x={x(i) - Math.max((W / bars.length) * 0.3, 0.5)} y={vy(b.volume)}
            width={Math.max((W / bars.length) * 0.6, 1)} height={H - PAD - vy(b.volume)}
            fill="#292f39" />
        ) : null
      )}
      {/* candles หรือเส้น close */}
      {candleMode ? (
        bars.map((b, i) => {
          const c = b.close ?? 0, o = b.open ?? c;
          const green = c >= o;
          const col = green ? "#24c99a" : "#f4788c";
          const top = y(Math.max(o, c)), bot = y(Math.min(o, c));
          return (
            <g key={i}>
              {(b.high != null && b.low != null) && <line x1={x(i)} y1={y(b.high)} x2={x(i)} y2={y(b.low)} stroke={col} strokeWidth="0.8" />}
              <rect x={x(i) - Math.max((W / bars.length) * 0.32, 0.7)} y={top}
                width={Math.max((W / bars.length) * 0.64, 1.4)} height={Math.max(bot - top, 0.8)}
                fill={green ? "none" : col} stroke={col} strokeWidth="0.8" />
            </g>
          );
        })
      ) : (
        <polyline points={line(bars.map(b => b.close))} fill="none" stroke={stroke} strokeWidth="1.6" />
      )}
      {/* MA */}
      <polyline points={line(ma5)} fill="none" stroke="#f0bd70" strokeWidth="1" opacity="0.9" />
      <polyline points={line(ma20)} fill="none" stroke="#4b8cf7" strokeWidth="1" opacity="0.9" />
      {/* ป้ายระยะ */}
      <text x={PAD} y={PAD + 8} fill="#f0bd70" fontSize="8">MA5</text>
      <text x={PAD + 26} y={PAD + 8} fill="#4b8cf7" fontSize="8">MA20</text>
      <text x={W - PAD} y={PAD + 8} fill="#a2adbc" fontSize="8" textAnchor="end">{bars[0].time.slice(0, 10)}</text>
      <text x={W - PAD} y={H - 2} fill="#a2adbc" fontSize="8" textAnchor="end">{bars[bars.length - 1].time.slice(0, 10)}</text>
      <text x={W - PAD} y={y(maxP) + 8} fill="#a2adbc" fontSize="8" textAnchor="end">{maxP.toFixed(1)}</text>
      <text x={W - PAD} y={y(minP) - 2} fill="#a2adbc" fontSize="8" textAnchor="end">{minP.toFixed(1)}</text>
    </svg>
  );
}
