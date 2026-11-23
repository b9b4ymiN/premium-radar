//! หน้าแรก (PLAN-APP2 P17) — quotes ของ watchlist + สถานะระบบ + ทางลัด
//! ตัวเลข/การเรียงทั้งหมดจาก Rust

import { useCallback, useEffect, useState } from "react";
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface Quote { symbol: string; price: number; change_pct: number | null; source: string; volume: number }
interface Overview { watchlist_name: string | null; quotes: Quote[]; lab: { capture_days: number; last_capture: string | null }; fetched_at: string }

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
  const up = q.filter(x => (x.change_pct ?? 0) > 0).length;
  const down = q.filter(x => (x.change_pct ?? 0) < 0).length;

  return (
    <div className="homepage">
      <div className="lab-summary">
        <div className="lab-cell">
          <label>{o?.watchlist_name ?? "Watchlist"} · หุ้นขึ้นเด่น</label>
          <strong className="pos">{q.length ? up : "--"}</strong>
          <small>จาก {q.length} ตัวในลิสต์</small>
        </div>
        <div className="lab-cell">
          <label>หุ้นลงเด่น</label>
          <strong className="neg">{q.length ? down : "--"}</strong>
          <small>ตาม % เปลี่ยนของวัน (ราคาปิดล่าสุดของ API)</small>
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
          <thead><tr><th>สัญลักษณ์</th><th className="num">ราคา</th><th className="num">เปลี่ยน %</th><th className="num">วอลุ่ม</th><th></th></tr></thead>
          <tbody>
            {q.length ? q.map(x => (
              <tr key={x.symbol} className="hp-row" onClick={() => onOpenRadar(x.symbol)}>
                <td><b>{x.symbol}</b></td>
                <td className="num">{x.price.toFixed(2)}</td>
                <td className={`num ${(x.change_pct ?? 0) >= 0 ? "pos" : "neg"}`}>
                  {x.change_pct != null ? `${x.change_pct >= 0 ? "+" : ""}${x.change_pct.toFixed(2)}%` : "--"}
                </td>
                <td className="num">{x.volume ? (x.volume / 1e6).toFixed(1) + "M" : "--"}</td>
                <td><button className="wl-open">เปิด Radar →</button></td>
              </tr>
            )) : (
              <tr><td colSpan={5} className="pf-empty">
                {loading ? "กำลังโหลด…" : "ยังไม่มีหุ้นใน watchlist — ไปหน้า Watchlists เพิ่มก่อน"}
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
      <small className="pc-note">ข้อมูล ณ {o?.fetched_at?.replace("T", " ").slice(0, 19) ?? "--"} UTC · แถวหุ้นคลิกเปิด Radar ได้</small>
    </div>
  );
}
