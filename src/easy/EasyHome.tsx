/** Easy Mode home — the market clock, the agent's one idea for tonight, the
 *  contracts already sold, and the links into the flow. Ported from the
 *  prototype; every dynamic sentence comes from the EasyIdeas contract. */

import { useEffect, useState } from "react";
import type { EasyIdeas, EasyProfile } from "../types/easy";
import { marketClockText, usSession } from "../components/shared/usSession";
import { EzIcon } from "./icons";
import { badgeText, daysUntil, expiryTh, usd } from "./util";

export interface HeldContract {
  symbol: string;
  strike: number;
  expiry: string;
  daysLeft: number;
  credit: number | null;
  spot: number | null;
  avoid: boolean;
}

interface Props {
  profile: EasyProfile;
  ideas: EasyIdeas | null;
  ideasBusy: boolean;
  ideasError: string | null;
  /** Another stock the profile tracks, with its plain-Thai skip reason. */
  skip: EasyIdeas | null;
  held: HeldContract[];
  browserPreview: boolean;
  onRetry: () => void;
  onOpenIdeas: () => void;
  onGo: (screen: "s1" | "ob1") => void;
}

/** Beginner wording over the pro clock: same DST-correct times, but no
 *  pre/after-hours session talk, and every time is labelled Thai time. */
function easyClockText(now: Date): string {
  const t = marketClockText(now).replace(/(\d{2}:\d{2}) น\./g, "$1 น. เวลาไทย");
  const rest = t.slice(t.indexOf("·") + 1).trim();
  switch (usSession(now)) {
    case "regular": return `ตลาดหุ้นอเมริกาเปิดอยู่ · ${rest}`;
    case "after": return `ตลาดหุ้นอเมริกาปิดแล้ว · ${rest.replace(/^ตลาดปกติ/, "")}`;
    case "pre": return `ตลาดหุ้นอเมริกายังไม่เปิด · ${rest.replace(/^ตลาดปกติ/, "")}`;
    default:
      return t.startsWith("ตลาดปิดวันหยุด")
        ? `ตลาดหุ้นอเมริกาปิดวันหยุดสุดสัปดาห์ · ${rest.replace("เปิดวันจันทร์", "เปิดคืนวันจันทร์")}`
        : `ตลาดหุ้นอเมริกายังไม่เปิด · ${rest}`;
  }
}

