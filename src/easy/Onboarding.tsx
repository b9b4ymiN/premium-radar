/** Easy Mode onboarding — the three opening questions (stocks → money →
 *  risk), ported from the approved prototype. The same stock/stance editor
 *  also backs step 1 of the decision flow ("หุ้นที่ฉันสนใจ"). */

import { useEffect, useState } from "react";
import type { EasyProfile, RiskLevel, Stance } from "../types/easy";
import { EzIcon } from "./icons";
import { usd } from "./util";
import { SAMPLE_THB } from "./sample";

/** Per-stock "ถ้าราคาลง อยากได้หุ้นตัวนั้นไว้ถือไหม" toggle. `subline` supplies the
 *  small second line under the ticker (company · price); it returns null when
 *  the price is unknown — never a dash placeholder. */
export function StockStanceList({ stocks, onStance, subline }: {
  stocks: EasyProfile["stocks"];
  onStance: (symbol: string, stance: Stance) => void;
  subline?: (symbol: string) => string | null;
}) {
  return (
    <>
      {stocks.map(s => {
        const sub = subline?.(s.symbol) ?? null;
        return (
          <div className="ez-stockrow" key={s.symbol}>
            <div className="ez-ticker">
              {s.symbol}
              {sub && <small>{sub}</small>}
            </div>
            <div className="ez-seg2" role="group" aria-label={`ถ้าราคาลง อยากได้ ${s.symbol} ไว้ถือไหม`}>
              <button className="yes" aria-pressed={s.stance === "want"} onClick={() => onStance(s.symbol, "want")}>อยากถือ</button>
              <button className="no" aria-pressed={s.stance === "avoid"} onClick={() => onStance(s.symbol, "avoid")}>ไม่อยากถือ</button>
            </div>
          </div>
        );
      })}
    </>
  );
}

export function Onboarding({ step, profile, onPatch, onGo, browserPreview, subline }: {
  step: 1 | 2 | 3;
  profile: EasyProfile;
  onPatch: (patch: Partial<EasyProfile>) => void;
  onGo: (screen: "ob1" | "ob2" | "ob3" | "home") => void;
  browserPreview: boolean;
  subline?: (symbol: string) => string | null;
}) {
  const [cashText, setCashText] = useState(profile.cash ? profile.cash.toLocaleString("en-US") : "");
  useEffect(() => { setCashText(profile.cash ? profile.cash.toLocaleString("en-US") : ""); }, [profile.cash]);

  if (step === 1) {
    return (
      <section className="ez-screen">
        <h1>หุ้นตัวไหนที่คุณสนใจ</h1>
        <p className="ez-lead">บอกด้วยว่า ถ้าราคาลงมา คุณ<strong>อยากได้หุ้นตัวนั้นไว้ถือ</strong>ไหม ผู้ช่วยจะใช้คำตอบนี้เลือกไอเดียให้เหมาะกับคุณ</p>
        <div className="ez-panel ez-gap-l">
          <StockStanceList
            stocks={profile.stocks}
            subline={subline}
            onStance={(symbol, stance) =>
              onPatch({ stocks: profile.stocks.map(s => (s.symbol === symbol ? { ...s, stance } : s)) })}
          />
        </div>
        {browserPreview && (
          <p className="ez-note ez-dim" style={{ marginTop: 14 }}>
            <EzIcon name="info" />
            <span>ตัวอย่างนี้ใส่ NVDA กับ VLO ไว้ให้แล้ว ในแอปจริงพิมพ์ชื่อหุ้นเพิ่มเอง หรือดึงจากลิสต์ใน Webull ได้</span>
          </p>
        )}
        <div className="ez-actions"><button className="ez-btn primary" onClick={() => onGo("ob2")}>ต่อไป</button></div>
      </section>
    );
  }

  if (step === 2) {
    const n = parseInt(cashText.replace(/[^\d]/g, ""), 10) || 0;
    return (
      <section className="ez-screen">
        <button className="ez-back" onClick={() => onGo("ob1")}><EzIcon name="back" />ย้อนกลับ</button>
        <h1>ตั้งใจใช้เงินเท่าไร</h1>
        <p className="ez-lead">เงินก้อนนี้ต้องพร้อมจ่ายจริง เพราะถ้าราคาหุ้นลงมาถึงราคาที่ตกลงไว้ คุณต้องใช้เงินนี้ซื้อหุ้น 100 หุ้น</p>
        <label className="ez-cash ez-gap-l">
          <span>$</span>
          <input
            inputMode="numeric"
            value={cashText}
            aria-label="เงินที่ตั้งใจใช้ (ดอลลาร์)"
            onChange={e => {
              setCashText(e.target.value.replace(/[^\d]/g, "").replace(/\B(?=(\d{3})+(?!\d))/g, ","));
              onPatch({ cash: parseInt(e.target.value.replace(/[^\d]/g, ""), 10) || 0 });
            }}
            onBlur={() => setCashText(n ? n.toLocaleString("en-US") : "")}
          />
        </label>
        <p className="ez-muted" style={{ marginTop: 10 }}>
          {n ? `ประมาณ ${Math.round(n * SAMPLE_THB).toLocaleString("en-US")} บาท (อัตราแลกเปลี่ยนตัวอย่าง)` : "ใส่จำนวนเงินเป็นดอลลาร์"}
        </p>
        <div className="ez-actions"><button className="ez-btn primary" onClick={() => onGo("ob3")}>ต่อไป</button></div>
      </section>
    );
  }

  const risks: Array<{ v: RiskLevel; t: string; d: string }> = [
    { v: 1, t: "ไม่บ่อย · ประมาณ 1 ใน 10 ครั้ง", d: "ได้เงินน้อยลง แต่ส่วนใหญ่จะไม่ต้องซื้อหุ้น" },
    { v: 2, t: "พอรับได้ · ประมาณ 2 ใน 10 ครั้ง", d: "ได้เงินพอสมควร เหมาะกับหุ้นที่อยากถืออยู่แล้ว" },
    { v: 3, t: "บ่อยได้ · ประมาณ 3 ใน 10 ครั้ง", d: "ได้เงินมากที่สุด แต่ต้องพร้อมซื้อหุ้นบ่อย" },
  ];
  return (
    <section className="ez-screen">
      <button className="ez-back" onClick={() => onGo("ob2")}><EzIcon name="back" />ย้อนกลับ</button>
      <h1>ยอมให้ต้องซื้อหุ้นได้บ่อยแค่ไหน</h1>
      <p className="ez-lead">ยิ่งยอมบ่อย เงินที่ได้ต่อครั้งยิ่งมาก แต่ก็มีโอกาสต้องซื้อหุ้นจริงมากขึ้นตามไปด้วย</p>
      <div className="ez-list ez-gap-l">
        {risks.map(r => (
          <button key={r.v} className="ez-choice" aria-pressed={profile.risk === r.v} onClick={() => onPatch({ risk: r.v })}>
            <span className="ez-radio" />
            <span><span className="ez-t">{r.t}</span><br /><span className="ez-d">{r.d}</span></span>
          </button>
        ))}
      </div>
      <p className="ez-note ez-dim" style={{ marginTop: 14 }}>
        <EzIcon name="info" />
        <span>ตอนนี้ตั้งเงินไว้ {usd(profile.cash)} ต่อ 1 สัญญา</span>
      </p>
      <div className="ez-actions"><button className="ez-btn primary" onClick={() => onGo("home")}>เสร็จแล้ว ไปดูไอเดียคืนนี้</button></div>
    </section>
  );
}
