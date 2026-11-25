/** Step 2 of the decision flow — the agent's one to three offers for the
 *  active stock, ported from the prototype. Level names, the ⚠ badge and the
 *  plain-Thai sentences all come from the EasyIdeas contract. */

import type { EasyIdea, EasyIdeas } from "../types/easy";
import { EzIcon } from "./icons";
import { LEVEL_TH, expiryTh, sourceLabel, usd } from "./util";

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
            <span className="ez-src">{sourceLabel(d.source)}</span>
            {sample && <span className="ez-src" style={{ color: "var(--amber)", borderColor: "#5a4724" }}>ข้อมูลตัวอย่าง</span>}
          </div>
          <p className="ez-say" dangerouslySetInnerHTML={{ __html: d.agent_say_th }} />
        </div>
      </div>

      {d.ideas.length === 0 ? (
        <div className="ez-panel ez-gap-l">
          <p className="ez-muted">{d.skip_reason_th}</p>
        </div>
      ) : (
        <div className="ez-list ez-gap-l">
          {d.ideas.map(o => (
            <button
              key={o.occ}
              className={`ez-opt ${o.recommended ? "recommended" : ""}`}
              disabled={o.cash_short}
              onClick={() => p.onPick(o)}
            >
              <span className="ez-lvl">
                {LEVEL_TH[o.level]}
                {o.recommended && <span className="ez-tag">แนะนำ</span>}
                {o.cash_short && <span className="ez-tag short">เงินไม่พอ</span>}
              </span>
              <span className="ez-amt ez-money ez-pos">{usd(o.credit)}</span>
              <span className="ez-desc">
                ยอมซื้อ {d.symbol} ที่ ${o.strike} (ถูกกว่าตอนนี้ {o.below_spot_pct.toFixed(1)}%) · รอ {o.days} วัน
              </span>
              <span className="ez-per">
                {o.period_pct.toFixed(2)}% ของเงินที่กันไว้{o.annual_pct != null && <> · ต่อปี {o.annual_pct.toFixed(1)}%</>}
              </span>
              <span className="ez-optrow">
                <span className={`ez-chance ${o.warn ? "warn" : ""}`}>
                  {o.warn && <EzIcon name="alert" className="ez-warn" />}โอกาสต้องซื้อหุ้น {Math.round(o.assign_pct)}%
                </span>
                <span className="ez-muted">กันเงินไว้ {usd(o.cost)}</span>
                <span className="ez-muted">หมดอายุ{expiryTh(o.expiry)}</span>
              </span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
