/** Easy Mode home — the market clock, the agent's one idea for tonight, the
 *  contracts already sold, and the links into the flow. Ported from the
 *  prototype; every dynamic sentence comes from the EasyIdeas contract. */

import { useEffect, useState } from "react";
import type { EasyIdeas, EasyProfile } from "../types/easy";
import { EzIcon } from "./icons";
import { clockLine, daysUntil, expiryTh, sourceLabel, usd } from "./util";

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

export function EasyHome(p: Props) {
  const [clock, setClock] = useState(() => clockLine(new Date()));
  useEffect(() => {
    const t = window.setInterval(() => setClock(clockLine(new Date())), 60_000);
    return () => window.clearInterval(t);
  }, []);

  const rec = p.ideas?.ideas.find(i => i.recommended) ?? p.ideas?.ideas[0] ?? null;
  const sample = p.browserPreview || p.ideas?.source === "DEMO";

  return (
    <section className="ez-screen">
      <div className="ez-clock">
        <span className={`ez-dot ${clock.open ? "open" : ""}`} />
        <span>{clock.text}</span>
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
            <p className="ez-say" dangerouslySetInnerHTML={{ __html: p.ideas.agent_say_th }} />
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
                    <span className="ez-src">{sourceLabel(p.ideas.source)}</span>
                    {sample && <span className="ez-src" style={{ color: "var(--amber)", borderColor: "#5a4724" }}>ข้อมูลตัวอย่าง</span>}
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
            ) : (
              <>
                <div>
                  <div className="ez-what">
                    {p.ideas.symbol} · คืนนี้ยังไม่มีไอเดียที่พอดีกับที่คุณตั้งไว้
                    <span className="ez-src">{sourceLabel(p.ideas.source)}</span>
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

function HeldRow({ h, first }: { h: HeldContract; first: boolean }) {
  const days = h.daysLeft ?? daysUntil(h.expiry);
  const away = h.spot != null ? ((h.spot - h.strike) / h.spot) * 100 : null;
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
        <span className="ez-status-ok">ปลอดภัย</span>
        {h.spot != null && np != null && away != null && (
          <div
            className="ez-pline"
            role="img"
            aria-label={`ราคา ${h.symbol} ตอนนี้ $${h.spot.toFixed(2)} ${away >= 0 ? "สูงกว่า" : "ต่ำกว่า"}จุดที่ต้องซื้อหุ้น $${h.strike} อยู่ ${Math.abs(away).toFixed(1)}%`}
          >
            <span className="ez-track" />
            <span className="ez-plgap" style={{ left: `${Math.min(sp, np)}%`, width: `${Math.abs(np - sp)}%` }} />
            <span className="ez-mk strike" style={{ left: `${sp}%` }} />
            <span className="ez-mk now" style={{ left: `${np}%` }} />
            <span className="ez-lb top" style={{ left: `${sp}%` }}>ต้องซื้อหุ้นถ้าต่ำกว่า ${h.strike}</span>
            <span className="ez-lb bot" style={{ left: `${np}%` }}>ตอนนี้ ${h.spot.toFixed(2)}</span>
            <span className="ez-lb bot" style={{ left: `${(sp + np) / 2}%`, color: "var(--green)" }}>ห่าง {Math.abs(away).toFixed(1)}%</span>
          </div>
        )}
      </div>
    </>
  );
}