export function EasyHome(p: Props) {
  // DST-correct times from the pro clock, worded for a beginner; the dot is
  // lit only while the regular New York session is running
  const [clock, setClock] = useState(() => {
    const now = new Date();
    return { open: usSession(now) === "regular", text: easyClockText(now) };
  });
  useEffect(() => {
    const t = window.setInterval(() => {
      const now = new Date();
      setClock({ open: usSession(now) === "regular", text: easyClockText(now) });
    }, 60_000);
    return () => window.clearInterval(t);
  }, []);

  // the agent's pick is exactly the recommended idea — no silent fallback
  const rec = p.ideas?.ideas.find(i => i.recommended) ?? null;
  const sample = p.browserPreview || p.ideas?.source === "DEMO";
  const badge = p.ideas ? (
    <span className={`ez-src${sample ? " amber" : ""}`}>{badgeText(p.ideas.source, sample)}</span>
  ) : null;

  // The home sentence is UI copy built here from the profile and the
  // recommended idea, exactly as the prototype's renderHome; the contract's
  // agent_say_th is the IDEAS-step (3-choices) sentence and stays on step 2.
  const homeSay = (() => {
    const d = p.ideas;
    if (!d) return null;
    if (d.ideas.length === 0) return `${d.symbol} ยังไม่มีไอเดียที่ดีพอสำหรับคืนนี้`;
    if (rec == null) {
      const minCost = Math.min(...d.ideas.map(i => i.cost));
      return `คืนนี้ยังไม่มีไอเดีย ${d.symbol} ที่ใช้เงินไม่เกิน <strong>${usd(p.profile.cash)}</strong> เพราะต้องกันเงินไว้อย่างน้อย ${usd(minCost)} ต่อ 1 สัญญา`;
    }
    const want = p.profile.stocks.find(s => s.symbol.toUpperCase() === d.symbol.toUpperCase())?.stance !== "avoid";
    const tail = p.skip ? ` ส่วน ${p.skip.symbol} ยังไม่มีไอเดียที่ดีพอ` : "";
    return `คืนนี้มี <strong>1 ไอเดีย</strong>ที่ตรงกับที่คุณตั้งไว้ คือ ${d.symbol}${want ? " ซึ่งคุณอยากถืออยู่แล้ว" : " (คุณตั้งไว้ว่าไม่อยากถือ ผมจึงเลือกทางที่โอกาสต้องซื้อหุ้นต่ำ)"}${tail}`;
  })();

  return (
    <section className="ez-screen">
      <div className="ez-clock">
        <span className={`ez-dot ${clock.open ? "open" : ""}`} />
        {/* the Thai time portion ("HH:MM น.") renders bold, as in the prototype */}
        <span>
          {clock.text.split(/(\d{2}:\d{2} น\. เวลาไทย)/).map((part, i) =>
            i % 2 === 1 ? <b key={i}>{part}</b> : part)}
        </span>
      </div>

      <div className="ez-agent ez-gap-l">
        <span className="ez-avatar"><EzIcon name="radar" /></span>
        <div>
          <div className="ez-agent-name">ผู้ช่วย</div>
          {p.ideasError ? (
            <p className="ez-say">
              ดึงไอเดียไม่สำเร็จ — {p.ideasError}
              <br />
              <button className="ez-back" style={{ marginBottom: 0 }} onClick={p.onRetry}>ลองใหม่</button>
            </p>
          ) : p.ideas == null ? (
            <p className="ez-say ez-muted">ผู้ช่วยกำลังคำนวณไอเดียของคืนนี้ รอสักครู่</p>
          ) : (
            homeSay && <p className="ez-say" dangerouslySetInnerHTML={{ __html: homeSay }} />
          )}
        </div>
      </div>

      {p.ideas && !p.ideasError && (
        <div className="ez-panel" style={{ marginTop: 20 }}>
          <div className="ez-idea">
            {rec ? (
              <>
                <div>
                  <div className="ez-what">
                    {p.ideas.symbol} · รับเงินตอนนี้ ถ้ายอมซื้อหุ้นที่ ${rec.strike}
                    {badge}
                  </div>
                  <div className="ez-big ez-money ez-pos">{usd(rec.credit)}</div>
                </div>
                <span className={`ez-chance ${rec.warn ? "warn" : ""}`}>
                  {rec.warn && <EzIcon name="alert" className="ez-warn" />}โอกาสต้องซื้อหุ้น {Math.round(rec.assign_pct)}%
                </span>
                <div className="ez-sub">
                  รอ {rec.days} วัน ถึง{expiryTh(rec.expiry)} · ถูกกว่าราคาตอนนี้ {rec.below_spot_pct.toFixed(1)}%
                </div>
              </>
            ) : p.ideas.ideas.length > 0 ? (
              // cash too short for every idea — the prototype's not-enough-cash state
              <>
                <div>
                  <div className="ez-what">
                    {p.ideas.symbol} · เงินที่ตั้งไว้ยังไม่พอ
                    {badge}
                  </div>
                  <div className="ez-big ez-money">—</div>
                </div>
                <span className="ez-chance">ต้องกันไว้ {usd(Math.min(...p.ideas.ideas.map(i => i.cost)))}</span>
                <div className="ez-sub">ลองเพิ่มเงินที่ตั้งไว้ หรือเพิ่มหุ้นที่ราคาต่ำกว่านี้</div>
              </>
            ) : (
              <>
                <div>
                  <div className="ez-what">
                    {p.ideas.symbol} · คืนนี้ยังไม่มีไอเดียที่พอดีกับที่คุณตั้งไว้
                    {badge}
                  </div>
                  <div className="ez-big ez-money">—</div>
                </div>
                <div className="ez-sub">{p.ideas.skip_reason_th}</div>
              </>
            )}
          </div>
          {rec && (
            <div className="ez-actions" style={{ marginTop: 20 }}>
              <button className="ez-btn primary" onClick={p.onOpenIdeas}>ดูไอเดียนี้<EzIcon name="chev" /></button>
            </div>
          )}
          {p.skip?.skip_reason_th && (
            <>
              <div className="ez-rule" />
              <div className="ez-skip">
                <EzIcon name="pause" className="ez-warn" />
                <span>
                  <strong style={{ color: "var(--text)" }}>{p.skip.symbol} · ยังไม่แนะนำคืนนี้</strong>
                  <br />
                  {p.skip.skip_reason_th}
                </span>
              </div>
            </>
          )}
        </div>
      )}
      {p.ideasBusy && p.ideas == null && !p.ideasError && (
        <div className="ez-panel ez-loading" style={{ marginTop: 20 }} role="status">
          <span className="ez-spin" />
          <span>กำลังรอข้อมูลของ {p.profile.stocks[0]?.symbol ?? "หุ้นที่สนใจ"} — ผู้ช่วยจะแสดงไอเดียเมื่อคำนวณเสร็จ</span>
        </div>
      )}

      <h2 className="ez-gap-l">สัญญาที่ขายไปแล้ว</h2>
      <div className="ez-panel" style={{ marginTop: 14 }}>
        {p.held.length === 0 && (
          <p className="ez-muted">ยังไม่มีสัญญาที่ขายไปแล้ว — เมื่อคุณขายสัญญาแล้ว ผู้ช่วยจะติดตามให้ในหน้านี้</p>
        )}
        {p.held.map((h, i) => <HeldRow key={`${h.symbol}-${h.expiry}-${i}`} h={h} first={i === 0} />)}
        {p.held.length > 0 && (
          <>
            <div className="ez-rule" />
            <p className="ez-note">
              <EzIcon name="info" />
              <span>
                {p.held.every(h => h.avoid)
                  ? p.held.map(h => `คุณไม่อยากถือ ${h.symbol} ถ้าราคาลงมาใกล้ $${h.strike} ผู้ช่วยจะเตือนให้พิจารณา`).join(" ")
                  : "ถ้าราคาลงมาใกล้ราคาที่ต้องซื้อหุ้น ผู้ช่วยจะเตือนให้พิจารณา"}
                <strong style={{ color: "var(--text)" }}>ซื้อสัญญาคืน</strong>ก่อนหมดอายุ จะได้ไม่ต้องรับหุ้น
              </span>
            </p>
          </>
        )}
      </div>

      <div className="ez-panel quiet ez-gap-l" style={{ paddingTop: 8, paddingBottom: 8 }}>
        <button className="ez-linkrow" onClick={() => p.onGo("s1")}>
          <span>
            หุ้นที่ฉันสนใจ <span className="ez-dim">· {p.profile.stocks.length} ตัว · เงิน {usd(p.profile.cash)}</span>
          </span>
          <EzIcon name="chev" className="ez-chev" />
        </button>
        <button className="ez-linkrow" onClick={() => p.onGo("ob1")}>
          <span>ตั้งค่าใหม่ <span className="ez-dim">· เริ่มคำถาม 3 ข้อใหม่</span></span>
          <EzIcon name="chev" className="ez-chev" />
        </button>
      </div>
    </section>
  );
}

