//! หน้าแรก (PLAN-APP2 P17) — quotes ของ watchlist + movers วันนี้ + สถานะระบบ + ทางลัด
//! ตัวเลข/การเรียงทั้งหมดจาก Rust

import { useEffect, useState } from "react";
import type { MoverList, MoverRow } from "../../types/webull";
import { compact, fmt, signOf, signedPct, timeEt } from "../../lib/format";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

type Quote = { symbol: string } & (
  | { status: "available"; price: number; change_pct: number | null; volume: number | null; source: "REST"; market_session: "regular"; source_timestamp: string | null; received_at: string }
  | { status: "no_data" | "error" | "setup_required"; reason?: string | null }
);
/** `error` is set when the whole quote request failed (every row is then "error"). */
interface Overview { watchlist_name: string | null; quotes: Quote[]; error?: string | null; lab: { capture_days: number; last_capture: string | null }; fetched_at: string }

const quoteStatusText = {
  no_data: "Webull ไม่มีราคา",
  setup_required: "ยังไม่ได้ตั้งค่า Webull",
};

const MOVER_LISTS: Array<{ list: MoverList; title: string }> = [
  { list: "gainers", title: "ขึ้นแรงวันนี้" },
  { list: "losers", title: "ลงแรงวันนี้" },
  { list: "most_active", title: "ซื้อขายหนาแน่น" },
];

/** Top five of three screener lists; a row opens the stock in the scanner. */
function Movers({ onOpenRadar }: { onOpenRadar: (symbol: string) => void }) {
  // per list: undefined = loading, string = why it failed, array = rows
  const [lists, setLists] = useState<Partial<Record<MoverList, MoverRow[] | string>>>({});

  useEffect(() => {
    if (!hasTauri) return;
    let live = true;
    // three paced requests; the Rust coordinator queues them behind the quotes
    MOVER_LISTS.forEach(({ list }) => {
      invokeCmd<{ rows: MoverRow[] }>("get_movers", { list })
        .then(answer => { if (live) setLists(prev => ({ ...prev, [list]: answer.rows ?? [] })); })
        .catch((e: unknown) => {
          if (live) setLists(prev => ({ ...prev, [list]: e instanceof Error ? e.message : String(e) }));
        });
    });
    return () => { live = false; };
  }, []);

  return (
    <div className="hp-movers">
      {MOVER_LISTS.map(({ list, title }) => {
        const state = lists[list];
        return (
          <div key={list} className="hp-mover">
            <div className="hp-mover-head">{title}</div>
            {state == null ? (
              <div className="hp-mover-note">กำลังดึงจาก Webull …</div>
            ) : typeof state === "string" ? (
              <div className="hp-mover-note neg">⚠ {state}</div>
            ) : state.length === 0 ? (
              <div className="hp-mover-note">ไม่มีข้อมูล</div>
            ) : (
              state.slice(0, 5).map((row, index) => (
                <button
                  key={row.symbol}
                  type="button"
                  className="hp-mover-row num"
                  onClick={() => onOpenRadar(row.symbol)}
                  title={`${row.name ?? row.symbol} · ราคา ${row.price == null ? "--" : fmt(row.price)} · วอลุ่ม ${compact(row.volume)} · เปิดใน Options Monitor`}
                >
                  <span className="hp-mover-rank">{index + 1}</span>
                  <b>{row.symbol}</b>
                  <span className={signOf(row.change_ratio)}>{signedPct(row.change_ratio)}</span>
                </button>
              ))
            )}
          </div>
        );
      })}
    </div>
  );
}

