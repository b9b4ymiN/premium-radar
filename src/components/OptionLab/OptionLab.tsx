//! Option Lab — แผงวิเคราะห์สัญญา + freeze คำทำนาย (PLAN-LAB P9)
//! ตัวเลขทั้งหมดมาจาก Rust (lab_analyze) — component ทำแค่ format/render

import { useEffect, useState } from "react";
import type { EnrichedRow } from "../../types/screener";
import { SkelPanel } from "../shared/Skeleton";

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface ScenarioRow { label: string; s_t: number; pnl: number }
interface NamedProb { event_th: string; p: number; model_note: string }
interface LegAnalysis { max_profit: number; max_loss: number | null; breakeven: number | null; collateral: number | null }
interface CcAnalysis { max_profit: number; max_loss: number | null; breakeven: number | null; capital_today: number }
interface LabAnalysis {
  fees_round_trip: number;
  fees_note: string;
  leg: { open: LegAnalysis; round_trip: LegAnalysis };
  pop: NamedProb | null;
  p_itm: number | null;
  p_touch: NamedProb | null;
  delta: number;
  covered_call: CcAnalysis | null;
  call_capital_today?: number | null;
  scenarios: ScenarioRow[];
  cc_scenarios: ScenarioRow[] | null;
}

const usd = (v: number | null | undefined, digits = 2) =>
  v == null ? "--" : `${v < 0 ? "−" : ""}$${Math.abs(v).toLocaleString(undefined, { maximumFractionDigits: digits })}`;
const pct = (v: number | null | undefined, digits = 1) =>
  v == null ? "--" : `${(v * 100).toFixed(digits)}%`;

