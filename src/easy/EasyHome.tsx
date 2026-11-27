/** Easy Mode home — the market clock, the assistant's ranked ideas picked from
 *  the whole market, the contracts already sold, and the link into the flow.
 *  Every dynamic sentence comes from the UniverseIdeas contract; the only
 *  numbers rendered here are the ones it carries. */

import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type {
  EasyProfile,
  NearMiss,
  RiskLevel,
  Stance,
  UniverseIdeas,
  UniversePick,
  WatchCash,
} from "../types/easy";
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
  /** Saves the profile with the new risk and refetches the universe (EasyApp). */
  onRisk: (risk: RiskLevel) => void;
  onGo: (screen: "ob1") => void;
}

/** The three risk modes — labels/emoji per the product copy; the numbers in
 *  MODE_DESC are the mode definitions themselves (the same 15/25/30 caps and
 *  14–45/10–45 day windows Rust enforces), not per-night data. */
const MODES: { risk: RiskLevel; dot: string }[] = [
  { risk: 1, dot: "🟢" },
  { risk: 2, dot: "🟡" },
  { risk: 3, dot: "🟠" },
];

const MODE_LABEL: Record<RiskLevel, string> = { 1: "ปลอดภัย", 2: "ปานกลาง", 3: "เสี่ยงมากขึ้น" };

/** assign-chance ceiling each mode accepts (Rust comfort_cap 15/25/30) —
 *  used only for the teaching sentence that names a friendlier mode. */
const MODE_CAP: Record<RiskLevel, number> = { 1: 15, 2: 25, 3: 30 };

const MODE_DESC: Record<RiskLevel, string> = {
  1: "เลือกเฉพาะไอเดียที่โอกาสต้องซื้อหุ้นไม่เกิน 15% หรือประมาณ 1 ใน 7 ครั้ง · สัญญายาว 14–45 วัน · ได้เงินน้อยกว่า แต่ต้องซื้อหุ้นน้อยครั้งกว่า",
  2: "ยอมให้โอกาสต้องซื้อหุ้นได้ถึง 25% หรือประมาณ 1 ใน 4 ครั้ง · สัญญายาว 14–45 วัน · ได้เงินมากกว่าโหมดปลอดภัย แลกกับโอกาสต้องซื้อหุ้นที่บ่อยขึ้น",
  3: "ยอมให้โอกาสต้องซื้อหุ้นได้ถึง 30% หรือประมาณ 1 ใน 3 ครั้ง · สัญญาสั้นลงเหลือ 10–45 วัน · ได้เงินมากที่สุด แต่ต้องซื้อหุ้นบ่อยที่สุดด้วย",
};

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

      <div className="ez-modebox">
        <ModeSwitch risk={p.profile.risk} universe={u} onRisk={p.onRisk} />
        <p className="ez-mode-desc">{MODE_DESC[p.profile.risk]}</p>
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
          {u.near_misses.length > 0 && (
            <>
              <div className="ez-nm-title">ตัวที่เกือบผ่าน — ทำไมยังไม่แนะนำ</div>
              <p className="ez-dim" style={{ fontSize: 14, marginTop: 6 }}>
                สัญญาเหล่านี้คุณจ่ายไหว แต่ยังไม่ผ่านข้อตรวจของผู้ช่วยอย่างน้อย 1 ข้อ — เปิดดูได้ว่าข้อไหนไม่ผ่านและเพราะอะไร
              </p>
              {u.near_misses.map(nm => (
                <NearMissCard key={nm.ref_occ} nm={nm} cash={p.profile.cash} risk={p.profile.risk} />
              ))}
            </>
          )}
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

/** The 3-mode risk switch. Native radios would buy arrow keys for free but
 *  fight the card styling, so this is a real radiogroup made of buttons:
 *  role=radio + aria-checked, roving tabindex, and arrow keys both focus and
 *  select (selecting saves the profile and refetches — EasyApp's onRisk). */
function ModeSwitch({ risk, universe, onRisk }: {
  risk: RiskLevel;
  universe: UniverseIdeas | null;
  onRisk: (risk: RiskLevel) => void;
}) {
  const btns = useRef<(HTMLButtonElement | null)[]>([]);
  const countOf = (r: RiskLevel) => universe?.mode_counts.find(m => m.risk === r)?.picks;
  const move = (ix: number, dir: 1 | -1) => {
    const next = (ix + dir + MODES.length) % MODES.length;
    btns.current[next]?.focus();
    onRisk(MODES[next].risk);
  };
  return (
    <div className="ez-modebar" role="radiogroup" aria-label="ระดับความเสี่ยงที่รับได้">
      {MODES.map((m, ix) => (
        <ModeButton
          key={m.risk}
          mode={m}
          active={risk === m.risk}
          count={countOf(m.risk)}
          onSelect={onRisk}
          onKeyDown={e => {
            if (e.key === "ArrowRight" || e.key === "ArrowDown") {
              e.preventDefault();
              move(ix, 1);
            } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
              e.preventDefault();
              move(ix, -1);
            }
          }}
          registerRef={el => { btns.current[ix] = el; }}
        />
      ))}
    </div>
  );
}

