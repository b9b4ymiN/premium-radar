/** "เรียนพื้นฐานก่อนตัดสินใจ" — the 8-step lesson ported from the approved
 *  prototype (docs/dev/lesson-prototype.html), filled with ONE idea's real
 *  numbers. pele renders the Mermaid text templates; the price ladder and the
 *  history chart are hand-drawn SVG. The only arithmetic here is WhatIf's
 *  expiry formula plus counting — every other number comes from the contract.
 *  Rendered as a fixed full-screen panel by the entry screens, so closing it
 *  simply unmounts this component and the caller's screen is still there. */

import { useEffect, useMemo, useRef, useState } from "react";
import { render } from "pele";
import type { EasyIdea, EasyIdeas } from "../types/easy";
import { anchorId, useAgentAnchor } from "../agent/anchors";
import { EzIcon } from "./icons";
import { expiryOutcome } from "./WhatIf";
import { asOfTh, badgeText, expiryTh, usd } from "./util";
import { easyHasTauri, easyInvoke } from "./invoke";

/** Diagram text is Thai-first, so measure with the Thai-capable stack from
 *  tokens.css (Inter first would mis-measure the long Thai labels). */
const PELE_FONT = "'IBM Plex Sans Thai', 'Noto Sans Thai', 'Segoe UI', sans-serif";

/** localStorage flag: the first idea the user ever opens shows the lesson by
 *  itself once. Any storage failure counts as "seen" so the auto-open never
 *  fires twice — the manual button still works. */
const SEEN_FLAG = "ez-lesson-seen";
export function lessonSeen(): boolean {
  try {
    return window.localStorage.getItem(SEEN_FLAG) === "1";
  } catch {
    return true;
  }
}
export function markLessonSeen(): void {
  try {
    window.localStorage.setItem(SEEN_FLAG, "1");
  } catch {
    // private mode etc. — the flag is best-effort only
  }
}

/** Trimmed get_stock_history shape (see PriceChart/chartData.ts). */
interface HistoryBars {
  bars: { time: string; close: number | null }[];
}
/** ~6 months of trading days for the history chart. */
const HISTORY_BARS = 126;