export default function OptionLab({
  row, symbol, spot, loading = false,
}: { row: EnrichedRow | null; symbol: string; spot: number; loading?: boolean }) {
  const [a, setA] = useState<LabAnalysis | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [covered, setCovered] = useState(false);
  const [freezeMsg, setFreezeMsg] = useState<string | null>(null);

  const isCall = row?.q.option_type === "C";
  const iv = row?.iv ?? 0;
  const dteYears = row ? row.dte / 365.0 : 0;

  useEffect(() => {
    setFreezeMsg(null);
    if (!row || !spot || iv <= 0 || dteYears <= 0) { setA(null); setErr(row ? "ยังไม่มี IV/DTE ที่ใช้ได้ของสัญญานี้" : "เลือกสัญญาจากตารางก่อน"); return; }
    let live = true;
    invokeCmd<LabAnalysis>("lab_analyze", {
      input: {
        symbol, spot, strike: row.q.strike, premium: row.prem ?? 0,
        iv, t: dteYears, is_call: isCall, covered: covered && isCall,
        basis: covered && isCall ? spot : null,
      },
    }).then(r => { if (live) { r.scenarios.sort((x, y) => x.s_t - y.s_t); if (r.cc_scenarios) r.cc_scenarios.sort((x, y) => x.s_t - y.s_t); setA(r); setErr(null); } })
      .catch(e => { if (live) { setA(null); setErr(String(e)); } });
    return () => { live = false; };
  }, [row?.q.symbol, spot, iv, covered, symbol, isCall, dteYears, row]);

  if (!row) {
    // สัญญายังไม่มีเพราะ chain กำลังโหลด = แผง skeleton; โหลดเสร็จแล้วแต่ไม่ได้เลือก = ข้อความเดิม
    if (loading) return <SkelPanel lines={6} />;
    return <div className="lab-empty">ยังไม่ได้เลือกสัญญา — กดแถวในตาราง Monitor ก่อน แล้วกลับมาที่แท็บนี้</div>;
  }

  const freeze = async () => {
    if (!a?.pop) return;
    try {
      const id = await invokeCmd<string>("lab_freeze_prediction", {
        input: {
          occ: row.q.symbol, p: a.pop.p, event_th: a.pop.event_th,
          model_note: a.pop.model_note, spot, strike: row.q.strike,
          premium: row.prem ?? 0, iv, dte_days: row.dte, expiry: row.q.expiry,
        },
      });
      setFreezeMsg(`✓ บันทึกคำทำนายแล้ว (${id}) — แก้ย้อนหลังไม่ได้ จะเทียบผลเมื่อสัญญาหมดอายุ`);
    } catch (e) {
      setFreezeMsg(`บันทึกไม่สำเร็จ: ${e}`);
    }
  };

  const leg = a?.leg.round_trip;
  return (
    <div className="optionlab">
      {/* summary band 3 ช่องตามแผน §5 */}
      <div className="lab-summary">
        <div className="lab-cell">
          <label>กำไรสูงสุด (จนถึงหมดอายุ)</label>
          <strong className="pos">{usd(leg?.max_profit)}</strong>
          <small>หลัง fee เข้า+ออก {usd(a?.fees_round_trip)} · ไม่รวม slippage</small>
        </div>
        <div className="lab-cell">
          <label>{a?.pop ? "โอกาสกำไร (POP)" : "โอกาสกำไร"}</label>
          <strong>{a?.pop ? pct(a.pop.p) : "--"}</strong>
          <small>{a?.pop ? "ราคาปิดวันหมดอายุเกินจุดคุ้มทุน · BS proxy" : "รอ IV ที่ใช้ได้"}</small>
        </div>
        <div className="lab-cell">
          <label>{isCall ? "ทุนเทียบวันนี้ (หุ้น 100)" : "ทุนที่ต้องผูก (CSP)"}</label>
          <strong>{isCall ? usd(a?.call_capital_today ?? a?.covered_call?.capital_today ?? null, 0) : usd(leg?.collateral, 0)}</strong>
          <small>{isCall ? "มูลค่าหุ้น ณ วันนี้ (คุ้มคลุมสัญญา)" : "เงินสดเต็มจำนวน strike · gross"}</small>
        </div>
      </div>

      {err && <div className="lab-err">{err}</div>}

      {/* กราฟ payoff — polyline จาก scenarios ที่ Rust คำนวณ */}
      {a && a.scenarios.length > 1 && (
        <div className="lab-chart">
          <div className="panel-heading" style={{ minHeight: 30 }}>กำไร/ขาดทุน ณ วันหมดอายุ (USD ต่อสัญญา)</div>
          <PayoffSvg rows={a.scenarios} be={leg?.breakeven ?? null} strike={row.q.strike} spot={spot} />
        </div>
      )}

      {/* ความน่าจะเป็น — ทุกตัวมีชื่อเหตุการณ์ */}
      {a && (
        <div className="lab-probs">
          <div className="lab-prob"><label>POP — {a.pop?.event_th ?? "--"}</label><strong>{pct(a.pop?.p)}</strong></div>
          <div className="lab-prob"><label>จบในตัวเงิน (ITM) — คนละเหตุการณ์กับ POP</label><strong>{pct(a.p_itm)}</strong></div>
          <div className="lab-prob"><label>{a.p_touch?.event_th ?? "แตะ strike (ประมาณ)"}</label><strong>{pct(a.p_touch?.p)}</strong></div>
          <div className="lab-prob"><label>Delta (ความอ่อนไหว)</label><strong>{a.delta.toFixed(3)}</strong></div>
          <small className="lab-note">{a.pop?.model_note} · |delta| ไม่ใช่อัตราถูก assigned</small>
        </div>
      )}

      {/* CC รวมหุ้น */}
      {isCall && (
        <div className="lab-cc">
          <label className="lab-toggle">
            <input type="checkbox" checked={covered} onChange={e => setCovered(e.target.checked)} />
            ดูแบบ Covered Call รวมหุ้น (สมมติซื้อหุ้นวันนี้ที่ {usd(spot)})
          </label>
          {covered && a?.covered_call && (
            <div className="lab-ccline">
              กำไรสูงสุดแบบคุ้มหุ้น <strong className="pos">{usd(a.covered_call.max_profit)}</strong>
              {" · "}จุดคุ้มทุน <strong>{usd(a.covered_call.breakeven)}</strong>
              {" · "}ขาดทุนสูงสุด (หุ้นเหลือศูนย์) <strong className="neg">{usd(a.covered_call.max_loss)}</strong>
            </div>
          )}
        </div>
      )}

      {/* ตาราง scenario */}
      {a && (
        <div className="lab-tablewrap">
          <table className="screener-table lab-table">
            <thead><tr><th>สถานการณ์ (ราคาหุ้นตอนหมดอายุ)</th><th className="num">กำไร/ขาดทุน</th>{covered && isCall && <th className="num">แบบคุ้มหุ้น</th>}</tr></thead>
            <tbody>
              {a.scenarios.map((s, i) => (
                <tr key={i}>
                  <td>{s.label}</td>
                  <td className={`num ${s.pnl >= 0 ? "pos" : "neg"}`}>{usd(s.pnl)}</td>
                  {covered && isCall && (
                    <td className={`num ${(a.cc_scenarios?.[i]?.pnl ?? 0) >= 0 ? "pos" : "neg"}`}>
                      {usd(a.cc_scenarios?.[i]?.pnl ?? null)}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
          <small className="lab-note">{a.fees_note} · สัญญา standard 100 หุ้น</small>
        </div>
      )}

      {/* freeze คำทำนาย (P9.4) */}
      <div className="lab-freeze">
        <button className="lab-freeze-btn" onClick={freeze} disabled={!a?.pop}>
          📌 บันทึกคำทำนายนี้ (freeze)
        </button>
        <small>{freezeMsg ?? "บันทึก POP + ตัวเลขที่ใช้ตัดสินใจวันนี้ไว้เทียบผลจริงตอนหมดอายุ — แก้ย้อนหลังไม่ได้"}</small>
      </div>
    </div>
  );
}

/// Payoff SVG — รับจุดจาก Rust แล้ววาด polyline + เส้นศูนย์ + BE/strike/spot
function PayoffSvg({ rows, be, strike, spot }: { rows: ScenarioRow[]; be: number | null; strike: number; spot: number }) {
  const W = 560, H = 180, PAD = 34;
  const xs = rows.map(r => r.s_t);
  const minX = Math.min(...xs), maxX = Math.max(...xs, spot * 1.05);
  const ys = rows.map(r => r.pnl);
  const minY = Math.min(...ys, 0), maxY = Math.max(...ys, 0);
  const sx = (v: number) => PAD + ((v - minX) / (maxX - minX || 1)) * (W - PAD * 2);
  const sy = (v: number) => H - PAD - ((v - minY) / (maxY - minY || 1)) * (H - PAD * 2);
  const pts = rows.map(r => `${sx(r.s_t)},${sy(r.pnl)}`).join(" ");
  return (
    <svg viewBox={`0 0 ${W} ${H}`} style={{ width: "100%", height: "auto" }}>
      <line x1={PAD} y1={sy(0)} x2={W - PAD} y2={sy(0)} stroke="#292f39" strokeWidth="1" />
      <polyline points={pts} fill="none" stroke="#4b8cf7" strokeWidth="2" />
      {be != null && <line x1={sx(be)} y1={PAD} x2={sx(be)} y2={H - PAD} stroke="#f0bd70" strokeDasharray="4 3" strokeWidth="1" />}
      <line x1={sx(strike)} y1={PAD} x2={sx(strike)} y2={H - PAD} stroke="#a2adbc" strokeDasharray="2 4" strokeWidth="1" />
      <line x1={sx(spot)} y1={PAD} x2={sx(spot)} y2={H - PAD} stroke="#4b8cf7" strokeDasharray="2 4" strokeWidth="1" opacity="0.7" />
      <text x={sx(be ?? strike)} y={PAD - 6} fill="#f0bd70" fontSize="9" textAnchor="middle">คุ้มทุน {be != null ? be.toFixed(1) : "--"}</text>
      <text x={sx(spot)} y={H - 6} fill="#4b8cf7" fontSize="9" textAnchor="middle">ราคาวันนี้ {spot.toFixed(1)}</text>
    </svg>
  );
}
