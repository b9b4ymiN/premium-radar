/** Browser-preview sample data (browser ONLY — inside the desktop app every
 *  screen renders what `easy_ideas` / `save_easy_profile` return). Numbers
 *  mirror the approved prototype exactly: NVDA spot 182.45; safe 165 / 2.20;
 *  balanced 175 / 4.15; high 178 / 2.85 (9 days). */

import type { EasyIdea, EasyIdeas, EasyProfile, IdeaLevel } from "../types/easy";
import { LEVEL_TH, expiryTh, usd } from "./util";

/** Sample exchange rate for the onboarding money hint only. */
export const SAMPLE_THB = 32.4;

/** The prototype's two demo stocks — names and prices for the browser-preview
 *  sub-lines under each ticker. */
export const SAMPLE_STOCKS: Record<string, { name: string; spot: number }> = {
  NVDA: { name: "NVIDIA · ชิปคอมพิวเตอร์", spot: 182.45 },
  VLO: { name: "Valero Energy · โรงกลั่นน้ำมัน", spot: 142.30 },
};

const NVDA_SPOT = SAMPLE_STOCKS.NVDA.spot;
const VLO_SPOT = SAMPLE_STOCKS.VLO.spot;

export function sampleProfile(onboarded: boolean): EasyProfile {
  return {
    onboarded,
    stocks: [
      { symbol: "NVDA", stance: "want" },
      { symbol: "VLO", stance: "avoid" },
    ],
    cash: 18_000,
    risk: 1,
  };
}

function nvdaIdea(
  level: IdeaLevel,
  occ: string,
  strike: number,
  premium: number,
  days: number,
  expiry: string,
  assign: number,
  keep: number,
  mid: number,
  loss: number,
  annual: number,
  warn: boolean,
): EasyIdea {
  const credit = Math.round(premium * 100);
  const cost = strike * 100;
  return {
    occ,
    symbol: "NVDA",
    level,
    recommended: false,
    strike,
    expiry,
    days,
    premium,
    credit,
    cost,
    period_pct: (credit / cost) * 100,
    annual_pct: annual,
    breakeven: strike - premium,
    below_spot_pct: (1 - strike / NVDA_SPOT) * 100,
    assign_pct: assign,
    cases: { keep_pct: keep, between_pct: mid, loss_pct: loss },
    worst_30_usd: (NVDA_SPOT * 0.7 - strike) * 100 + credit,
    max_loss_usd: cost - credit,
    warn,
    cash_short: false,
  };
}

const NVDA_IDEAS: EasyIdea[] = [
  nvdaIdea("safe", "NVDA261218P00165000", 165, 2.20, 23, "2026-12-18", 13, 87, 2, 11, 18.7, false),
  nvdaIdea("balanced", "NVDA261218P00175000", 175, 4.15, 23, "2026-12-18", 22, 78, 4, 18, 33.3, false),
  nvdaIdea("high", "NVDA261204P00178000", 178, 2.85, 9, "2026-12-04", 30, 70, 4, 26, 48.7, true),
];

const NVDA_WHY_NOW =
  "ช่วงนี้คนยอมจ่ายค่าประกันราคาหุ้นแพงกว่าปกติ ราคา option สะท้อนการแกว่ง <strong>48% ต่อปี</strong> " +
  "ขณะที่ 30 วันที่ผ่านมา NVDA แกว่งจริงแค่ <strong>36%</strong> คนขายจึงได้ราคาดี " +
  "และไม่มีวันประกาศงบของ NVDA ก่อนวันหมดอายุ (ข้อมูลตัวอย่าง)";

/** The prototype's recommend() rule, applied to the sample so the preview
 *  reacts to onboarding answers the same way the prototype did: risk comfort
 *  first, then cash. */
function recommendOf(ideas: EasyIdea[], profile: EasyProfile): EasyIdea | null {
  const want = profile.stocks.some(s => s.symbol === "NVDA" && s.stance === "want");
  const cap = want ? profile.risk * 10 + 5 : 15;
  const order: IdeaLevel[] = ["high", "balanced", "safe"];
  const byLevel = (lvl: IdeaLevel) => ideas.find(i => i.level === lvl)!;
  return order.map(byLevel).find(i => i.cost <= profile.cash && i.assign_pct <= cap)
    ?? (byLevel("safe").cost <= profile.cash ? byLevel("safe") : null);
}

