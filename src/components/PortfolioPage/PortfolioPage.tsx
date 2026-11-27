//! หน้า Portfolio read-only (PLAN-APP2 P18) — ข้อมูลบัญชีจริงจาก Rust
//! ไม่มี order controls ใดๆ · แสดง broker-reported fields เป็นหลัก

import { useCallback, useEffect, useState } from "react";
import type { OpenOrder } from "../../types/webull";
import { anchorId, useAgentAnchor } from "../../agent/anchors";
import { thaiDateWithDays } from "../../lib/format";

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface Leg { symbol: string; quantity: number | null; option_type: string | null; expire_date: string | null; exercise_price: number | null; multiplier: number | null }
interface Position { symbol: string; instrument_type: string; currency: string; quantity: number | null; cost_price: number | null; last_price: number | null; unrealized_profit_loss: number | null; option_strategy: string | null; legs: Leg[] }
interface CurrencyBalance { currency: string; cash_balance: number | null; market_value: number | null; buying_power: number | null; unrealized_profit_loss: number | null }
interface Balance { total_asset_currency: string; total_cash_balance: number | null; total_market_value: number | null; total_unrealized_profit_loss: number | null; currencies: CurrencyBalance[] }
/** `*_error` says which section the broker did not answer; the rest still shows. */
interface Portfolio {
  account_label?: string;
  account_masked?: string;
  as_of?: string;
  note?: string;
  positions: Position[];
  balance: Balance | null;
  balance_error?: string | null;
  open_orders?: OpenOrder[];
  open_orders_error?: string | null;
}