export function Lesson({ idea: o, ideas: d, onClose }: {
  idea: EasyIdea;
  ideas: EasyIdeas;
  onClose: () => void;
}) {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    closeRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const sym = o.symbol;
  const strike = `$${o.strike}`;
  const whenFull = expiryTh(o.expiry); // "ศุกร์ 18 ธ.ค."
  const when = whenFull.replace(/^\S+ /, ""); // "18 ธ.ค."
  const today = expiryTh(d.computed_at.slice(0, 10)).replace(/^\S+ /, "");
  const sample = d.source === "DEMO";

  // the two worked endings of step 4: a price above the strike (today's real
  // spot when it already qualifies, otherwise strike + 5%) and a round price
  // about 8% below the strike — both fed through WhatIf's formula
  const above = d.spot >= o.strike ? d.spot : o.strike * 1.05;
  const below = Math.round(o.strike * 0.92);
  const aboveMoney = expiryOutcome(above, o);
  const belowMoney = expiryOutcome(below, o);
  const belowWorth = below * 100;

  const keepPct = 100 - o.assign_pct;
  const perN = Math.max(2, Math.round(100 / Math.max(1, o.assign_pct)));

  const ladderRef = useAgentAnchor(anchorId("lesson", "ladder"), `บันไดราคา ${sym}`);
  const quizRef = useAgentAnchor(anchorId("lesson", "quiz"), "แบบทดสอบท้ายบทเรียน");
  const s1 = useAgentAnchor(anchorId("lesson", "step1"), "บทเรียนขั้น 1 ใครให้อะไรใคร");
  const s2 = useAgentAnchor(anchorId("lesson", "step2"), "บทเรียนขั้น 2 ทำไมต้องกันเงิน");
  const s3 = useAgentAnchor(anchorId("lesson", "step3"), "บทเรียนขั้น 3 ระหว่างทางเกิดอะไร");
  const s4 = useAgentAnchor(anchorId("lesson", "step4"), "บทเรียนขั้น 4 จบได้แค่ 2 ทาง");
  const s5 = useAgentAnchor(anchorId("lesson", "step5"), "บทเรียนขั้น 5 โอกาสแต่ละทาง");
  const s8 = useAgentAnchor(anchorId("lesson", "step8"), "บทเรียนขั้น 8 ลองตอบดู");

  return (
    <div className="ez-lesson" role="dialog" aria-modal="true" aria-label={`บทเรียน ${sym} ทีละขั้น`}>
      <div className="ez-lesson-bar">
        <button ref={closeRef} className="ez-btn ghost" onClick={onClose}>
          <EzIcon name="back" />ปิดบทเรียน
        </button>
      </div>
      <main className="ez-lesson-col">
        <header className="ez-lesson-step">
          <span className="ez-tag">บทเรียน 5 นาที · ข้อมูลสัญญาจริง {today}</span>
          <h1>ขายสัญญา {sym} ทีละขั้น</h1>
          <p className="ez-lesson-lead">
            หน้านี้เล่าไอเดียเดียวให้จบ: ถ้าคุณกดขายสัญญานี้ จะเกิดอะไรกับเงินของคุณ
            ตั้งแต่วันนี้จนวันที่สัญญาหมดอายุ อ่านทีละขั้นจากบนลงล่าง ไม่ต้องรู้ศัพท์อะไรมาก่อน
            {sample && <span className="ez-src amber">{badgeText(d.source, true)}</span>}
          </p>
          <div className="ez-lesson-facts">
            <div className="ez-lesson-fact"><b>{`$${d.spot.toFixed(2)}`}</b><span>ราคา {sym} ตอนนี้ (ต่อ 1 หุ้น)</span></div>
            <div className="ez-lesson-fact"><b>{strike}</b><span>ราคาที่คุณ "นัดซื้อ"</span></div>
            <div className="ez-lesson-fact"><b className="ez-pos">+{usd(o.credit)}</b><span>เงินที่ได้ทันที</span></div>
            <div className="ez-lesson-fact"><b>{when}</b><span>วันที่สัญญาหมดอายุ (อีก {o.days} วัน)</span></div>
          </div>
        </header>

        <section className="ez-lesson-step">
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 1</span>
            <h2 ref={s1}>ใครให้อะไรใคร</h2>
          </div>
          <p>
            เรื่องนี้มี 3 ฝ่าย คือ <b>คุณ</b>, <b>คนซื้อสัญญา</b> (คนที่กลัวว่า {sym} จะลง)
            และ <b>Webull</b> (คนถือเงินกลาง) คุณทำหน้าที่คล้ายบริษัทประกัน
            คือรับเงินค่าเบี้ยไว้ก่อน แลกกับคำสัญญาว่าถ้าเกิดเรื่อง คุณจะช่วยรับซื้อหุ้น
          </p>
          <Diagram
            label={`ใครให้อะไรใคร สำหรับสัญญา ${sym}`}
            source={`sequenceDiagram
  participant B as คนซื้อสัญญา
  participant Y as คุณ
  participant W as Webull
  B->>Y: จ่ายเงินให้ ${usd(o.credit)} (เป็นของคุณทันที)
  Y->>B: สัญญาว่า "ถ้า ${sym} ต่ำกว่า ${strike} ฉันจะซื้อ 100 หุ้นที่ ${strike}"
  W->>W: กันเงินในบัญชีคุณไว้ ${usd(o.cost)} จนสัญญาจบ`}
          />
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> คุณได้เงิน {usd(o.credit)} ไปก่อน แลกกับการยอมรับซื้อหุ้นถ้าราคาลง</p>
        </section>

        <section className="ez-lesson-step">
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 2</span>
            <h2 ref={s2}>ทำไมต้องกันเงิน {usd(o.cost)}</h2>
          </div>
          <p>
            ตลาดหุ้นอเมริกากำหนดว่า <b>สัญญา 1 ตัว = หุ้น 100 หุ้นเสมอ</b> จะทำทีละ 10 หุ้นไม่ได้
            ถ้าวันหนึ่งคุณต้องซื้อจริง ต้องจ่าย 100 หุ้น × {strike} = <b>{usd(o.cost)}</b>
            {" "}Webull จึงล็อกเงินก้อนนี้ไว้ล่วงหน้า เพื่อให้แน่ใจว่าคุณมีเงินจ่าย
          </p>
          <div className="ez-panel quiet">
            <p><b>ระหว่างที่สัญญายังไม่จบ</b></p>
            <p>เงิน <b>{usd(o.cost)}</b> ยังเป็นของคุณ แต่ <b>ถอนหรือเอาไปซื้ออย่างอื่นไม่ได้</b> จนถึงวันที่ {when}</p>
            <p>เงิน <b className="ez-pos">{usd(o.credit)}</b> ที่ได้มา ใช้ได้เลย</p>
          </div>
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> เงินที่ต้องมี = ราคาที่นัดซื้อ × 100 เช่นงบ $3,000 ทำได้กับหุ้นราคาไม่เกินราว $30</p>
        </section>

        <section className="ez-lesson-step">
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 3</span>
            <h2 ref={s3}>ระหว่าง {o.days} วันนี้ เกิดอะไรบ้าง</h2>
          </div>
          <Diagram
            label={`เส้นเวลาจากวันนี้ถึงวันหมดอายุของสัญญา ${sym}`}
            source={`timeline
  title จากวันนี้ถึงวันหมดอายุ
  ${today} : ขายสัญญา : ได้ ${usd(o.credit)} ทันที : Webull กันเงิน ${usd(o.cost)}
  ระหว่างทาง : ราคา ${sym} ขึ้นลงทุกวัน : เป็นเรื่องปกติ ยังไม่มีอะไรต้องทำ
  ${when} : วันตัดสิน : ดูว่า ${sym} ปิดเหนือหรือต่ำกว่า ${strike}`}
          />
          <p>
            ระหว่างทาง ราคา {sym} อาจลงไปแตะ {strike} แล้วเด้งกลับก็ได้
            {" "}<b>แต่สิ่งที่ตัดสินคือราคาตอนปิดวันที่ {when}</b>
            {" "}เห็นตัวเลขแดงระหว่างทางจึงยังไม่ได้แปลว่าเสียเงินจริง
          </p>
          <p className="ez-lesson-note">
            โดยทั่วไป ราคามักลงไปแตะราคาที่นัดระหว่างทางบ่อยกว่าที่จะจบต่ำกว่าจริง ประมาณ 2 เท่า
            เตรียมใจไว้ก่อนจะได้ไม่ตกใจ
          </p>
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> ตัดสินกันที่ราคาปิดวันสุดท้ายเท่านั้น เห็นแดงระหว่างทางยังไม่แปลว่าแพ้</p>
        </section>

        <section className="ez-lesson-step">
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 4</span>
            <h2 ref={s4}>จบได้แค่ 2 ทาง</h2>
          </div>
          <Diagram
            label={`สองทางจบของสัญญา ${sym}`}
            source={`flowchart TD
  A["${today} ขายสัญญา<br/>ได้ ${usd(o.credit)} ทันที"] --> B{"${when}<br/>${sym} ปิดที่เท่าไร?"}
  B -->|"เหนือ ${strike}"| C["สัญญาจบเอง<br/>ได้ ${usd(o.credit)} เต็ม<br/>เงิน ${usd(o.cost)} กลับมาใช้ได้"]
  B -->|"ต่ำกว่า ${strike}"| D["ต้องซื้อ ${sym} 100 หุ้นที่ ${strike}<br/>จ่าย ${usd(o.cost)}<br/>ได้หุ้นมาถือ"]`}
          />
          <div className="ez-lesson-endings">
            <div className="ez-lesson-end good">
              <h3>{`ทางที่ 1 · ปิดที่ $${above.toFixed(2)} (สูงกว่า ${strike})`}</h3>
              <p>คนซื้อสัญญาไม่ได้ใช้สิทธิ สัญญาจบไปเฉยๆ</p>
              <p>คุณได้ <b className="ez-pos">+{usd(aboveMoney)}</b> และเงิน {usd(o.cost)} ปลดล็อก</p>
            </div>
            <div className="ez-lesson-end warn">
              <h3>{`ทางที่ 2 · ปิดที่ $${below} (ต่ำกว่า ${strike})`}</h3>
              <p>คุณต้องจ่าย <b>{usd(o.cost)}</b> ซื้อ 100 หุ้น แต่ตอนนั้นหุ้นมีมูลค่า 100 × ${`$${below}`} = <b>{usd(belowWorth)}</b></p>
              <p>ขาดทุนบนกระดาษ <span className="ez-neg">−{usd(o.cost - belowWorth)}</span> แต่ได้ {usd(o.credit)} มาแล้ว สุทธิ <b className="ez-neg">{belowMoney >= 0 ? "+" : "−"}{usd(belowMoney)}</b></p>
              <p className="ez-lesson-note">ยังถือหุ้นอยู่ ถ้าราคากลับขึ้นมา ตัวเลขนี้ก็ลดลงได้ ขาดทุนจริงเมื่อขายหุ้นออกเท่านั้น</p>
            </div>
          </div>
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> จบมีแค่ 2 ทาง — ได้เงิน {usd(o.credit)} เต็ม หรือได้ {sym} 100 หุ้นในราคา {strike}</p>
        </section>

        <section className="ez-lesson-step">
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 5</span>
            <h2 ref={s5}>โอกาสแต่ละทาง</h2>
          </div>
          <Diagram
            label={`โอกาสของสองทางจบ สำหรับสัญญา ${sym}`}
            source={`pie showData
  title โอกาสที่จะเกิด (ประมาณจากสูตร)
  "ทางที่ 1 จบสวย ได้ ${usd(o.credit)}" : ${keepPct.toFixed(1)}
  "ทางที่ 2 ต้องซื้อหุ้น" : ${o.assign_pct.toFixed(1)}`}
          />
          <p>
            ทุก {perN} ครั้งที่ขายสัญญาแบบนี้ ประมาณ 1 ครั้งจะต้องซื้อหุ้นจริง
            {" "}ตัวเลขนี้คำนวณจากราคาในตลาดตอนนี้ <b>เป็นค่าประมาณ ไม่ใช่คำทำนาย</b>
            {" "}ถ้ามีข่าวร้ายแรงๆ โอกาสก็เปลี่ยนได้
          </p>
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> ก่อนกด ให้ถามตัวเองว่า "ถ้าโชคร้ายต้องซื้อ {sym} 100 หุ้นที่ {strike} จริง ฉันโอเคไหม?" ถ้าไม่โอเค ไม่ควรขายสัญญานี้</p>
        </section>

        <Ladder idea={o} spot={d.spot} when={when} anchorRef={ladderRef} />

        <HistoryStep idea={o} symbol={sym} />

        <section className="ez-lesson-step" ref={quizRef}>
          <div className="ez-lesson-stephead">
            <span className="ez-lesson-no">ขั้น 8</span>
            <h2 ref={s8}>ลองตอบดู</h2>
          </div>
          <Quiz
            question={`ถ้าวันที่ ${when} ${sym} ปิดที่ $${above.toFixed(2)} คุณจะได้อะไร?`}
            options={[
              `ได้ ${usd(o.credit)} และเงิน ${usd(o.cost)} กลับมาใช้ได้`,
              `ต้องซื้อ ${sym} 100 หุ้น`,
              `ต้องคืนเงิน ${usd(o.credit)}`,
            ]}
            correct={0}
            explainRight={`ถูกต้อง: $${above.toFixed(2)} สูงกว่า ${strike} คนซื้อสัญญาจึงไม่ใช้สิทธิ สัญญาจบเอง เงิน ${usd(o.credit)} เป็นของคุณ และเงิน ${usd(o.cost)} ปลดล็อก`}
            explainWrong={`ยังไม่ใช่: ต้องซื้อหุ้นเฉพาะเมื่อราคาปิด "ต่ำกว่า" ${strike} เท่านั้น ส่วนเงิน ${usd(o.credit)} เป็นของคุณตั้งแต่วันแรก ไม่ต้องคืน`}
          />
          <Quiz
            question={`ถ้าวันที่ ${when} ${sym} ปิดที่ $${below} เกิดอะไรขึ้น?`}
            options={[
              "สัญญาจบเอง ไม่มีอะไรเกิดขึ้น",
              `ต้องจ่าย ${usd(o.cost)} ซื้อ ${sym} 100 หุ้น`,
              "Webull ยึดเงินไปทั้งหมด",
            ]}
            correct={1}
            explainRight={`ถูกต้อง: $${below} ต่ำกว่า ${strike} คุณต้องทำตามสัญญา คือจ่าย ${usd(o.cost)} ซื้อ 100 หุ้น ได้หุ้นมาถือ (มูลค่าตอนนั้น ${usd(belowWorth)})`}
            explainWrong={`ยังไม่ใช่: เมื่อปิดต่ำกว่า ${strike} คุณต้องซื้อ 100 หุ้นที่ ${strike} จ่าย ${usd(o.cost)} และได้หุ้นมาถือ เงินไม่ได้หายไปเปล่าๆ`}
          />
          <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> เข้าใจ 2 ทางจบและจุดคุ้มทุน ${`$${o.breakeven.toFixed(2)}`} = พร้อมตัดสินใจเอง ไม่ต้องเดา</p>
        </section>

        <details className="ez-lesson-more">
          <summary>อยากรู้เพิ่ม · เรื่องที่ยังไม่ได้บอกใน 8 ขั้น</summary>
          <div className="ez-lesson-morein">
            <p>
              <b>ถ้าหุ้นร่วงหนัก</b> คุณยังต้องซื้อ {sym} 100 หุ้นที่ {strike} แม้ราคาตลาดจะต่ำกว่านั้นมาก
              {" "}ถ้า {sym} ลง 30% จากวันนี้ ตัวเลขของคุณจะเป็น <span className="ez-neg">−{usd(-o.worst_30_usd)}</span> บนกระดาษ
              {" "}และในทางทฤษฎีถ้าราคาเหลือ $0 จะเสียได้มากสุด {usd(o.max_loss_usd)}
            </p>
            <p>
              <b>ถ้าคุณอยากยกเลิกก่อนวันที่ {when}</b> คุณต้อง "ซื้อสัญญาคืน" ในราคาตลาดตอนนั้น
              {" "}ราคาซื้อคืนขึ้นกับว่าคนอื่นกลัวหุ้นตัวนี้แค่ไหน อาจแพงกว่าเงิน {usd(o.credit)} ที่ได้มา
              {" "}จึงเสียเงินทันทีได้ แม้ราคาหุ้นยังไม่ขยับ ยิ่งมีคนซื้อขายสัญญานี้น้อย การเปลี่ยนใจยิ่งเสียแพง
            </p>
            <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> วางแผนตาม 2 ทางจบเท่านั้น การหวังยกเลิกกลางทางไม่ใช่แผน</p>
          </div>
        </details>

        <p className="ez-note ez-dim">
          <EzIcon name="info" />
          <span>
            ข้อมูลสัญญาจากตัวเก็บข้อมูลของแอป {asOfTh(d.computed_at)} ({sym} ยอมซื้อที่ {strike} หมดอายุ {whenFull} ·
            {" "}โอกาสต้องซื้อหุ้นประมาณ {Math.round(o.assign_pct)}%) · หน้านี้สอนวิธีคิด ไม่ใช่คำแนะนำให้ซื้อขาย
          </span>
        </p>
      </main>
    </div>
  );
}