/** Sample `easy_ideas` for one symbol — NVDA carries the three ideas, every
 *  other stock answers with the prototype's "not tonight" reasons. */
export function sampleIdeas(symbol: string, profile: EasyProfile): EasyIdeas {
  const sym = symbol.toUpperCase();
  if (sym !== "NVDA") {
    const avoid = profile.stocks.some(s => s.symbol === sym && s.stance === "avoid");
    return {
      symbol: sym,
      spot: sym === "VLO" ? VLO_SPOT : 100,
      source: "DEMO",
      stance: avoid ? "avoid" : "want",
      agent_say_th: `${sym} ยังไม่มีไอเดียที่ดีพอสำหรับคืนนี้`,
      ideas: [],
      skip_reason_th: avoid
        ? `ไอเดียที่มีตอนนี้ มีโอกาสต้องซื้อหุ้นราว 25% ซึ่งมากกว่าที่คุณรับได้ (${profile.risk} ใน 10) และคุณบอกว่าไม่อยากถือ ${sym}`
        : "เงินที่ได้ตอนนี้น้อยเกินไปเมื่อเทียบกับเงินที่ต้องกันไว้ จึงยังไม่คุ้ม",
      why_now: { iv_pct: null, realized_30d_pct: null, text_th: "ยังไม่มีข้อมูล" },
      computed_at: new Date().toISOString(),
    };
  }
  const want = profile.stocks.some(s => s.symbol === "NVDA" && s.stance === "want");
  const rec = recommendOf(NVDA_IDEAS, profile);
  const ideas = NVDA_IDEAS.map(i => ({
    ...i,
    cash_short: i.cost > profile.cash,
    recommended: rec != null && i.level === rec.level,
  }));
  // the contract's agent_say_th is the IDEAS-step sentence (the 3 choices)
  const agent_say_th = rec
    ? `NVDA มี 3 ทางเลือก คุณตั้งเงินไว้ <strong>${usd(profile.cash)}</strong> ${want ? `และยอมให้ต้องซื้อหุ้นได้ราว <strong>${profile.risk} ใน 10</strong> ครั้ง` : `และ<strong>ไม่อยากถือ NVDA</strong> ผมจึงเลือกเฉพาะทางที่โอกาสต้องซื้อหุ้นต่ำ`} ผมแนะนำ <strong>ทาง${LEVEL_TH[rec.level]}</strong>`
    : `เงินที่ตั้งไว้ ${usd(profile.cash)} ยังไม่พอสำหรับ NVDA ทุกทาง ต้องกันไว้อย่างน้อย ${usd(NVDA_IDEAS[0].cost)}`;
  return {
    symbol: "NVDA",
    spot: NVDA_SPOT,
    source: "DEMO",
    stance: want ? "want" : "avoid",
    agent_say_th,
    ideas,
    skip_reason_th: null,
    why_now: { iv_pct: 48, realized_30d_pct: 36, text_th: NVDA_WHY_NOW },
    computed_at: new Date().toISOString(),
  };
}

/** The prototype's held contract (browser preview of "สัญญาที่ขายไปแล้ว"). */
export interface HeldContract {
  symbol: string;
  strike: number;
  expiry: string;
  daysLeft: number;
  credit: number | null;
  spot: number | null;
  avoid: boolean;
}

export const SAMPLE_HELD: HeldContract = {
  symbol: "VLO",
  strike: 130,
  expiry: "2026-12-04",
  daysLeft: 9,
  credit: 95,
  spot: VLO_SPOT,
  avoid: true,
};

/** The prototype's canned ask-agent answers (browser preview only), with every
 *  prototype branch of "ทำไมไม่เลือกตัวที่ได้เงินมากกว่า?" ported: high option
 *  not affordable / stance avoid / viewing the high option itself / the
 *  risk-comfort comparison. */
