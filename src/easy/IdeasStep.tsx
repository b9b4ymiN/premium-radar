/** Step 2 of the decision flow — the agent's one to three offers for the
 *  active stock, ported from the prototype. Level names, the ⚠ badge and the
 *  plain-Thai sentences all come from the EasyIdeas contract. */

import { useState } from "react";
import type { EasyIdea, EasyIdeas } from "../types/easy";
import { anchorId, useAgentAnchor } from "../agent/anchors";
import { EzIcon } from "./icons";
import { Lesson } from "./Lesson";
import { LEVEL_TH, badgeText, expiryTh, usd } from "./util";

interface Props {
  ideas: EasyIdeas | null;
  ideasBusy: boolean;
  ideasError: string | null;
  browserPreview: boolean;
  onRetry: () => void;
  onBack: () => void;
  onPick: (idea: EasyIdea) => void;
}

export function IdeasStep(p: Props) {
  // the lesson overlay opened from this screen (null = closed) — state lives
  // before the loading/error early returns so the hook order is stable
  const [lessonIdea, setLessonIdea] = useState<EasyIdea | null>(null);

  if (p.ideasError) {
    return (
      <section className="ez-screen">
        <button className="ez-back" onClick={p.onBack}><EzIcon name="back" />หน้าแรก</button>
        <div className="ez-panel ez-loading" role="status">
          <span className="ez-error">ดึงไอเดียไม่สำเร็จ — {p.ideasError}</span>
          <button className="ez-btn" onClick={p.onRetry}>ลองใหม่</button>
        </div>
      </section>
    );
  }
  if (p.ideas == null || (p.ideasBusy && p.ideas.ideas.length === 0)) {
    return (
      <section className="ez-screen">
        <button className="ez-back" onClick={p.onBack}><EzIcon name="back" />หน้าแรก</button>
        <div className="ez-panel ez-loading" role="status">
          <span className="ez-spin" />
          <span>ผู้ช่วยกำลังคำนวณทางเลือกของ {p.ideas?.symbol ?? ""} รอสักครู่</span>
        </div>
      </section>
    );
  }

  const d = p.ideas;
  const sample = p.browserPreview || d.source === "DEMO";
  return (
    <section className="ez-screen">
      <button className="ez-back" onClick={p.onBack}><EzIcon name="back" />หน้าแรก</button>
      <div className="ez-agent">
        <span className="ez-avatar"><EzIcon name="radar" /></span>
        <div>
          <div className="ez-agent-name">
            ผู้ช่วย · {d.symbol} ราคาตอนนี้ ${d.spot.toFixed(2)}
            <span className={`ez-src${sample ? " amber" : ""}`}>{badgeText(d.source, sample)}</span>
          </div>
          <p className="ez-say" dangerouslySetInnerHTML={{ __html: d.agent_say_th }} />
        </div>
      </div>

      {d.ideas.length === 0 ? (
        <div className="ez-panel ez-gap-l">
          <p className="ez-muted">{d.skip_reason_th}</p>
        </div>
      ) : (
        <>
          <div className="ez-lesson-entry">
            <button
              className="ez-btn ghost"
              onClick={() => setLessonIdea(d.ideas.find(i => i.recommended) ?? d.ideas[0])}
            >
              <EzIcon name="info" />เรียนพื้นฐานก่อนตัดสินใจ (5 นาที)
            </button>
          </div>
          <div className="ez-list ez-gap-l">
            {d.ideas.map(o => (
              <IdeaOption key={o.occ} o={o} symbol={d.symbol} onPick={p.onPick} />
            ))}
          </div>
        </>
      )}

      {lessonIdea && <Lesson idea={lessonIdea} ideas={d} onClose={() => setLessonIdea(null)} />}
    </section>
  );
}

/** One offer card — its own component so each card can register its own agent
 *  anchors (hooks cannot be called inside the `.map` loop of the parent). */
function IdeaOption({ o, symbol, onPick }: {
  o: EasyIdea;
  symbol: string;
  onPick: (idea: EasyIdea) => void;
}) {
  const when = expiryTh(o.expiry).replace(/^\S+ /, "");
  const cardRef = useAgentAnchor(anchorId("idea", o.occ), `ทางเลือก ${symbol} ยอมซื้อที่ $${o.strike} (${when})`);
  const premiumRef = useAgentAnchor(anchorId("idea", o.occ, "premium"), `เงินที่ได้ทันที ${symbol} ${o.strike} (${when})`);
  const cashRef = useAgentAnchor(anchorId("idea", o.occ, "cash_needed"), `เงินที่ต้องกันไว้ ${symbol} ${o.strike} (${when})`);
  const assignRef = useAgentAnchor(anchorId("idea", o.occ, "assign"), `โอกาสต้องซื้อหุ้น ${symbol} ${o.strike} (${when})`);
  const expiryRef = useAgentAnchor(anchorId("idea", o.occ, "expiry"), `วันหมดอายุของทางเลือก ${symbol} ${o.strike} (${when})`);
  return (
    <button
      ref={cardRef}
      className={`ez-opt ${o.recommended ? "recommended" : ""}`}
      disabled={o.cash_short}
      onClick={() => onPick(o)}
    >
      <span className="ez-lvl">
        {LEVEL_TH[o.level]}
        {o.recommended && <span className="ez-tag">แนะนำ</span>}
        {o.cash_short && <span className="ez-tag short">เงินไม่พอ</span>}
      </span>
      <span ref={premiumRef} className="ez-amt ez-money ez-pos">{usd(o.credit)}</span>
      <span className="ez-desc">
        ยอมซื้อ {symbol} ที่ ${o.strike} (ถูกกว่าตอนนี้ {o.below_spot_pct.toFixed(1)}%) · รอ {o.days} วัน
      </span>
      <span className="ez-per">
        {o.period_pct.toFixed(2)}% ของเงินที่กันไว้{o.annual_pct != null && <> · ต่อปี {o.annual_pct.toFixed(1)}%</>}
      </span>
      <span className="ez-optrow">
        <span ref={assignRef} className={`ez-chance ${o.warn ? "warn" : ""}`}>
          {o.warn && <EzIcon name="alert" className="ez-warn" />}โอกาสต้องซื้อหุ้น {Math.round(o.assign_pct)}%
        </span>
        <span ref={cashRef} className="ez-muted">กันเงินไว้ {usd(o.cost)}</span>
        <span ref={expiryRef} className="ez-muted">หมดอายุ{expiryTh(o.expiry)}</span>
      </span>
    </button>
  );
}