function modeCountText(count: number | undefined): string {
  if (count == null) return "กำลังดู…";
  return count > 0 ? `${count} ไอเดีย` : "ยังไม่มี";
}

function ModeButton({ mode, active, count, onSelect, onKeyDown, registerRef }: {
  mode: { risk: RiskLevel; dot: string };
  active: boolean;
  count: number | undefined;
  onSelect: (risk: RiskLevel) => void;
  onKeyDown: (e: KeyboardEvent<HTMLButtonElement>) => void;
  registerRef: (el: HTMLButtonElement | null) => void;
}) {
  const label = MODE_LABEL[mode.risk];
  const anchorRef = useAgentAnchor(
    anchorId("mode", String(mode.risk)),
    `โหมด${label} — ${modeCountText(count)}`,
  );
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      tabIndex={active ? 0 : -1}
      className="ez-mode"
      onClick={() => onSelect(mode.risk)}
      onKeyDown={onKeyDown}
      ref={el => { registerRef(el); anchorRef(el); }}
    >
      <span className="ez-mode-name">{mode.dot} {label}</span>
      <span className="ez-mode-count">{modeCountText(count)}</span>
    </button>
  );
}

/** One ✅/❌ line on a near-miss card; a failed check gets an "อยากรู้เพิ่ม"
 *  <details> under it. The line text is formatting only — every number comes
 *  from the NearMiss contract. */
function NmLine({ ok, text, why }: { ok: boolean; text: string; why?: ReactNode }) {
  return (
    <div className="ez-nm-line">
      <span role="img" aria-label={ok ? "ผ่าน" : "ไม่ผ่าน"}>{ok ? "✅" : "❌"}</span>
      <div>
        <span className={ok ? "" : "ez-neg"}>{text}</span>
        {!ok && why && (
          <details className="ez-c100 ez-nm-why">
            <summary>อยากรู้เพิ่ม</summary>
            <div className="ez-c100-body">{why}</div>
          </details>
        )}
      </div>
    </div>
  );
}

/** A put the user can afford that missed ≥1 gate tonight: the contract in
 *  words first, then every check with its real numbers and, for the failed
 *  ones, a plain-Thai explanation the reader can open. */
