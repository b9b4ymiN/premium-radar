/** Browser-preview sample data (browser ONLY — inside the desktop app every
 *  screen renders what `easy_ideas` / `save_easy_profile` return). Numbers
 *  mirror the approved prototype exactly: NVDA spot 182.45; safe 165 / 2.20;
 *  balanced 175 / 4.15; high 178 / 2.85 (9 days). */

import type {
  EasyIdea,
  EasyIdeas,
  EasyProfile,
  IdeaLevel,
  OiContext,
  ScoreParts,
  UniverseIdeas,
  UniversePick,
} from "../types/easy";
import { LEVEL_TH, expiryTh, usd } from "./util";

/** Sample exchange rate for the onboarding money hint only. */
export const SAMPLE_THB = 32.4;

/** The prototype's demo stocks — names and prices for the browser-preview
 *  sub-lines under each ticker. */
export const SAMPLE_STOCKS: Record<string, { name: string; spot: number }> = {
  NVDA: { name: "NVIDIA · ชิปคอมพิวเตอร์", spot: 182.45 },
  VLO: { name: "Valero Energy · โรงกลั่นน้ำมัน", spot: 142.30 },
  BAC: { name: "Bank of America · ธนาคาร", spot: 44.20 },
  F: { name: "Ford · รถยนต์", spot: 11.15 },
};

const NVDA_SPOT = SAMPLE_STOCKS.NVDA.spot;
const VLO_SPOT = SAMPLE_STOCKS.VLO.spot;

