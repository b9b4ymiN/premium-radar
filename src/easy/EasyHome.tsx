/** Easy Mode home — the market clock, the assistant's ranked ideas picked from
 *  the whole market, the contracts already sold, and the link into the flow.
 *  Every dynamic sentence comes from the UniverseIdeas contract; the only
 *  numbers rendered here are the ones it carries. */

import { useEffect, useState } from "react";
import type { EasyProfile, Stance, UniverseIdeas, UniversePick, WatchCash } from "../types/easy";
import { anchorId, useAgentAnchor } from "../agent/anchors";
import { marketClockText, usSession } from "../components/shared/usSession";
import { EzIcon } from "./icons";
import { asOfTh, badgeText, daysUntil, expiryTh, usd } from "./util";

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
  universe: UniverseIdeas | null;
  universeError: string | null;
  held: HeldContract[];
  browserPreview: boolean;
  onRetry: () => void;
  onOpenPick: (symbol: string) => void;
  onStance: (symbol: string, stance: Stance) => void;
  onGo: (screen: "ob1") => void;
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

  const u = p.universe;
  const sample = p.browserPreview || u?.source === "DEMO";

  // the small dim meta line under the agent's sentence: market note, data
  // time (Bangkok) and how many stocks the assistant looked at
  const meta = u
    ? [
        u.market_note_th,
        asOfTh(u.as_of) ? `ข้อมูลของ ${asOfTh(u.as_of)}` : "",
        `ผู้ช่วยดูหุ้น ${u.scanned_symbols} ตัว`,
      ].filter(Boolean).join(" · ")
    : "";

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
          <div className="ez-agent-name">
            ผู้ช่วย
            {u && <span className={`ez-src${sample ? " amber" : ""}`}>{badgeText(u.source, sample)}</span>}
          </div>
          {p.universeError ? (
            <p className="ez-say">
              ดึงไอเดียไม่สำเร็จ — {p.universeError}
              <br />
              <button className="ez-back" style={{ marginBottom: 0 }} onClick={p.onRetry}>ลองใหม่</button>
            </p>
          ) : u == null ? (
            <p className="ez-say ez-muted">ผู้ช่วยกำลังดูหุ้นทั้งตลาดเพื่อหาไอเดียของคืนนี้ รอสักครู่</p>
          ) : (
            <>
              <p className="ez-say">{u.agent_say_th}</p>
              {meta && <p className="ez-dim" style={{ fontSize: 14, marginTop: 6 }}>{meta}</p>}
            </>
          )}
        </div>
      </div>

      {p.universeError == null && u == null && (
        <div className="ez-panel ez-loading" style={{ marginTop: 20 }} role="status">
          <span className="ez-spin" />
          <span>กำลังไล่ดูหุ้นทั้งตลาด — ผู้ช่วยจะแสดงไอเดียที่คัดมาแล้วเมื่อคำนวณเสร็จ</span>
        </div>
      )}

      {u && p.universeError == null && u.budget_short && u.picks.length === 0 && (
        <div className="ez-panel" style={{ marginTop: 20 }}>
          <div className="ez-skip">
            <EzIcon name="pause" className="ez-warn" />
            <span>
              <strong style={{ color: "var(--text)" }}>งบที่ตั้งไว้ยังไม่พอ</strong>
              <br />
              {u.budget_short.text_th}
            </span>
          </div>
          <Contract100 cash={p.profile.cash} />
          <div className="ez-actions" style={{ marginTop: 20 }}>
            <button className="ez-btn primary" onClick={() => p.onGo("ob1")}>ปรับงบ</button>
          </div>
        </div>
      )}

      {u && p.universeError == null && !u.budget_short && u.picks.length === 0 && (
        <div className="ez-panel" style={{ marginTop: 20 }}>
          <p className="ez-muted">คืนนี้ยังไม่มีไอเดียที่ดีพอ</p>
          <p className="ez-dim" style={{ fontSize: 14, marginTop: 6 }}>{u.market_note_th}</p>
        </div>
      )}

      {u && p.universeError == null && u.picks.length > 0 && (
        <div className="ez-list ez-gap-l">
          {u.picks.map(pick => (
            <PickCard
              key={pick.idea.occ}
              pick={pick}
              stance={p.profile.stocks.find(s => s.symbol.toUpperCase() === pick.idea.symbol.toUpperCase())?.stance ?? null}
              onStance={p.onStance}
              onOpen={() => p.onOpenPick(pick.idea.symbol)}
            />
          ))}
        </div>
      )}

      {u && p.universeError == null && u.watch_cash.length > 0 && (
        <div className="ez-panel ez-gap-l">
          <div className="ez-wcash-title">เงินที่ต้องมีสำหรับหุ้นที่คุณสนใจ</div>
          {u.watch_cash.map(w => <WatchCashRow key={w.symbol} w={w} />)}
          <p className="ez-note ez-dim" style={{ fontSize: 13, marginTop: 12 }}>
            <EzIcon name="info" />
            <span>✓ = งบของคุณพอสำหรับสัญญาที่ผ่านเกณฑ์ของคืนนี้ · ✕ = ยังไม่พอหรือยังไม่มีตัวที่ผ่านเกณฑ์</span>
          </p>
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
        <button className="ez-linkrow" onClick={() => p.onGo("ob1")}>
          <span>
            ตั้งค่าใหม่ <span className="ez-dim">· เงิน {usd(p.profile.cash)} · ยอมให้ต้องซื้อหุ้นราว {p.profile.risk} ใน 10 ครั้ง</span>
          </span>
          <EzIcon name="chev" className="ez-chev" />
        </button>
      </div>
    </section>
  );
}