function NearMissCard({ nm, cash, risk }: { nm: NearMiss; cash: number; risk: RiskLevel }) {
  const when = expiryTh(nm.expiry).replace(/^\S+ /, "");
  const days = Math.round(nm.days);
  const cap = nm.checks.cap_pct;
  const assign = Math.round(nm.assign_pct * 10) / 10;
  const oneIn = Math.max(2, Math.round(100 / Math.max(assign, 1)));
  const window = risk === 3 ? "10–45" : "14–45";
  // the smallest friendlier mode whose cap would accept this assign chance
  const nextMode = ([2, 3] as RiskLevel[]).find(r => r > risk && nm.assign_pct <= MODE_CAP[r]);
  const cardRef = useAgentAnchor(
    anchorId("nearmiss", nm.ref_occ),
    `เกือบผ่าน ${nm.symbol} ยอมซื้อ 100 หุ้นที่ $${nm.strike}`,
  );
  return (
    <article ref={cardRef} className="ez-nm">
      <div className="ez-nm-head">
        ยอมซื้อ {nm.symbol} 100 หุ้นที่ ${nm.strike} · หมดอายุ {when} (อีก {days} วัน)
      </div>
      <div className="ez-nm-checks">
        <NmLine
          ok
          text={`ต้องกันเงิน ${usd(nm.cash_needed_usd)} — อยู่ในงบ ${usd(cash)} ของคุณ จึงขายสัญญานี้ได้`}
        />
        <NmLine
          ok={nm.checks.assign_ok}
          text={`โอกาสต้องซื้อหุ้น ${assign}% — ${nm.checks.assign_ok ? "ไม่เกิน" : "เกิน"} ${cap}% ที่โหมดนี้รับได้`}
          why={
            <p>
              โอกาส {assign}% หมายถึง ถ้าทำแบบนี้หลาย ๆ ครั้ง ราว 1 ใน {oneIn} ครั้ง ราคาหุ้นจะลงมาถึงจุดที่คุณต้องซื้อ {nm.symbol} 100 หุ้นที่ ${nm.strike} จริง ๆ
              {nextMode
                ? ` ถ้าสลับไปโหมด${MODE_LABEL[nextMode]} ซึ่งยอมรับโอกาสสูงถึง ${MODE_CAP[nextMode]}% ตัวนี้จะผ่านข้อนี้ทันที — แลกกับการที่คุณต้องยอมรับความเสี่ยงแบบนี้เป็นเรื่องปกติ`
                : " ตอนนี้ไม่มีโหมดไหนยอมรับโอกาสเกิน 30% ครับ"}
            </p>
          }
        />
        <NmLine
          ok={nm.checks.credit_ok}
          text={nm.checks.credit_ok
            ? `ได้เงินทันที ${usd(nm.credit_usd)} ต่อ 1 สัญญา`
            : `ได้เงินทันทีแค่ ${usd(nm.credit_usd)} — น้อยกว่าขั้นต่ำ $5 ต่อสัญญาที่ผู้ช่วยยอมแนะนำ`}
          why={
            // $5 mirrors Rust's MIN_CREDIT × 100, the floor every mode shares
            <p>
              เงิน {usd(nm.credit_usd)} ที่จะได้มา น้อยเกินไปเมื่อเทียบกับเงิน {usd(nm.cash_needed_usd)} ที่ต้องกันไว้ทั้งก้อนตลอดราว {days} วัน
              {" "}ผู้ช่วยจึงตั้งขั้นต่ำไว้ที่ $5 ต่อสัญญา ถ้าได้น้อยกว่านี้ ความเสี่ยงที่แบกรับไม่คุ้มกับเงินที่ได้
            </p>
          }
        />
        <NmLine
          ok={nm.checks.days_ok}
          text={`เหลืออายุอีก ${days} วัน — โหมดนี้รับสัญญาที่เหลือ ${window} วัน`}
          why={
            <p>
              โหมดที่คุณเลือกรับเฉพาะสัญญาที่เหลืออายุ {window} วัน ส่วนตัวนี้เหลือ {days} วัน
              {" "}สัญญาที่ใกล้หมดอายุ ราคาเด้งขึ้นลงแรงในเวลาสั้น ๆ ส่วนสัญญาที่ยาวเกิน 45 วันจะล็อคเงินก้อนนี้ไว้นานเกินไป
            </p>
          }
        />
        <NmLine
          ok={nm.checks.has_buyer}
          text={nm.checks.has_buyer
            ? "มีคนยื่นราคารับซื้อสัญญานี้อยู่"
            : "ยังไม่มีคนยื่นราคารับซื้อสัญญานี้ — ขายออกได้ยาก"}
          why={
            <p>
              ยังไม่มีราคารับซื้อเลย แปลว่าถ้าคุณขายสัญญานี้ตอนนี้ อาจไม่มีใครรับซื้อ
              {" "}ต้องรอหรือลดราคาลงเรื่อย ๆ จนมีคนสนใจ ผู้ช่วยจึงรอให้มีคนยื่นราคาจริงก่อนจึงจะแนะนำ
            </p>
          }
        />
        <NmLine
          ok={nm.checks.liquidity_ok}
          text={nm.checks.liquidity_ok
            ? "ซื้อขายสัญญานี้คล่องพอ"
            : "มีคนซื้อขายสัญญานี้น้อย — ถ้าวันหลังอยากยกเลิกก่อนหมดอายุ คุณอาจต้องจ่ายคืนแพงกว่าที่ได้มา"}
          why={
            <p>
              ราคาที่มีคนรับซื้อสัญญานี้ {usd(nm.bid_usd, 2)} กับราคาที่มีคนขายให้ {usd(nm.ask_usd, 2)} ห่างกัน {usd(nm.gap_usd, 2)} ต่อหุ้น
              {" "}หรือ {usd(nm.gap_usd * 100)} ต่อสัญญา
              {nm.spread_pct != null && <> (ราว {Math.round(nm.spread_pct)}% ของราคากลาง)</>}
              {" "}ช่องว่างแบบนี้แปลว่าวันหนึ่งถ้าคุณอยากซื้อคืนเพื่อยกเลิกก่อนหมดอายุ คุณต้องจ่ายราคาฝั่งขายที่แพงกว่า บางครั้งแพงกว่าเงินที่ได้มาทั้งหมด
            </p>
          }
        />
      </div>
    </article>
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
