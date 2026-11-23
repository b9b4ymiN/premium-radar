//! หน้าแรก (PLAN-APP2 P17) — quotes ของ watchlist + สถานะระบบ + ทางลัด
//! ตัวเลข/การเรียงทั้งหมดจาก Rust

import { useCallback, useEffect, useState } from "react";
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

type Quote = { symbol: string } & (
  | { status: "available"; price: number; change_pct: number | null; volume: number | null; source: "REST"; market_session: "regular"; source_timestamp: string | null; received_at: string }
  | { status: "no_data" | "error" | "setup_required" }
);
interface Overview { watchlist_name: string | null; quotes: Quote[]; lab: { capture_days: number; last_capture: string | null }; fetched_at: string }

const quoteStatusText = {
  no_data: "Webull ไม่มีราคา",
  error: "ดึงราคาไม่สำเร็จ",
  setup_required: "ยังไม่ได้ตั้งค่า Webull",
};

export default function HomePage({
  onOpenRadar, goTab,
}: { onOpenRadar: (s: string) => void; goTab: (t: "lab" | "portfolio" | "watchlist") => void }) {
  const [o, setO] = useState<Overview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(() => {
    if (!hasTauri) {
      setErr("เปิดแอปเดสก์ท็อปเพื่อดูข้อมูล Watchlist และ Option Lab");
      return;
    }
    setLoading(true); setErr(null);
    invokeCmd<Overview>("get_home_overview")
      .then(setO).catch(e => setErr(String(e)))
      .finally(() => setLoading(false));
  }, []);
  useEffect(() => { load(); }, [load]);

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
          <small>ตาม % เปลี่ยนแปลงที่ Webull รายงาน</small>
        </div>
        <div className="lab-cell">
          <label>Lab capture</label>
          <strong>{o?.lab.capture_days ?? 0} วัน</strong>
          <small>ล่าสุด {o?.lab.last_capture ?? "ยังไม่มี"} · เก็บอัตโนมัติทุกเช้า</small>
        </div>
      </div>

      {err && <div className="pf-err">{err}</div>}

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
                  <td title={`แหล่งข้อมูล ${x.source} · ดึงเมื่อ ${new Date(x.received_at).toLocaleString("th-TH")}`}>
                    {x.source_timestamp ? `ซื้อขายล่าสุด ${new Date(x.source_timestamp).toLocaleString("th-TH")}` : "ไม่ทราบเวลาซื้อขายล่าสุด"}
                  </td>
                </> : <>
                  <td className="num">--</td><td className="num">--</td><td className="num">--</td>
                  <td>{quoteStatusText[x.status]}</td>
                </>}
                <td><button className="wl-open">เปิด Radar →</button></td>
              </tr>
            )) : (
              <tr><td colSpan={6} className="pf-empty">
                {loading ? "กำลังโหลด…" : o ? "ยังไม่มีหุ้นใน watchlist — ไปหน้า Watchlists เพิ่มก่อน" : "ยังไม่มีข้อมูล watchlist"}
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="hp-shortcuts">
        <button onClick={() => goTab("watchlist")}>📋 จัด Watchlist</button>
        <button onClick={() => goTab("lab")}>🧪 Option Lab</button>
        <button onClick={() => goTab("portfolio")}>💼 พอร์ตจริง</button>
        <button onClick={load} disabled={loading}>↻ รีเฟรช</button>
      </div>
      <small className="pc-note">Webull REST · เวลาซื้อขายปกติ · ดึงข้อมูลเสร็จ {o?.fetched_at?.replace("T", " ").slice(0, 19) ?? "--"} UTC · แสดงสูงสุด 20 ตัว</small>
    </div>
  );
}