function amount(value: number | null, currency: string, maximumFractionDigits = 2) {
  if (value == null || !Number.isFinite(value)) return "--";
  const code = currency.trim().toUpperCase();
  if (/^[A-Z]{3}$/.test(code)) {
    return value.toLocaleString(undefined, { style: "currency", currency: code, currencyDisplay: "code", maximumFractionDigits });
  }
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 4 })} · ไม่ทราบสกุลเงิน`;
}

function pnlClass(value: number | null) {
  if (value == null || value === 0) return "";
  return value > 0 ? "pos" : "neg";
}

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export default function PortfolioPage() {
  const [p, setP] = useState<Portfolio | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const cashRef = useAgentAnchor(anchorId("pf", "cash"), "เงินสดรวมในพอร์ต");
  const mvRef = useAgentAnchor(anchorId("pf", "market_value"), "มูลค่าถือครองรวมในพอร์ต");
  const plRef = useAgentAnchor(anchorId("pf", "pl"), "กำไร/ขาดทุนลอยรวมของพอร์ต");

  const load = useCallback(() => {
    if (!hasTauri) {
      setErr("เปิดแอปเดสก์ท็อปเพื่อดูข้อมูลพอร์ต Webull");
      return;
    }
    setLoading(true); setErr(null);
    invokeCmd<Portfolio>("get_portfolio")
      .then(setP)
      .catch(e => setErr(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => { load(); }, [load]);

  const b = p?.balance;
  const asOf = p?.as_of ? new Date(p.as_of).toLocaleString("th-TH") : "--";
  return (
    <div className="portfolio">
      <div className="pf-head">
        <div>
          <div className="pf-title">พอร์ตจริง · {p?.account_label ?? "--"} <small>({p?.account_masked ?? "--"})</small></div>
          <small className="pf-sub">อ่านอย่างเดียวจาก Webull · ดึงข้อมูลเมื่อ {asOf}</small>
        </div>
        <button className="pf-refresh" onClick={load} disabled={loading}>{loading ? "กำลังโหลด…" : "↻ รีเฟรช"}</button>
      </div>

      {err && <div className="pf-err" role="status">{p?.as_of ? "รีเฟรชไม่สำเร็จ · แสดงข้อมูลจากครั้งก่อน · " : ""}{err}</div>}
      {p?.note && <div className="pf-note">{p.note}</div>}

      {b && (
        <div className="pf-summary">
          <div className="lab-cell" ref={cashRef}>
            <label>เงินสดรวม</label><strong>{amount(b.total_cash_balance, b.total_asset_currency)}</strong>
          </div>
          <div className="lab-cell" ref={mvRef}>
            <label>มูลค่าถือครอง</label><strong>{amount(b.total_market_value, b.total_asset_currency)}</strong>
            <small>ตามราคาล่าสุดของโบรกเกอร์</small>
          </div>
          <div className="lab-cell" ref={plRef}>
            <label>กำไร/ขาดทุนลอยรวม</label>
            <strong className={pnlClass(b.total_unrealized_profit_loss)}>
              {amount(b.total_unrealized_profit_loss, b.total_asset_currency)}
            </strong>
            <small>ยังไม่ปิดสถานะ — ไม่ใช่กำไรที่รับรองแล้ว</small>
          </div>
          {b.currencies.map(c => (
            <div key={c.currency} className="lab-cell">
              <label>{c.currency || "ไม่ทราบสกุลเงิน"} · Buying Power</label><strong>{amount(c.buying_power, c.currency)}</strong>
              <small>เงินสด {amount(c.cash_balance, c.currency)} · โบรกเกอร์รายงานแยกจาก Buying Power</small>
            </div>
          ))}
        </div>
      )}
      {!b && p?.balance_error && <div className="pf-note" role="status">ยอดเงินดึงไม่สำเร็จ — {p.balance_error} (แสดงเฉพาะสถานะถือครอง)</div>}
      {!b && !err && p?.as_of && !p.balance_error && <div className="pf-note">ยอดเงินยังดึงไม่ได้ — แสดงเฉพาะสถานะถือครอง</div>}

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
              p.positions.map((pos, i) => <PositionRow key={i} pos={pos} />)
            ) : (
              <tr><td colSpan={6} className="pf-empty">{loading ? "กำลังโหลดสถานะถือครอง…" : p?.as_of ? "ไม่มีสถานะถือครอง" : "ไม่มีข้อมูลสถานะถือครอง"}</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="section-title">
        OPEN ORDERS · คำสั่งที่ยังทำงานอยู่
        <span>อ่านอย่างเดียว</span>
      </div>
      {p?.open_orders_error ? (
        <div className="pf-note" role="status">ดึงคำสั่งที่ยังทำงานไม่สำเร็จ — {p.open_orders_error}</div>
      ) : (
        <div className="pf-tablewrap">
          <table className="screener-table">
            <thead>
              <tr>
                <th>สัญลักษณ์</th><th>ประเภท</th><th>ขาย/ซื้อ</th><th>คำสั่ง</th>
                <th className="num">จำนวน (เท่าไหร่แล้ว)</th><th className="num">ราคาจำกัด</th><th>สถานะ</th><th>ส่งเมื่อ</th>
              </tr>
            </thead>
            <tbody>
              {(p?.open_orders ?? []).length ? (
                p!.open_orders!.map((order, i) => (
                  <OrderRow key={`${order.combo_type ?? ""}-${order.client_order_id}`} order={order} n={i + 1} />
                ))
              ) : (
                <tr>
                  <td colSpan={8} className="pf-empty">{p ? "ไม่มีคำสั่งที่ยังทำงานอยู่" : err ? "—" : "กำลังโหลดคำสั่ง…"}</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      )}
      <small className="pc-note">ตัวเลขทั้งหมดรายงานโดยโบรกเกอร์ · แอปนี้ไม่มีการส่งคำสั่งซื้อขาย · กำไรลอย ≠ กำไรที่รับรอง</small>
    </div>
  );
}

/** One position row — its own component so each row can register its own
 *  agent anchor (hooks cannot be called inside the `.map` loop). */
function PositionRow({ pos }: { pos: Position }) {
  const ref = useAgentAnchor(anchorId("pos", pos.symbol), `สถานะถือครอง ${pos.symbol}`);
  return (
    <tr ref={ref}>
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
      <td className="num">{amount(pos.cost_price, pos.currency, 4)}</td>
      <td className="num">{amount(pos.last_price, pos.currency, 4)}</td>
      <td className={`num ${pnlClass(pos.unrealized_profit_loss)}`}>
        {amount(pos.unrealized_profit_loss, pos.currency)}
      </td>
    </tr>
  );
}

/** One open-order row — its own component so each row can register its own
 *  agent anchor; the anchor uses the 1-based row number, never an order id. */
function OrderRow({ order, n }: { order: OpenOrder; n: number }) {
  const ref = useAgentAnchor(
    anchorId("order", String(n)),
    `คำสั่งที่ยังทำงาน ลำดับ ${n}${order.symbol ? ` · ${order.symbol}` : ""}`,
  );
  return (
    <tr ref={ref}>
      <td>
        {order.symbol ?? "--"}
        {order.combo_type && order.combo_type !== "NORMAL" && <small className="pf-strat"> {order.combo_type}</small>}
      </td>
      <td>{order.instrument_type === "OPTION" ? "Option" : order.instrument_type === "EQUITY" ? "หุ้น" : order.instrument_type ?? "--"}</td>
      <td className={order.side === "SELL" ? "pos" : order.side === "BUY" ? "neg" : ""}>{order.side === "SELL" ? "ขาย" : order.side === "BUY" ? "ซื้อ" : order.side ?? "--"}</td>
      <td>{[order.order_type, order.time_in_force].filter(Boolean).join(" · ") || "--"}</td>
      <td className="num">{order.total_quantity ?? "--"}{order.filled_quantity != null && order.filled_quantity > 0 && <small> ({order.filled_quantity})</small>}</td>
      <td className="num">{order.limit_price ?? "--"}</td>
      <td>{order.status ?? "--"}</td>
      <td title={order.place_time ?? undefined}>{order.place_time ? thaiDateWithDays(order.place_time.slice(0, 10)) : "--"}</td>
    </tr>
  );
}