export function sampleAnswer(question: string, idea: EasyIdea, ideas: EasyIdeas, profile: EasyProfile): string {
  const high = ideas.ideas.find(i => i.level === "high") ?? null;
  const risk = profile.risk;
  switch (question) {
    case "ถ้าได้หุ้นแล้ว ทำอะไรต่อ?":
      return `ถ้าได้ ${ideas.symbol} 100 หุ้น ต้นทุนจริงของคุณคือ $${idea.breakeven.toFixed(2)} ต่อหุ้น (เพราะได้เงิน ${usd(idea.credit)} มาแล้ว) คุณถือไว้เฉยๆ ได้ หรือจะ<strong>ขาย Call</strong> เพื่อรับเงินเพิ่มระหว่างรอ ซึ่งผมจะอธิบายเป็นขั้นตอนให้ตอนนั้นครับ`;
    case "ทำไมไม่เลือกตัวที่ได้เงินมากกว่า?": {
      if (high && high.cost > profile.cash && idea.level !== "high")
        return `ทาง “ได้เงินมากต่อวัน” ต้องกันเงินไว้ ${usd(high.cost)} มากกว่าเงินที่คุณตั้งไว้ (${usd(profile.cash)}) จึงเลือกไม่ได้ และยังมีโอกาสต้องซื้อหุ้นถึง ${Math.round(high.assign_pct)}%`;
      if (ideas.stance !== "want" && idea.level !== "high")
        return `ทาง “ได้เงินมากต่อวัน” มีโอกาสต้องซื้อหุ้น ${Math.round(high?.assign_pct ?? 30)}% ซึ่งไม่เหมาะ เพราะคุณบอกว่าไม่อยากถือ ${ideas.symbol}`;
      if (idea.level === "high")
        return `ทางนี้ได้เงินต่อวันมากที่สุดอยู่แล้ว แต่โอกาสต้องซื้อหุ้น ${Math.round(idea.assign_pct)}% หมายความว่าราว ${Math.round(idea.assign_pct / 10)} ใน 10 ครั้งคุณจะได้หุ้นจริง ${risk < 3 ? `ซึ่งมากกว่าที่คุณตั้งไว้ (${risk} ใน 10)` : "ตรงกับที่คุณตั้งไว้พอดี"}`;
      return high
        ? `ทาง “ได้เงินมากต่อวัน” ได้ ${usd(high.credit)} ใน ${high.days} วัน แต่โอกาสต้องซื้อหุ้น ${Math.round(high.assign_pct)}% หรือราว ${Math.round(high.assign_pct / 10)} ใน 10 ครั้ง ${risk < 3 ? `ซึ่งมากกว่าที่คุณตั้งไว้ (${risk} ใน 10)` : "ถ้ารับได้ ก็เลือกทางนั้นได้"} ทางที่ดูอยู่ได้น้อยกว่า แต่ต้องซื้อหุ้นน้อยครั้งกว่า`
        : `ตอนนี้ยังไม่มีทางเลือกที่ได้เงินมากกว่าทางที่คุณดูอยู่ครับ`;
    }
    case "เปลี่ยนใจกลางทางได้ไหม?":
      return `ได้ครับ คุณ<strong>ซื้อสัญญาคืน</strong>ได้ทุกวันที่ตลาดเปิด ถ้าราคา option ตอนนั้นต่ำกว่า $${idea.premium.toFixed(2)} คุณจะเหลือกำไรบางส่วน ถ้าสูงกว่า จะขาดทุนส่วนต่าง`;
    default:
      return `ส่วนใหญ่การต้องซื้อหุ้นจะตัดสินกันที่ราคาปิด${expiryTh(idea.expiry)} แต่ถ้า ${ideas.symbol} ลงไปต่ำกว่า $${idea.strike} มากๆ ก็<strong>อาจถูกให้ซื้อหุ้นก่อนวันหมดอายุได้</strong> (ใช้เงินก้อนเดิมที่กันไว้) ระหว่างทางราคา option จะแพงขึ้น ถ้าอยากเลิกสัญญาก่อน ต้อง<strong>ซื้อคืน</strong>ในราคาที่แพงกว่า $${idea.premium.toFixed(2)} ที่ขายไป จึงขาดทุนส่วนต่าง แต่ถ้ารอถึงวันหมดอายุแล้วราคากลับขึ้นมาเหนือ $${idea.strike} ก็ยังเก็บ ${usd(idea.credit)} ครบ`;
  }
}
