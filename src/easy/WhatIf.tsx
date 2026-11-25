/** "ลองดูว่า ถ้าวันหมดอายุ … ปิดที่…" — the what-if slider, ported from the
 *  prototype with the same maths: the slider is a % change of today's spot,
 *  band segments and the strike tick are positioned by mapping prices onto
 *  the −40%…+25% range, and the money figure tweens with an ease-out curve
 *  (skipped entirely under prefers-reduced-motion). */

import { useEffect, useRef, useState } from "react";
import type { EasyIdea } from "../types/easy";
import { expiryTh, signed, usd } from "./util";

const MIN = -40;
const MAX = 25;

interface Props {
  idea: EasyIdea;
  spot: number;
  symbol: string;
}

export function WhatIf({ idea, spot, symbol }: Props) {
  const [chg, setChg] = useState(0);
  const [display, setDisplay] = useState<number | null>(null);
  const shownRef = useRef<number | null>(null);
  const raf = useRef(0);

  const toPos = (px: number) => ((px / spot - 1) * 100 - MIN) / (MAX - MIN) * 100;
  const price = spot * (1 + chg / 100);
  const money = price >= idea.strike ? idea.credit : (price - idea.strike) * 100 + idea.credit;

  useEffect(() => {
    const to = Math.round(money);
    cancelAnimationFrame(raf.current);
    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (shownRef.current === null || reduce) {
      shownRef.current = to;
      setDisplay(to);
      return;
    }
    const from = shownRef.current;
    const t0 = performance.now();
    const dur = 380;
    const step = (now: number) => {
      const k = Math.min(1, (now - t0) / dur);
      const e = k === 1 ? 1 : 1 - Math.pow(2, -10 * k);
      const v = Math.round(from + (to - from) * e);
      shownRef.current = v;
      setDisplay(v);
      if (k < 1) raf.current = requestAnimationFrame(step);
    };
    raf.current = requestAnimationFrame(step);
    return () => cancelAnimationFrame(raf.current);
  }, [money]);

  const be = Math.max(0, Math.min(100, toPos(idea.breakeven)));
  const k = Math.max(0, Math.min(100, toPos(idea.strike)));

  const outcomeText = price >= idea.strike
    ? `ราคาสูงกว่า $${idea.strike} <strong style="color:var(--text)">ไม่ต้องซื้อหุ้น</strong> เก็บเงิน ${usd(idea.credit)} ครบ และเงิน ${usd(idea.cost)} ที่กันไว้กลับมาใช้ได้`
    : `ราคาต่ำกว่า $${idea.strike} <strong style="color:var(--text)">ต้องซื้อ ${symbol} 100 หุ้น</strong> จ่าย ${usd(idea.cost)} ได้หุ้นมูลค่า ${usd(price * 100)} ` +
      (money >= 0
        ? `รวมเงินที่ได้มาแล้ว ยัง${money === 0 ? "เท่าทุน" : "กำไรบนกระดาษ"}`
        : `รวมเงินที่ได้มาแล้ว ขาดทุนบนกระดาษ จนกว่าราคาจะกลับขึ้นมา`);

  return (
    <div className="ez-whatif">
      <div className="ez-whatif-head">
        <h2>ลองดูว่า ถ้าวันหมดอายุ {symbol} ปิดที่…</h2>
        <div className="ez-whatif-price">
          ราคา<b>{usd(price, 2)}</b>{" "}
          <span className="ez-dim">{chg === 0 ? "(เท่าวันนี้)" : `(${chg > 0 ? "ขึ้น" : "ลง"} ${Math.abs(chg)}%)`}</span>
        </div>
      </div>
      <div className="ez-slider">
        <div className="ez-band" aria-hidden="true">
          <span style={{ width: `${be}%`, background: "#3a1c24", boxShadow: "inset 0 0 0 1px #6b2c38" }} />
          <span style={{ width: `${Math.max(0, k - be)}%`, background: "#3a2e1a", boxShadow: "inset 0 0 0 1px #6a5229" }} />
          <span style={{ flex: 1, background: "#12332b", boxShadow: "inset 0 0 0 1px #1f6b55" }} />
        </div>
        <span className="ez-tick" style={{ left: `${k}%` }} />
        <span className="ez-tick-label" style={{ left: `${k}%` }}>ต่ำกว่า ${idea.strike} ต้องซื้อหุ้น</span>
        <input
          type="range"
          min={MIN}
          max={MAX}
          step={1}
          value={chg}
          onChange={e => setChg(Number(e.target.value))}
          aria-label={`ราคา ${symbol} วันหมดอายุ เทียบกับวันนี้ (เปอร์เซ็นต์)`}
        />
      </div>
      <div className="ez-band-labels">
        <span style={{ left: 0 }}>ลง 40%</span>
        <span style={{ left: `${toPos(spot)}%` }}>เท่าวันนี้</span>
        <span style={{ left: "100%" }}>ขึ้น 25%</span>
      </div>
      <div className="ez-outcome">
        <div className={`ez-money ${price >= idea.strike ? "ez-pos" : money >= 0 ? "ez-warn" : "ez-neg"}`}>
          {display == null ? signed(money) : signed(display)}
        </div>
        <p dangerouslySetInnerHTML={{ __html: outcomeText }} />
      </div>
      <div className="ez-quick">
        <button className="ez-pill" onClick={() => setChg(0)}>ราคาเท่าเดิม</button>
        <button className="ez-pill" onClick={() => setChg(-10)}>ลง 10% วัน{expiryTh(idea.expiry)}</button>
        <button className="ez-pill" onClick={() => setChg(-20)}>ลง 20%</button>
        <button className="ez-pill" onClick={() => setChg(-30)}>ลง 30%</button>
      </div>
    </div>
  );
}