export default function HomePage({
  onOpenRadar, goTab,
}: { onOpenRadar: (s: string) => void; goTab: (t: "market" | "lab" | "portfolio" | "watchlist") => void }) {
  const [o, setO] = useState<Overview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // bumped by the refresh button; the effect below owns the request
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    if (!hasTauri) {
      setErr("เปิดแอปเดสก์ท็อปเพื่อดูข้อมูล Watchlist และ Option Lab");
      return;
    }
    let live = true;
    setLoading(true);
    setErr(null);
    invokeCmd<Overview>("get_home_overview")
      .then(overview => { if (live) setO(overview); })
      .catch(e => { if (live) setErr(String(e)); })
      .finally(() => { if (live) setLoading(false); });
    // unmounting, or a newer request, drops this answer
    return () => { live = false; };
  }, [nonce]);

  const reload = () => setNonce(value => value + 1);

  const q = o?.quotes ?? [];
  const changes = q.flatMap(x => x.status === "available" && x.change_pct != null ? [x.change_pct] : []);
  const up = changes.filter(change => change > 0).length;
  const down = changes.filter(change => change < 0).length;

  return (
    <div className="homepage">
      <div className="lab-summary">
        <div className="lab-cell">
          <label>{o?.watchlist_name ?? "Watchlist"} · หุ้นขึ้นเด่น</label>
          <strong className="pos">{changes.length ? up : "--"}</strong>
          <small>มี % เปลี่ยนแปลง {changes.length}/{q.length} ตัว</small>
        </div>
        <div className="lab-cell">
          <label>หุ้นลงเด่น</label>
          <strong className="neg">{changes.length ? down : "--"}</strong>
          <small>ตาม % เปลี่ยนแปลงที่ Webull รายงาน (session ปกติ)</small>
        </div>
        <div className="lab-cell">
          <label>Lab capture</label>
          <strong>{o?.lab.capture_days ?? 0} วัน</strong>
          <small>ล่าสุด {o?.lab.last_capture ?? "ยังไม่มี"} · เก็บอัตโนมัติทุกเช้า</small>
        </div>
      </div>

      {err && <div className="pf-err" role="alert">{err}</div>}

      <div className="hp-tablewrap">
        <table className="screener-table">
          <thead><tr><th>สัญลักษณ์</th><th className="num">ราคา</th><th className="num">เปลี่ยน %</th><th className="num">วอลุ่ม</th><th>สถานะราคา</th><th></th></tr></thead>
          <tbody>
            {q.length ? q.map(x => (
              <tr key={x.symbol} className="hp-row" onClick={() => onOpenRadar(x.symbol)}>
                <td><b>{x.symbol}</b></td>
                {x.status === "available" ? <>
                  <td className="num">{x.price.toFixed(2)}</td>
                  <td className={`num ${x.change_pct == null || x.change_pct === 0 ? "" : x.change_pct > 0 ? "pos" : "neg"}`}>
                    {x.change_pct != null ? `${x.change_pct > 0 ? "+" : ""}${x.change_pct.toFixed(2)}%` : "--"}
                  </td>
                  <td className="num">{x.volume?.toLocaleString() ?? "--"}</td>
                  <td title={`แหล่งข้อมูล ${x.source} (session ปกติ) · ดึงเมื่อ ${timeEt(Date.parse(x.received_at))}`}>
                    {x.source_timestamp ? `ซื้อขายล่าสุด ${timeEt(Date.parse(x.source_timestamp))}` : "ไม่ทราบเวลาซื้อขายล่าสุด"}
                  </td>
                </> : <>
                  <td className="num">--</td><td className="num">--</td><td className="num">--</td>
                  <td title={x.reason ?? undefined} className={x.status === "error" ? "neg" : ""}>
                    {x.status === "error" ? "ดึงราคาไม่สำเร็จ" : quoteStatusText[x.status]}
                  </td>
                </>}
                <td><button className="wl-open" onClick={e => { e.stopPropagation(); onOpenRadar(x.symbol); }}>เปิด Radar →</button></td>
              </tr>
            )) : (
              <tr><td colSpan={6} className="pf-empty">
                {loading ? "กำลังโหลด…" : o ? "ยังไม่มีหุ้นใน watchlist — ไปหน้า Watchlists เพิ่มก่อน" : "ยังไม่มีข้อมูล watchlist"}
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {o?.error && (
        <div className="pf-note" role="status">ราคาของ watchlist ดึงไม่สำเร็จ: {o.error}</div>
      )}

      <Movers onOpenRadar={onOpenRadar} />

      <div className="hp-shortcuts">
        <button onClick={() => goTab("market")}>◔ Market</button>
        <button onClick={() => goTab("watchlist")}>📋 จัด Watchlist</button>
        <button onClick={() => goTab("lab")}>🧪 Option Lab</button>
        <button onClick={() => goTab("portfolio")}>💼 พอร์ตจริง</button>
        <button onClick={reload} disabled={loading}>↻ รีเฟรช</button>
      </div>
      <small className="pc-note">
        Webull REST · เวลาซื้อขายปกติ (ET) · ดึงข้อมูลเสร็จ {o?.fetched_at ? timeEt(Date.parse(o.fetched_at)) : "--"} · แสดงสูงสุด 20 ตัว
      </small>
    </div>
  );
}