export function sampleProfile(onboarded: boolean): EasyProfile {
  return {
    onboarded,
    // the user no longer picks stocks up front — this list fills up with the
    // ถือได้/ไม่อยากถือ answers given on the home idea cards
    stocks: [],
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

/** Sample `easy_universe_ideas` — the assistant's whole-market picks for the
 *  browser preview: one mid-priced stock (NVDA, matching `sampleIdeas` so the
 *  detail flow has its three offers) plus two cheaper ones. When the budget is
 *  below even the cheapest pick, answers with the budget-short variant. */
export function sampleUniverse(profile: EasyProfile): UniverseIdeas {
  const now = new Date().toISOString();
  const today = now.slice(0, 10);
  const marketNote =
    "ภาพรวมตลาดคืนนี้ค่อนข้างสงบ ดัชนีใหญ่ขยับไม่ถึง 1% และค่าประกันราคาหุ้นโดยรวมแพงขึ้นเล็กน้อย (ข้อมูลตัวอย่าง)";

  const pick = (
    rank: number,
    idea: EasyIdea,
    score: number,
    parts: ScoreParts,
    reasons_th: string[],
    cautions_th: string[],
    iv_pct: number | null,
    realized_30d_pct: number | null,
    earnings_date: string | null,
    oi: OiContext,
  ): UniversePick => ({
    rank,
    idea: { ...idea, recommended: rank === 1, cash_short: idea.cost > profile.cash },
    score,
    parts,
    reasons_th,
    cautions_th,
    iv_pct,
    realized_30d_pct,
    earnings_date,
    oi,
  });

  const mkIdea = (
    symbol: string,
    spot: number,
    level: IdeaLevel,
    strike: number,
    premium: number,
    days: number,
    expiry: string,
    assign: number,
    annual: number,
  ): EasyIdea => {
    const credit = Math.round(premium * 100);
    const cost = strike * 100;
    return {
      occ: `${symbol}${expiry.slice(2, 4)}${expiry.slice(5, 7)}${expiry.slice(8, 10)}P${String(Math.round(strike * 1000)).padStart(8, "0")}`,
      symbol,
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
      below_spot_pct: (1 - strike / spot) * 100,
      assign_pct: assign,
      cases: { keep_pct: 100 - assign - 3, between_pct: 3, loss_pct: assign },
      worst_30_usd: (spot * 0.7 - strike) * 100 + credit,
      max_loss_usd: cost - credit,
      warn: false,
      cash_short: false,
    };
  };

  const EXPIRY = "2026-12-18";
  const picks: UniversePick[] = [
    pick(
      1,
      mkIdea("NVDA", NVDA_SPOT, "safe", 165, 2.2, 23, EXPIRY, 13, 18.7),
      78,
      { vrp: 0.32, premium: 0.71, fit: 0.82, cushion: 0.69, liquidity: 0.95, event_penalty: 0 },
      [
        "ค่าประกันราคาหุ้นช่วงนี้แพงกว่าความผันผวนจริงของราคา ขายได้ราคาดี",
        "จุดที่ยอมซื้อต่ำกว่าราคาปัจจุบันราว 10% มีที่ยืนก่อนถึงจุดที่ต้องซื้อ",
        "ไม่มีวันประกาศผลประกอบการก่อนสัญญาหมดอายุ",
      ],
      [
        "ถ้าตลาดหุ้นทั้งระบบดิ่งแรง ราคาอาจลงทะลุจุดที่ต้องซื้อได้",
        "เงินที่ต้องกันไว้ก้อนใหญ่ ต้องแน่ใจว่าไม่ต้องใช้เงินนี้ที่อื่น",
      ],
      48,
      36,
      "2027-02-24",
      { put_wall_strike: 165, put_wall_oi: 41_200, oi_change_pct: 18.4, note_th: "OI ณ ราคา $165 เพิ่มขึ้น 18% ในหนึ่งวัน มีผู้ขายสัญญาไว้ที่ราคานี้เป็นจำนวนมาก (ข้อมูล OI ยังไม่ผ่านการพิสูจน์)" },
    ),
    pick(
      2,
      mkIdea("BAC", SAMPLE_STOCKS.BAC.spot, "balanced", 40, 0.85, 30, EXPIRY, 18, 25.9),
      72,
      { vrp: 0.18, premium: 0.63, fit: 0.75, cushion: 0.72, liquidity: 0.88, event_penalty: 0 },
      [
        "ธนาคารใหญ่มีกำไรสม่ำเสมอ ถ้าราคาลงมาถือไว้ได้สบายใจ",
        "จุดที่ยอมซื้อต่ำกว่าราคาปัจจุบันราว 10%",
      ],
      ["งบที่ต้องกันไว้ 4,000 ดอลลาร์ ต่อ 1 สัญญา"],
      27,
      21,
      "2027-01-12",
      { put_wall_strike: 40, put_wall_oi: 9_800, oi_change_pct: 6.2, note_th: "OI ณ ราคา $40 เพิ่มขึ้น 6% ในหนึ่งสัปดาห์ (ข้อมูล OI ยังไม่ผ่านการพิสูจน์)" },
    ),
    pick(
      3,
      mkIdea("F", SAMPLE_STOCKS.F.spot, "balanced", 10, 0.3, 23, EXPIRY, 22, 47.6),
      69,
      { vrp: 0.22, premium: 0.66, fit: 0.7, cushion: 0.64, liquidity: 0.8, event_penalty: 0 },
      [
        "หุ้นราคาถูก งบเล็กก็ทำได้ เหมาะกับงบที่ตั้งไว้",
        "ค่าประกันราคาหุ้นแพงกว่าความผันผวนจริงพอสมควร",
      ],
      ["โอกาสต้องซื้อหุ้นค่อนข้างสูง ราว 2 ใน 10 ครั้ง"],
      33,
      29,
      "2027-02-04",
      { put_wall_strike: 10, put_wall_oi: 22_400, oi_change_pct: 11.3, note_th: "OI ณ ราคา $10 เพิ่มขึ้น 11% ในหนึ่งวัน (ข้อมูล OI ยังไม่ผ่านการพิสูจน์)" },
    ),
  ];

  const cheapest = Math.min(...picks.map(p => p.idea.cost));
  if (profile.cash < cheapest) {
    const cheapestSym = picks.find(p => p.idea.cost === cheapest)!.idea.symbol;
    return {
      session_date: today,
      as_of: now,
      source: "DEMO",
      scanned_symbols: 104,
      candidate_symbols: 12,
      market_note_th: marketNote,
      picks: [],
      budget_short: {
        min_cash_usd: cheapest,
        cheapest_symbol: cheapestSym,
        text_th: `งบที่ตั้งไว้ ${usd(profile.cash)} ยังน้อยกว่าราคาขั้นต่ำของคืนนี้ ตัวที่ถูกที่สุด (${cheapestSym}) ยังต้องกันเงินไว้ ${usd(cheapest)} ต่อ 1 สัญญา (ตัวอย่าง)`,
      },
      agent_say_th: "คืนนี้ยังไม่มีไอเดียที่งบนี้ทำได้ ลองเพิ่มงบแล้วผมจะหาใหม่ (ตัวอย่าง)",
      computed_at: now,
    };
  }

  const agent_say_th =
    `ผมดูหุ้นทั้งตลาด 104 ตัวแล้ว คืนนี้มี <strong>3 ไอเดีย</strong>ที่เข้ากับงบ ${usd(profile.cash)} ` +
    `อันดับแรกคือ <strong>NVDA</strong> ถ้าราคาลงจนต้องซื้อ คุณจะได้หุ้นในราคาที่ถูกกว่าตอนนี้ราว 10% (ตัวอย่าง)`;
  return {
    session_date: today,
    as_of: now,
    source: "DEMO",
    scanned_symbols: 104,
    candidate_symbols: 12,
    market_note_th: marketNote,
    picks,
    budget_short: null,
    agent_say_th,
    computed_at: now,
  };
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