// ---- sub-components ----

/** One pele diagram: rendered once per source, silently omitted when pele
 *  cannot parse it (the step still reads without the picture). */
function Diagram({ source, label }: { source: string; label: string }) {
  const svg = useMemo(() => {
    try {
      return render(source, { fontFamily: PELE_FONT, fontFamilyMono: PELE_FONT }).svg;
    } catch {
      return null;
    }
  }, [source]);
  if (svg == null) return null;
  return <div className="ez-lesson-dg" role="img" aria-label={label} dangerouslySetInnerHTML={{ __html: svg }} />;
}

/** ขั้น 6 — the price ladder. Zones are NOT to scale on purpose: the
 *  strike→break-even band is only a few cents wide, so it gets a fixed height
 *  of its own and each zone is scaled separately (kept from the prototype). */
function Ladder({ idea: o, spot, when, anchorRef }: {
  idea: EasyIdea;
  spot: number;
  when: string;
  anchorRef: (el: HTMLElement | null) => void;
}) {
  const sym = o.symbol;
  const lo = Math.round(o.strike * 0.7);
  const hi = Math.round(Math.max(spot, o.strike) * 1.1);
  const [price, setPrice] = useState(() => Math.round(Math.min(hi, Math.max(lo, spot)) * 4) / 4);
  const headRef = useAgentAnchor(anchorId("lesson", "step6"), "บทเรียนขั้น 6 บันไดราคา");

  const W = 640, H = 300, TOP = 20, BOT = 280, X0 = 120, X1 = 470, YK = 120, YBE = 176;
  const y = (p: number) => {
    if (p >= o.strike) return YK - Math.min(1, (p - o.strike) / (hi - o.strike)) * (YK - TOP);
    if (p >= o.breakeven) return YK + (o.strike - p) / (o.strike - o.breakeven) * (YBE - YK);
    return YBE + Math.min(1, (o.breakeven - p) / (o.breakeven - lo)) * (BOT - YBE);
  };
  const CX = (X0 + X1) / 2;

  const money = expiryOutcome(price, o);
  const resultCls = price >= o.strike ? "good" : money >= 0 ? "warn" : "bad";
  const resultHtml = price >= o.strike
    ? `ปิดที่ $${price.toFixed(2)} ซึ่งสูงกว่า $${o.strike} → สัญญาจบเอง คุณได้ <b>+${usd(o.credit)}</b> และเงิน ${usd(o.cost)} กลับมาใช้ได้`
    : `ปิดที่ $${price.toFixed(2)} ซึ่งต่ำกว่า $${o.strike} → คุณจ่าย ${usd(o.cost)} ได้ ${sym} 100 หุ้น ซึ่งตอนนั้นมีมูลค่า ${usd(price * 100)} · รวมเงิน ${usd(o.credit)} ที่ได้มาแล้ว สุทธิ <b>${money >= 0 ? "+" : "−"}${usd(money)}</b> (บนกระดาษ ยังถือหุ้นอยู่)`;

  const line = (p: number, text: string, cls: string, dash?: string) => (
    <g key={text}>
      <line x1={X0} x2={X1} y1={y(p)} y2={y(p)} className={cls} strokeWidth={2} strokeDasharray={dash} />
      <text x={X0 - 10} y={y(p) + 5} textAnchor="end" className="ez-lad-price">${p.toFixed(2)}</text>
      <text x={X1 + 10} y={y(p) + 5} className="ez-lad-lab">{text}</text>
    </g>
  );

  return (
    <section className="ez-lesson-step">
      <div className="ez-lesson-stephead">
        <span className="ez-lesson-no">ขั้น 6</span>
        <h2 ref={headRef}>ราคาต้องลงแค่ไหนถึงจะเจ็บ</h2>
      </div>
      <p>ลากแถบด้านล่าง เพื่อสมมติราคา {sym} ในวันที่ {when} แล้วดูว่าเงินของคุณเป็นยังไง</p>
      <div className="ez-lesson-ladwrap">
        <div className="ez-panel quiet ez-lesson-ladder" ref={anchorRef} tabIndex={-1}>
          <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`บันไดราคา ${sym}`}>
            <rect x={X0} y={TOP} width={X1 - X0} height={YK - TOP} className="ez-lad-good" />
            <rect x={X0} y={YK} width={X1 - X0} height={YBE - YK} className="ez-lad-warn" />
            <rect x={X0} y={YBE} width={X1 - X0} height={BOT - YBE} className="ez-lad-bad" />
            {line(spot, "ราคาตอนนี้", "ez-lad-spot", "6 5")}
            {line(o.strike, "ราคาที่นัดซื้อ", "ez-lad-k")}
            {line(o.breakeven, "จุดคุ้มทุน", "ez-lad-be", "3 4")}
            <text x={CX} y={(TOP + YK) / 2 + 30} textAnchor="middle" className="ez-lad-zone good">
              ปิดเหนือ ${o.strike} → ได้ {usd(o.credit)} เต็ม
            </text>
            <text x={CX} y={(YK + YBE) / 2 + 5} textAnchor="middle" className="ez-lad-zone warn">
              ต้องซื้อหุ้น แต่ยังไม่ขาดทุน
            </text>
            <text x={CX} y={(YBE + BOT) / 2 + 5} textAnchor="middle" className="ez-lad-zone bad">
              ต่ำกว่า ${o.breakeven.toFixed(2)} → ขาดทุนบนกระดาษ
            </text>
            <text x={W - 8} y={H - 6} textAnchor="end" className="ez-lad-foot">
              * ระยะห่างไม่ตามสัดส่วนจริง ช่วงสีเหลืองขยายให้เห็นชัด
            </text>
            <circle cx={CX} cy={y(price)} r={9} className="ez-lad-dot" />
          </svg>
        </div>
        <div className="ez-lesson-slider">
          <label htmlFor="ez-lesson-price">
            ราคา {sym} วันที่ {when}: <b>${price.toFixed(2)}</b>
          </label>
          <input
            id="ez-lesson-price"
            type="range"
            min={lo}
            max={hi}
            step={0.25}
            value={price}
            onChange={e => setPrice(Number(e.target.value))}
            aria-label={`ราคา ${sym} วันที่ ${when}`}
          />
        </div>
        <div className={`ez-lesson-result ${resultCls}`} aria-live="polite" dangerouslySetInnerHTML={{ __html: resultHtml }} />
      </div>
      <p className="ez-lesson-note">
        จุดคุ้มทุน ${o.breakeven.toFixed(2)} = ${o.strike} − ${o.premium.toFixed(2)} (เงินที่ได้ต่อหุ้น)
        {" "}ตัวเลขในหน้านี้ยังไม่รวมค่าธรรมเนียม
      </p>
      <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> ต่ำกว่า ${o.strike} คือต้องซื้อหุ้น แต่เริ่มขาดทุนจริงเมื่อต่ำกว่า ${o.breakeven.toFixed(2)}</p>
    </section>
  );
}

