/** Step 3 — the five questions card, the what-if slider and the ask-agent
 *  chips, ported from the prototype. Money figures come from the EasyIdea
 *  contract; the "why now" sentence comes from `why_now.text_th`. Ask-agent
 *  answers run one agent turn (`runAgentTurn`) with the question plus a
 *  short context string and render as Markdown; only the browser preview's
 *  canned answers are trusted HTML, labelled as sample. */

import { useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { EasyIdea, EasyIdeas, EasyProfile } from "../types/easy";
import { anchorId, useAgentAnchor } from "../agent/anchors";
import { runAgentTurn } from "../agent/bus";
import { EzIcon } from "./icons";
import { WhatIf } from "./WhatIf";
import { LEVEL_TH, badgeText, expiryTh, signed, usd } from "./util";
import { easyHasTauri } from "./invoke";
import { sampleAnswer } from "./sample";

const ASK_QUESTIONS = [
  "ถ้าได้หุ้นแล้ว ทำอะไรต่อ?",
  "ทำไมไม่เลือกตัวที่ได้เงินมากกว่า?",
  "ถ้าหุ้นลงก่อนวันหมดอายุล่ะ?",
  "เปลี่ยนใจกลางทางได้ไหม?",
];

/** One answered chip in the thread. Only the browser preview's canned
 *  answers are trusted HTML — agent answers and error texts are LLM /
 *  backend output that must never reach innerHTML. */
type ThreadEntry =
  | { kind: "sample"; q: string; a: string }
  | { kind: "answer"; q: string; a: string; unverified: boolean }
  | { kind: "error"; q: string; a: string };

interface Props {
  idea: EasyIdea;
  ideas: EasyIdeas;
  profile: EasyProfile;
  browserPreview: boolean;
  onBack: () => void;
  onNext: () => void;
}

export function FiveQuestions(p: Props) {
  const { idea: o, ideas: d, profile } = p;
  const want = d.stance === "want";
  const sample = p.browserPreview || d.source === "DEMO";

  const [thread, setThread] = useState<ThreadEntry[]>([]);
  const [busy, setBusy] = useState(false);

  // one anchor per answer block, in the fixed order of the five questions
  const when = expiryTh(o.expiry).replace(/^\S+ /, "");
  const q1Ref = useAgentAnchor(anchorId("five", o.occ, "q1"), `ได้เงินเท่าไร ${o.symbol} ${o.strike} (${when})`);
  const q2Ref = useAgentAnchor(anchorId("five", o.occ, "q2"), `แลกกับอะไร ${o.symbol} ${o.strike} (${when})`);
  const q3Ref = useAgentAnchor(anchorId("five", o.occ, "q3"), `โอกาสแค่ไหน ${o.symbol} ${o.strike} (${when})`);
  const q4Ref = useAgentAnchor(anchorId("five", o.occ, "q4"), `ทำไมช่วงนี้ ${o.symbol} ${o.strike} (${when})`);
  const q5Ref = useAgentAnchor(anchorId("five", o.occ, "q5"), `แย่สุดคืออะไร ${o.symbol} ${o.strike} (${when})`);

  const ask = async (q: string) => {
    if (busy) return;
    if (!easyHasTauri) {
      setThread(t => [...t, { kind: "sample", q, a: sampleAnswer(q, o, d, profile) }]);
      return;
    }
    setBusy(true);
    const context = `(${d.symbol}, ยอมซื้อหุ้นที่ $${o.strike}, หมดอายุ ${o.expiry}, ได้เงิน ${usd(o.credit)}, กันเงิน ${usd(o.cost)})`;
    let answerText = "";
    let unverified = false;
    try {
      await runAgentTurn([{ role: "user", content: `${q} ${context}` }], (ev) => {
        if (ev.type === "answer") {
          answerText = ev.text;
          unverified = ev.unverified;
        }
      });
      if (!answerText) {
        setThread(t => [...t, { kind: "error", q, a: "หยุดก่อนได้คำตอบ — ลองถามใหม่อีกครั้ง" }]);
        return;
      }
      setThread(t => [...t, { kind: "answer", q, a: answerText, unverified }]);
    } catch (e) {
      setThread(t => [...t, { kind: "error", q, a: `ถามผู้ช่วยไม่สำเร็จ — ${e instanceof Error ? e.message : String(e)}` }]);
    } finally {
      setBusy(false);
    }
  };

  const dropPct = 0.30;
  const dropPx = d.spot * (1 - dropPct);

  return (
    <section className="ez-screen">
      <button className="ez-back" onClick={p.onBack}><EzIcon name="back" />ทางเลือกอื่น</button>
      <h1>{d.symbol} ทาง{LEVEL_TH[o.level]}: รับ {usd(o.credit)} ถ้ายอมซื้อหุ้นที่ ${o.strike}</h1>
      <p className="ez-lead">
        สัญญาหมดอายุ{expiryTh(o.expiry)} (อีก {o.days} วัน) · ราคา {d.symbol} ตอนนี้ ${d.spot.toFixed(2)}
        <span className={`ez-src${sample ? " amber" : ""}`}>{badgeText(d.source, sample)}</span>
      </p>

      <div className="ez-panel ez-gap-l" style={{ paddingTop: 8 }}>
        <div className="ez-q5" ref={q1Ref}>
          <h2>ได้เงินเท่าไร</h2>
          <div className="ez-ans">
            <span className="ez-money ez-pos">{usd(o.credit)} ทันที</span>
            เงินเข้าบัญชีวันนี้ และเป็นของคุณไม่ว่าจะเกิดอะไรขึ้น คิดเป็น {o.period_pct.toFixed(2)}% ของเงินที่กันไว้ ใน {o.days} วัน
          </div>
        </div>
        <div className="ez-q5" ref={q2Ref}>
          <h2>แลกกับอะไร</h2>
          <div
            className="ez-ans"
            dangerouslySetInnerHTML={{
              __html: `คุณสัญญาว่า ถ้า${expiryTh(o.expiry)} ${d.symbol} ปิด<strong>ต่ำกว่า $${o.strike}</strong> คุณจะซื้อ ${d.symbol} 100 หุ้นที่ $${o.strike} รวม <strong>${usd(o.cost)}</strong> Webull จะกันเงินก้อนนี้ไว้จนสัญญาจบ`,
            }}
          />
        </div>
        <div className="ez-q5" ref={q3Ref}>
          <h2>โอกาสแค่ไหน</h2>
          <div className="ez-ans">
            <div className="ez-cases">
              <div>
                <span className="ez-pct ez-pos">{Math.round(o.cases.keep_pct)}%</span>
                <span>{d.symbol} ปิดสูงกว่า ${o.strike} → ไม่ต้องซื้อหุ้น เก็บ {usd(o.credit)} ครบ เงินที่กันไว้คืนมา</span>
              </div>
              <div>
                <span className="ez-pct ez-warn">{Math.round(o.cases.between_pct)}%</span>
                <span>ปิดระหว่าง ${o.breakeven.toFixed(2)} ถึง ${o.strike} → ได้หุ้น 100 หุ้น แต่ยังไม่ขาดทุน เพราะเงินที่ได้มาช่วยลดต้นทุนเหลือ ${o.breakeven.toFixed(2)} ต่อหุ้น</span>
              </div>
              <div>
                <span className="ez-pct ez-neg">{Math.round(o.cases.loss_pct)}%</span>
                <span>ปิดต่ำกว่า ${o.breakeven.toFixed(2)} → ได้หุ้นในราคาที่สูงกว่าตลาด ขาดทุนบนกระดาษ จนกว่าราคาจะกลับขึ้นมา</span>
              </div>
              <div className="ez-dim" style={{ gridTemplateColumns: "1fr", fontSize: 14 }}>
                ตัวเลขโอกาสมาจากสูตรคำนวณ ใช้ความผันผวนของราคาเป็นฐาน ไม่ใช่คำทำนาย
              </div>
            </div>
          </div>
        </div>
        <div className="ez-q5" ref={q4Ref}>
          <h2>ทำไมช่วงนี้น่าทำ</h2>
          <div className="ez-ans" dangerouslySetInnerHTML={{ __html: d.why_now.text_th }} />
        </div>
        <div className="ez-q5" ref={q5Ref}>
          <h2>แย่สุดคืออะไร</h2>
          <div
            className="ez-ans"
            dangerouslySetInnerHTML={{
              __html: `<span class="ez-money ez-neg">${signed(o.worst_30_usd)}</span>ถ้า ${d.symbol} ร่วง 30% เหลือ $${dropPx.toFixed(2)} คุณต้องซื้อ 100 หุ้นที่ $${o.strike} หุ้นที่ได้มามีมูลค่า ${usd(dropPx * 100)} ขาดทุนบนกระดาษประมาณ ${usd(-o.worst_30_usd)}${want ? ` แต่ได้ ${d.symbol} ที่คุณอยากถืออยู่แล้วในราคาถูกกว่าวันนี้` : ""}<br><span class="ez-dim" style="font-size:15px">ในทางทฤษฎี ถ้าหุ้นเหลือ $0 จะเสียได้มากสุด ${usd(o.max_loss_usd)}</span>`,
            }}
          />
        </div>
      </div>

      <WhatIf key={o.occ} idea={o} spot={d.spot} symbol={d.symbol} />

      <div className="ez-ask">
        <h2><EzIcon name="chat" />ถามผู้ช่วยต่อ</h2>
        <div className="ez-quick">
          {ASK_QUESTIONS.map(q => (
            <button key={q} className="ez-pill" disabled={busy} onClick={() => ask(q)}>{q}</button>
          ))}
        </div>
        <div className="ez-thread">
          {thread.map((m, i) => (
            <div key={i}>
              <div className="ez-q">{m.q}</div>
              <div className="ez-a">
                {m.kind === "sample" && <span dangerouslySetInnerHTML={{ __html: m.a }} />}
                {m.kind === "answer" && (
                  <span>
                    <Markdown remarkPlugins={[remarkGfm]}>{m.a}</Markdown>
                    {m.unverified && <span className="ez-dim">⚠ ตัวเลขบางตัวยังหาที่มาไม่ได้ — ตรวจกับหน้าจอก่อนตัดสินใจ</span>}
                  </span>
                )}
                {m.kind === "error" && <span>{m.a}</span>}
                {m.kind === "sample" && <div className="ez-dim" style={{ fontSize: 13, marginTop: 6 }}>คำตอบตัวอย่างใน prototype</div>}
              </div>
            </div>
          ))}
          {busy && <div className="ez-thread-busy">ผู้ช่วยกำลังคิด…</div>}
        </div>
      </div>

      <div className="ez-actions">
        <button className="ez-btn primary" onClick={p.onNext}>เข้าใจแล้ว ไปขั้นก่อนตัดสินใจ</button>
        <button className="ez-btn ghost" onClick={p.onBack}>ดูทางเลือกอื่น</button>
      </div>
      <p className="ez-note ez-dim" style={{ marginTop: 18 }}>
        <EzIcon name="info" />
        <span>คุณตั้งเงินไว้ {usd(profile.cash)} · สัญญานี้ต้องกันเงิน {usd(o.cost)}</span>
      </p>
    </section>
  );
}