/** Where a held contract sits against its strike: the status label, the dot
 *  class, the colour the "ห่าง x%" label takes, and the soft gap fill pair
 *  that follows the same colour. Thresholds: ≥1.05×strike safe, between
 *  strike and 1.05×strike near, below strike already past the buy point. */
function heldStatus(spot: number | null, strike: number) {
  if (spot == null)
    return { cls: "none", label: "ยังไม่มีราคา", color: "var(--dim)", gap: { background: "#12332b", boxShadow: "inset 0 0 0 1px #1f6b55" } };
  if (spot >= strike * 1.05)
    return { cls: "", label: "ปลอดภัย", color: "var(--green)", gap: { background: "#12332b", boxShadow: "inset 0 0 0 1px #1f6b55" } };
  if (spot >= strike)
    return { cls: "warn", label: "ใกล้จุดที่ต้องซื้อหุ้น", color: "var(--amber)", gap: { background: "#3a2e1a", boxShadow: "inset 0 0 0 1px #6a5229" } };
  return { cls: "neg", label: "ต่ำกว่าจุดที่ต้องซื้อหุ้นแล้ว", color: "var(--red)", gap: { background: "#3a1c24", boxShadow: "inset 0 0 0 1px #6b2c38" } };
}

function HeldRow({ h, first }: { h: HeldContract; first: boolean }) {
  const days = h.daysLeft ?? daysUntil(h.expiry);
  const away = h.spot != null ? ((h.spot - h.strike) / h.spot) * 100 : null;
  const st = heldStatus(h.spot, h.strike);
  // price line geometry: a window around the strike and today's price
  const lo = h.spot != null ? Math.min(h.strike, h.spot) * 0.96 : h.strike * 0.96;
  const hi = h.spot != null ? Math.max(h.strike, h.spot) * 1.03 : h.strike * 1.1;
  const pos = (v: number) => Math.max(2, Math.min(98, ((v - lo) / (hi - lo)) * 100));
  const sp = pos(h.strike);
  const np = h.spot != null ? pos(h.spot) : null;
  return (
    <>
      {!first && <div className="ez-rule" />}
      <div className="ez-held">
        <div>
          <div className="ez-name">{h.symbol} · ยอมซื้อที่ ${h.strike} · หมดอายุ{expiryTh(h.expiry)}</div>
          <div className="ez-muted" style={{ fontSize: 15 }}>
            {h.credit != null && <>ได้เงินไปแล้ว <span className="ez-pos">${h.credit}</span> · </>}
            เหลือ {Math.max(0, days)} วัน
          </div>
        </div>
        <span className={"ez-status-ok" + (st.cls ? ` ${st.cls}` : "")}>{st.label}</span>
        {h.spot != null && np != null && away != null && (
          <div
            className="ez-pline"
            role="img"
            aria-label={`ราคา ${h.symbol} ตอนนี้ $${h.spot.toFixed(2)} ${away >= 0 ? "สูงกว่า" : "ต่ำกว่า"}จุดที่ต้องซื้อหุ้น $${h.strike} อยู่ ${Math.abs(away).toFixed(1)}%`}
          >
            <span className="ez-track" />
            <span
              className="ez-plgap"
              style={{
                left: `${Math.min(sp, np)}%`,
                width: `${Math.abs(np - sp)}%`,
                background: st.gap.background,
                boxShadow: st.gap.boxShadow,
              }}
            />
            <span className="ez-mk strike" style={{ left: `${sp}%` }} />
            <span className="ez-mk now" style={{ left: `${np}%` }} />
            <span className="ez-lb top" style={{ left: `${sp}%` }}>ต้องซื้อหุ้นถ้าต่ำกว่า ${h.strike}</span>
            <span className="ez-lb bot" style={{ left: `${np}%` }}>ตอนนี้ ${h.spot.toFixed(2)}</span>
            <span className="ez-lb bot" style={{ left: `${(sp + np) / 2}%`, color: st.color }}>ห่าง {Math.abs(away).toFixed(1)}%</span>
          </div>
        )}
      </div>
    </>
  );
}