/** ขั้น 7 — the real daily closes of the last ~6 months with the strike line.
 *  In the browser preview (or on any error) a plain sentence replaces the
 *  chart — never a fabricated line. */
function HistoryStep({ idea: o, symbol: sym }: { idea: EasyIdea; symbol: string }) {
  const [state, setState] = useState<"loading" | "none" | { t: number; c: number }[]>("loading");
  const headRef = useAgentAnchor(anchorId("lesson", "step7"), "บทเรียนขั้น 7 ราคาย้อนหลัง");
  useEffect(() => {
    if (!easyHasTauri) {
      setState("none");
      return;
    }
    let live = true;
    easyInvoke<HistoryBars>("get_stock_history", { symbol: sym, timespan: "D" })
      .then(h => {
        if (!live) return;
        const closes = h.bars
          .filter(b => b.close != null && Number.isFinite(Date.parse(b.time)))
          .slice(-HISTORY_BARS)
          .map(b => ({ t: Date.parse(b.time), c: b.close as number }));
        setState(closes.length >= 2 ? closes : "none");
      })
      .catch(() => {
        if (live) setState("none");
      });
    return () => { live = false; };
  }, [sym]);

  return (
    <section className="ez-lesson-step">
      <div className="ez-lesson-stephead">
        <span className="ez-lesson-no">ขั้น 7</span>
        <h2 ref={headRef}>ในอดีต ราคาเคยลงถึงเส้นนี้บ่อยไหม</h2>
      </div>
      {state === "loading" && <div className="ez-lesson-chartfall">กำลังโหลดราคาย้อนหลัง…</div>}
      {state === "none" && (
        <div className="ez-lesson-chartfall">
          ยังไม่มีข้อมูลราคาย้อนหลังของ {sym} ตอนนี้ — เปิดแอปเดสก์ท็อปแล้วลองใหม่
          {" "}จะแสดงกราฟราคาจริง ~6 เดือน พร้อมเส้น ${o.strike} ให้เห็นว่าหุ้นตัวนี้เคยลงต่ำกว่าเส้นนี้บ่อยแค่ไหน
        </div>
      )}
      {Array.isArray(state) && <HistoryChart closes={state} strike={o.strike} symbol={sym} />}
      <p className="ez-lesson-remember"><b>จำง่ายๆ:</b> ดูประวัติราคาเพื่อรู้ว่าหุ้นตัวนี้ขึ้นลงแรงแค่ไหน ไม่ใช่เพื่อทำนายวันหน้า</p>
    </section>
  );
}

