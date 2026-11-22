//! หน้า Portfolio read-only (PLAN-APP2 P18) — ข้อมูลบัญชีจริงจาก Rust
//! ไม่มี order controls ใดๆ · แสดง broker-reported fields เป็นหลัก

import { useCallback, useEffect, useState } from "react";

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface Leg { symbol: string; quantity: number | null; option_type: string | null; expire_date: string | null; exercise_price: number | null; multiplier: number | null }
interface Position { symbol: string; instrument_type: string; currency: string; quantity: number | null; cost_price: number | null; last_price: number | null; unrealized_profit_loss: number | null; option_strategy: string | null; legs: Leg[] }
interface CurrencyBalance { currency: string; cash_balance: number | null; market_value: number | null; buying_power: number | null; unrealized_profit_loss: number | null }
interface Balance { total_asset_currency: string; total_cash_balance: number | null; total_market_value: number | null; total_unrealized_profit_loss: number | null; currencies: CurrencyBalance[] }
interface Portfolio { account_label: string; account_masked: string; as_of: string; positions: Position[]; balance: Balance | null }

const usd = (v: number | null | undefined) =>
  v == null ? "--" : `${v < 0 ? "−" : ""}$${Math.abs(v).toLocaleString(undefined, { maximumFractionDigits: 2 })}`;

export default function PortfolioPage() {
  const [p, setP] = useState<Portfolio | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [asOf, setAsOf] = useState<string | null>(null);

  const load = useCallback(() => {
    setLoading(true); setErr(null);
    invokeCmd<Portfolio>("get_portfolio")
      .then(r => { setP(r); setAsOf(new Date().toLocaleTimeString("th-TH")); })
      .catch(e => setErr(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => { load(); }, [load]);

  const b = p?.balance;
  return (
    <div className="portfolio">
      <div className="pf-head">
        <div>
          <div className="pf-title">พอร์ตจริง · {p?.account_label ?? "--"} <small>({p?.account_masked ?? "--"})</small></div>
          <small className="pf-sub">อ่านอย่างเดียวจาก Webull · ข้อมูล ณ {asOf ?? "--"}{p?.as_of ? "" : ""}</small>
        </div>
        <button className="pf-refresh" onClick={load} disabled={loading}>{loading ? "กำลังโหลด…" : "↻ รีเฟรช"}</button>
      </div>

      {err && <div className="pf-err">{err}</div>}

      {b && (
        <div className="pf-summary">
          <div className="lab-cell">
            <label>เงินสดรวม</label><strong>{usd(b.total_cash_balance)}</strong>
            <small>{b.total_asset_currency || "USD"}</small>
          </div>
          <div className="lab-cell">
            <label>มูลค่าถือครอง</label><strong>{usd(b.total_market_value)}</strong>
            <small>ตามราคาล่าสุดของโบรกเกอร์</small>
          </div>
          <div className="lab-cell">
            <label>กำไร/ขาดทุนลอยรวม</label>
            <strong className={(b.total_unrealized_profit_loss ?? 0) >= 0 ? "pos" : "neg"}>
              {usd(b.total_unrealized_profit_loss)}
            </strong>
            <small>ยังไม่ปิดสถานะ — ไม่ใช่กำไรที่รับรองแล้ว</small>
          </div>
          {b.currencies.map(c => (
            <div key={c.currency} className="lab-cell">
              <label>{c.currency} · Buying Power</label><strong>{usd(c.buying_power)}</strong>
              <small>ไม่ใช่เงินสดพร้อมทำ CSP ทั้งหมด</small>
            </div>
          ))}
        </div>
      )}
      {!b && !err && p && <div className="pf-note">ยอดเงินยังดึงไม่ได้ — แสดงเฉพาะสถานะถือครอง</div>}

      <div className="pf-tablewrap">
        <table className="screener-table">
          <thead>
            <tr>
              <th>สัญลักษณ์</th><th>ประเภท</th>
              <th className="num">จำนวน</th><th className="num">ต้นทุน</th>
              <th className="num">ราคาล่าสุด</th><th className="num">กำไร/ขาดทุนลอย</th>
            </tr>
          </thead>
          <tbody>
            {p?.positions.length ? (
              p.positions.map((pos, i) => (
                <tr key={i}>
                  <td title={pos.legs.map(l => l.symbol).join(" · ")}>
                    {pos.symbol}
                    {pos.option_strategy && <small className="pf-strat"> {pos.option_strategy}</small>}
                    {pos.legs.map((l, j) => (
                      <div key={j} className="pf-leg">
                        {l.option_type === "CALL" ? "C" : l.option_type === "PUT" ? "P" : "?"}
                        {" "}{l.exercise_price ?? "--"} · หมดอายุ {l.expire_date ?? "--"} ×{l.quantity ?? "--"}
                      </div>
                    ))}
                  </td>
                  <td>{pos.instrument_type === "OPTION" ? "Option" : "หุ้น"}</td>
                  <td className="num">{pos.quantity ?? "--"}</td>
                  <td className="num">{pos.cost_price?.toFixed(2) ?? "--"}</td>
                  <td className="num">{pos.last_price?.toFixed(2) ?? "--"}</td>
                  <td className={`num ${(pos.unrealized_profit_loss ?? 0) >= 0 ? "pos" : "neg"}`}>
                    {usd(pos.unrealized_profit_loss)}
                  </td>
                </tr>
              ))
            ) : (
              <tr><td colSpan={6} className="pf-empty">ไม่มีสถานะถือครอง (บัญชีไม่มีของเปิดอยู่ตอนนี้ — ไม่ใช่ข้อผิดพลาด)</td></tr>
            )}
          </tbody>
        </table>
      </div>
      <small className="pc-note">ตัวเลขทั้งหมดรายงานโดยโบรกเกอร์ · แอปนี้ไม่มีการส่งคำสั่งซื้อขาย · กำไรลอย ≠ กำไรที่รับรอง</small>
    </div>
  );
}
