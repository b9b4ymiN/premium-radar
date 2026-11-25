/** Easy Mode onboarding — the two opening questions (money → risk), ported
 *  from the approved prototype with the stock-picking step removed: the
 *  assistant now proposes ideas from the whole market, so onboarding only
 *  collects the budget and how often the user accepts having to buy shares. */

import { useEffect, useState } from "react";
import type { EasyProfile, RiskLevel } from "../types/easy";
import { EzIcon } from "./icons";
import { usd } from "./util";
import { SAMPLE_THB } from "./sample";

export function Onboarding({ step, profile, onPatch, onGo }: {
  step: 1 | 2;
  profile: EasyProfile;
  onPatch: (patch: Partial<EasyProfile>) => void;
  onGo: (screen: "ob1" | "ob2" | "home") => void;
}) {
  const [cashText, setCashText] = useState(profile.cash ? profile.cash.toLocaleString("en-US") : "");
  useEffect(() => { setCashText(profile.cash ? profile.cash.toLocaleString("en-US") : ""); }, [profile.cash]);

  if (step === 1) {
    const n = parseInt(cashText.replace(/[^\d]/g, ""), 10) || 0;
    const perShare = n % 100 === 0 ? (n / 100).toLocaleString("en-US") : (n / 100).toFixed(2);
    return (
      <section className="ez-screen">
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
          {n ? `≈ ${Math.round(n * SAMPLE_THB).toLocaleString("en-US")} บาท (คิดที่ ${SAMPLE_THB} บาท/ดอลลาร์)` : "ใส่จำนวนเงินเป็นดอลลาร์"}
        </p>
        <p className="ez-note ez-dim" style={{ marginTop: 14 }}>
          <EzIcon name="info" />
          <span>
            {n
              ? `1 สัญญา คือสัญญาว่าจะซื้อหุ้น 100 หุ้น ถ้าราคาลงมาถึงราคาที่ตกลงไว้ งบ ${usd(n)} จึงใช้ได้กับหุ้นที่ราคาตกลงไม่เกินราว $${perShare} ต่อหุ้น — ผู้ช่วยจะหาให้เฉพาะตัวที่งบนี้ทำได้`
              : "1 สัญญา คือสัญญาว่าจะซื้อหุ้น 100 หุ้น งบยิ่งมาก ยิ่งเลือกหุ้นที่ราคาแพงขึ้นได้ — ผู้ช่วยจะหาให้เฉพาะตัวที่งบนี้ทำได้"}
          </span>
        </p>
        <div className="ez-actions"><button className="ez-btn primary" onClick={() => onGo("ob2")}>ต่อไป</button></div>
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
      <button className="ez-back" onClick={() => onGo("ob1")}><EzIcon name="back" />ย้อนกลับ</button>
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