/** The 100-share contract explainer — collapsed by default; a native
 *  <details>/<summary> keeps it keyboard-focusable. The budget sentence is
 *  the only number computed here (cash ÷ 100, rounded down); the worked
 *  $26 → $2,600 example is static teaching text. */
function Contract100({ cash }: { cash: number }) {
  const ref = useAgentAnchor(anchorId("explain", "contract100"), "คำอธิบาย สัญญา 1 ตัว = 100 หุ้น");
  const perShare = Math.floor(cash / 100);
  return (
    <details className="ez-c100">
      <summary ref={ref}>สัญญา 1 ตัว = 100 หุ้น คืออะไร?</summary>
      <div className="ez-c100-body">
        <p>
          ขายสัญญา 1 ตัว คือให้คำสัญญาว่า "ถ้าหุ้นราคาลงมาถึงระดับที่นัดไว้ เราจะซื้อหุ้นนั้น 100 หุ้น"
          — ตลาดหุ้นอเมริกากำหนดให้ 1 สัญญาครอบคลุม 100 หุ้นเสมอ ขอซื้อทีละ 10 หุ้นไม่ได้
        </p>
        <p>
          เพราะคำสัญญานี้ผูกมัดเราไว้ Webull จึงต้องกันเงินสดไว้ก่อน เท่ากับ ราคาที่นัด × 100
          เช่น นัดซื้อที่ $26 → กันเงิน $2,600
        </p>
        <p>
          งบ {usd(cash)} ของคุณจึงใช้ได้กับหุ้นที่ราคาไม่เกินราว {usd(perShare)} ต่อหุ้น
        </p>
      </div>
    </details>
  );
}

/** One watched stock's "how much money do I need" row — the sentence is
 *  computed in Rust; here we only add the affordable ✓/✕ marker. */
function WatchCashRow({ w }: { w: WatchCash }) {
  const ref = useAgentAnchor(anchorId("watchcash", w.symbol), `เงินที่ต้องมี ${w.symbol}`);
  return (
    <div ref={ref} className="ez-wcash">
      <span className={`ez-wmark ${w.affordable ? "ez-pos" : "ez-neg"}`}>
        {w.affordable ? "✓" : "✕"}
      </span>
      <span>{w.text_th}</span>
    </div>
  );
}

/** One ranked idea from the universe scan: the money in plain sight first, the
 *  reasons and cautions under it, the OI note small and dim (unproven signal),
 *  then the single stance question and the way into the detail flow. */