function HistoryChart({ closes, strike, symbol }: {
  closes: { t: number; c: number }[];
  strike: number;
  symbol: string;
}) {
  const W = 640, H = 260, CL = 50, CR = 620, CT = 18, CB = 228;
  const prices = closes.map(p => p.c);
  const lo = Math.min(...prices, strike);
  const hi = Math.max(...prices, strike);
  const pad = (hi - lo) * 0.08 || 1;
  const mn = lo - pad, mx = hi + pad;
  const cy = (p: number) => CT + (mx - p) / (mx - mn) * (CB - CT);
  // narrow-ranged stocks need cents on the grid or all four ticks read alike
  const tickDec = mx - mn < 8 ? 2 : 0;
  const cx = (i: number) => CL + (i * (CR - CL)) / (closes.length - 1);
  const path = closes.map((p, i) => `${i ? "L" : "M"}${cx(i).toFixed(1)} ${cy(p.c).toFixed(1)}`).join(" ");
  const below = closes.filter(p => p.c < strike).length;

  return (
    <div className="ez-panel quiet ez-lesson-chart">
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`ราคาปิด ${symbol} ย้อนหลัง ~6 เดือน เทียบเส้น $${strike}`}>
        {[0, 1, 2, 3].map(i => {
          const v = mn + ((mx - mn) * i) / 3;
          return (
            <g key={i}>
              <line x1={CL} x2={CR} y1={cy(v)} y2={cy(v)} className="ez-ch-grid" />
              <text x={CL - 8} y={cy(v) + 5} textAnchor="end" className="ez-ch-lab">${v.toFixed(tickDec)}</text>
            </g>
          );
        })}
        <path d={path} className="ez-ch-line" />
        <line x1={CL} x2={CR} y1={cy(strike)} y2={cy(strike)} className="ez-ch-strike" />
        <text x={CR} y={cy(strike) - 8} textAnchor="end" className="ez-ch-strikelab">เส้นราคาที่นัด ${strike}</text>
        <text x={CL} y={252} className="ez-ch-lab">ย้อนหลัง ~6 เดือน (ราคาจริง)</text>
        <text x={CR} y={252} textAnchor="end" className="ez-ch-lab">วันนี้</text>
      </svg>
      <p>
        ใน 6 เดือนที่ผ่านมา ({closes.length} วันที่มีการซื้อขาย)
        {" "}มี <b>{below}</b> วันที่ราคาปิดของ {symbol} ต่ำกว่า <b>${strike}</b>
        {" "}— ยิ่งจำนวนนี้มาก คุณยิ่งต้องเตรียมใจว่าการต้องซื้อหุ้นเป็นเรื่องเป็นไปได้จริง
      </p>
    </div>
  );
}

/** One quiz block: pick an option, see why it is right or wrong. */
function Quiz({ question, options, correct, explainRight, explainWrong }: {
  question: string;
  options: string[];
  correct: number;
  explainRight: string;
  explainWrong: string;
}) {
  const [picked, setPicked] = useState<number | null>(null);
  return (
    <div className="ez-lesson-quizbox">
      <p className="ez-lesson-q"><b>{question}</b></p>
      {options.map((opt, i) => (
        <button
          key={i}
          type="button"
          className={`ez-lesson-opt${picked == null ? "" : i === correct ? " right" : i === picked ? " wrong" : ""}`}
          onClick={() => setPicked(i)}
        >
          {opt}
        </button>
      ))}
      <p className="ez-lesson-fb" aria-live="polite">
        {picked != null && (picked === correct ? explainRight : explainWrong)}
      </p>
    </div>
  );
}
