/** IdeaCard (W6 P4) — the structured recommendation card `present_idea`
 *  shows inside a chat turn. Every number is formatted from `card.numbers`
 *  (computed by Rust from the contract ref); the five answers render as
 *  plain Thai text and are never parsed for numbers. The card is a compact
 *  version of Easy Mode's five-questions layout (src/easy/FiveQuestions.tsx). */

import { useAgentAnchor } from "./anchors";
import type { IdeaCardData } from "./types";
import { asOfTh, expiryTh, signed, usd } from "../easy/util";

interface Props {
  card: IdeaCardData;
  /** easy = plain words only; pro may add strike/expiry shorthand. */
  mode: "easy" | "pro";
}

/** The five questions in the fixed order the user already knows. */
const QUESTIONS: Array<{ key: keyof IdeaCardData["answers"]; q: string }> = [
  { key: "gain", q: "ได้เงินเท่าไร" },
  { key: "tradeoff", q: "แลกกับอะไร" },
  { key: "chance", q: "โอกาสแค่ไหน" },
  { key: "why_now", q: "ทำไมช่วงนี้" },
  { key: "worst", q: "แย่สุดคืออะไร" },
];

/** "$175" for whole strikes, "$175.50" otherwise. */
const strikeText = (n: number) => usd(n, Number.isInteger(n) ? 0 : 2);

/** Pro shorthand "175P"/"175.50C" — the strike comes from `numbers`, only the
 *  P/C letter is read from the OCC ref (display-only, never a data source). */
function contractShort(card: IdeaCardData): string {
  const pc = card.ref.slice(-9, -8);
  if (pc !== "P" && pc !== "C") return card.symbol;
  return `${strikeText(card.numbers.strike).replace("$", "")}${pc}`;
}

function NumTile({
  id,
  label,
  value,
  tone,
}: {
  id: string;
  label: string;
  value: string;
  tone?: "pos" | "neg";
}) {
  const ref = useAgentAnchor(id, label);
  return (
    <div className="agent-card-num" ref={ref}>
      <span className="agent-card-num-label">{label}</span>
      <b className={`agent-card-num-value${tone ? ` ${tone}` : ""}`}>{value}</b>
    </div>
  );
}

export function IdeaCard({ card, mode }: Props) {
  const n = card.numbers;
  const rootRef = useAgentAnchor(`card:${card.ref}`, `การ์ดคำแนะนำ ${card.symbol}`);
  const days = Math.round(n.days);
  const sub = mode === "pro"
    ? `${contractShort(card)} · ${card.ref}`
    : `${card.symbol} · หมดอายุ ${expiryTh(n.expiry)} · อีก ${days} วัน`;

  return (
    <section className="agent-card" ref={rootRef}>
      <header className="agent-card-head">
        <h3>{card.headline}</h3>
        <div className="agent-card-sub">{sub}</div>
      </header>
      <div className="agent-card-nums">
        <NumTile id={`card:${card.ref}/premium`} label="เงินที่ได้ทันที" value={usd(n.premium_usd)} tone="pos" />
        <NumTile id={`card:${card.ref}/cash`} label="เงินที่ต้องกันไว้" value={usd(n.cash_needed_usd)} />
        <NumTile id={`card:${card.ref}/strike`} label="ราคาที่ยอมซื้อ" value={strikeText(n.strike)} />
        <NumTile id={`card:${card.ref}/breakeven`} label="จุดคุ้มทุน" value={usd(n.breakeven, 2)} />
        <NumTile id={`card:${card.ref}/assign`} label="โอกาสต้องซื้อหุ้น" value={n.assign_pct == null ? "—" : `${Math.round(n.assign_pct)}%`} />
        <NumTile id={`card:${card.ref}/worst30`} label="ถ้าหุ้นลง 30%" value={n.worst_30_usd == null ? "—" : signed(n.worst_30_usd)} tone={n.worst_30_usd != null && n.worst_30_usd < 0 ? "neg" : undefined} />
        <NumTile id={`card:${card.ref}/maxloss`} label="ขาดทุนสูงสุด" value={n.max_loss_usd == null ? "ไม่จำกัด" : usd(n.max_loss_usd)} />
        <NumTile id={`card:${card.ref}/expiry`} label="หมดอายุ" value={`${expiryTh(n.expiry)} · อีก ${days} วัน`} />
      </div>
      <div className="agent-card-answers">
        {QUESTIONS.map(({ key, q }) => (
          <div className="agent-card-answer" key={key}>
            <h4>{q}</h4>
            <p>{card.answers[key]}</p>
          </div>
        ))}
      </div>
      <footer className="agent-card-foot">ตัวเลขทั้งหมดคำนวณจากข้อมูลจริง ณ {asOfTh(card.as_of)}</footer>
    </section>
  );
}