function PickCard({ pick, stance, onStance, onOpen }: {
  pick: UniversePick;
  stance: Stance | null;
  onStance: (symbol: string, stance: Stance) => void;
  onOpen: () => void;
}) {
  const i = pick.idea;
  const when = expiryTh(i.expiry).replace(/^\S+ /, "");
  const cardRef = useAgentAnchor(anchorId("pick", i.symbol), `ไอเดีย ${i.symbol} ยอมซื้อที่ $${i.strike} (${when})`);
  const premiumRef = useAgentAnchor(anchorId("pick", i.symbol, "premium"), `เงินที่ได้ทันที ${i.symbol} ${i.strike} (${when})`);
  const assignRef = useAgentAnchor(anchorId("pick", i.symbol, "assign"), `โอกาสต้องซื้อหุ้น ${i.symbol} ${i.strike} (${when})`);
  return (
    <article ref={cardRef} className={`ez-pick${pick.rank === 1 ? " top" : ""}`}>
      <div className="ez-pick-head">
        <div>
          <div className="ez-what">
            {i.symbol} · ยอมซื้อที่ ${i.strike} (ถูกกว่าราคาตอนนี้ {i.below_spot_pct.toFixed(1)}%)
            {pick.rank === 1 && <span className="ez-tag">ผู้ช่วยแนะนำ</span>}
          </div>
          <div ref={premiumRef} className="ez-big ez-money ez-pos">{usd(i.credit)}</div>
          <div className="ez-sub">
            รับเงินตอนนี้ · กันเงินไว้ {usd(i.cost)} · รอ {i.days} วัน ถึง{expiryTh(i.expiry)}
          </div>
        </div>
        <span ref={assignRef} className={`ez-chance ${i.warn ? "warn" : ""}`}>
          {i.warn && <EzIcon name="alert" className="ez-warn" />}โอกาสต้องซื้อหุ้น {Math.round(i.assign_pct)}%
        </span>
      </div>

      {pick.reasons_th.length > 0 && (
        <ul className="ez-why">
          {pick.reasons_th.map((r, k) => <li key={k}><EzIcon name="check" />{r}</li>)}
        </ul>
      )}
      {pick.cautions_th.length > 0 && (
        <ul className="ez-why caution">
          {pick.cautions_th.map((r, k) => <li key={k}><EzIcon name="alert" />{r}</li>)}
        </ul>
      )}
      <p className="ez-note ez-dim" style={{ fontSize: 13, marginTop: 10 }}>
        <EzIcon name="info" />
        <span>{pick.oi.note_th}</span>
      </p>

      <div className="ez-rule" />
      <div className="ez-pick-foot">
        <div className="ez-stance">
          <p>ถ้าราคาลงจนต้องซื้อ {i.symbol} 100 หุ้น คุณโอเคจะถือไหม</p>
          <div className="ez-seg2" role="group" aria-label={`ถ้าราคาลงจนต้องซื้อ ${i.symbol} 100 หุ้น คุณโอเคจะถือไหม`}>
            <button className="yes" aria-pressed={stance === "want"} onClick={() => onStance(i.symbol, "want")}>ถือได้</button>
            <button className="no" aria-pressed={stance === "avoid"} onClick={() => onStance(i.symbol, "avoid")}>ไม่อยากถือ</button>
          </div>
        </div>
        <button className="ez-btn primary" onClick={onOpen}>ดูไอเดียนี้<EzIcon name="chev" /></button>
      </div>
    </article>
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
            <span className="ez-lb top" style={{ left: `${sp}%` }}>ต้องซื้อหุ้นถ้าต่ำกว่า {h.strike}</span>
            <span className="ez-lb bot" style={{ left: `${np}%` }}>ตอนนี้ {h.spot.toFixed(2)}</span>
            <span className="ez-lb bot" style={{ left: `${(sp + np) / 2}%`, color: st.color }}>ห่าง {Math.abs(away).toFixed(1)}%</span>
          </div>
        )}
      </div>
    </>
  );
}
