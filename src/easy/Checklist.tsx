/** Step 4 — the three-item checklist before deciding, ported from the
 *  prototype. The "ready" button unlocks only when every item is ticked, and
 *  the cash item is disabled (and marked เงินไม่พอ) when the reserved cash
 *  exceeds what the user set aside — a disabled item never counts as ticked,
 *  so a cash-short user cannot pass the gate. */

import { useEffect, useState } from "react";
import type { EasyIdea, EasyIdeas, EasyProfile } from "../types/easy";
import { EzIcon } from "./icons";
import { expiryTh, usd } from "./util";

interface Props {
  idea: EasyIdea;
  ideas: EasyIdeas;
  profile: EasyProfile;
  onBack: () => void;
  onReady: () => void;
  onNotYet: () => void;
}

export function Checklist(p: Props) {
  const { idea: o, ideas: d, profile } = p;
  const want = d.stance === "want";
  const enough = profile.cash >= o.cost;

  const items: Array<[string, string]> = [
    [`ถ้า${expiryTh(o.expiry)} ${d.symbol} ต่ำกว่า $${o.strike} ฉันต้องซื้อ 100 หุ้น จ่าย ${usd(o.cost)}`, "ฉันเข้าใจว่านี่ไม่ใช่แค่รับเงิน แต่เป็นสัญญาว่าจะซื้อหุ้น"],
    [`ฉันมีเงินสด ${usd(o.cost)} ในบัญชี Webull`, `Webull จะกันเงินนี้ไว้ ใช้ทำอย่างอื่นไม่ได้จนถึง${expiryTh(o.expiry)} · คุณตั้งไว้ ${usd(profile.cash)}`],
    want
      ? ["ถ้าได้หุ้น ฉันโอเคที่จะถือ " + d.symbol, "คุณตั้งไว้ว่าอยากถือ " + d.symbol]
      : ["ถ้าได้หุ้น ฉันรู้ว่าจะซื้อสัญญาคืนหรือขายหุ้นออก", "คุณตั้งไว้ว่าไม่อยากถือ " + d.symbol],
  ];

  const [checked, setChecked] = useState<boolean[]>([false, false, false]);
  useEffect(() => { setChecked([false, false, false]); }, [o.occ]);

  // every item must be ticked by hand; a disabled item (cash short) can never
  // count, so the ready button stays disabled until the user can actually pay
  const all = checked.every(c => c);

  return (
    <section className="ez-screen">
      <button className="ez-back" onClick={p.onBack}><EzIcon name="back" />ย้อนกลับ</button>
      <h1>ก่อนตัดสินใจ ลองเช็ก 3 ข้อนี้</h1>
      <p className="ez-lead">ติ๊กได้ครบเมื่อเข้าใจจริง ถ้ายังไม่แน่ใจข้อไหน ถามผู้ช่วยหรือเลือก “ยังไม่ทำ” ได้เสมอ</p>
      <div className="ez-list ez-gap-l">
        {items.map(([t, desc], i) => (
          <button
            key={i}
            className="ez-check"
            role="checkbox"
            aria-checked={checked[i]}
            disabled={i === 1 && !enough}
            onClick={() => setChecked(c => c.map((v, j) => (j === i ? !v : v)))}
          >
            <span className="ez-box"><EzIcon name="check" /></span>
            <span>
              <span className="ez-t">{t}</span>
              <span className="ez-d" style={{ display: "block" }}>
                {desc}
                {i === 1 && !enough && <span className="ez-neg"> · เงินไม่พอ</span>}
              </span>
            </span>
          </button>
        ))}
      </div>
      <div className="ez-actions">
        <button className="ez-btn primary" disabled={!all} onClick={p.onReady}>พร้อม · ดูวิธีกรอกใน Webull</button>
        <button className="ez-btn ghost" onClick={p.onNotYet}>ยังไม่ทำ</button>
      </div>
    </section>
  );
}
