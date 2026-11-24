/** Display helpers shared by the pages. Market times are shown in New York
 *  time because that is the clock the contracts trade on. */

const ET = "America/New_York";
const DAY_MS = 86_400_000;

export const fmt = (n: number, d = 2) =>
  n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });

const valid = (ms: number | null | undefined): ms is number =>
  typeof ms === "number" && Number.isFinite(ms) && ms > 0;

const et = (ms: number, options: Intl.DateTimeFormatOptions) =>
  new Intl.DateTimeFormat("en-GB", { timeZone: ET, ...options }).format(new Date(ms));

/** "13:45:07 ET" */
export const timeEt = (ms: number | null | undefined) =>
  valid(ms) ? `${et(ms, { hour: "2-digit", minute: "2-digit", second: "2-digit", hour12: false })} ET` : "--";

/** "20 Nov" */
export const dayEt = (ms: number | null | undefined) =>
  valid(ms) ? et(ms, { day: "2-digit", month: "short" }) : "--";

/** "20 Nov · 13:45 ET" */
export const dateTimeEt = (ms: number | null | undefined) =>
  valid(ms)
    ? `${et(ms, { day: "2-digit", month: "short" })} · ${et(ms, { hour: "2-digit", minute: "2-digit", hour12: false })} ET`
    : "--";

/** "2.5K" / "184.7K" / "3.0M" / "3.0B" / "3.0T" */
export const compact = (n: number | null | undefined) =>
  n == null || !Number.isFinite(n)
    ? "--"
    : new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 1 }).format(n);

/** "+1.28%" for a fraction; null stays "--". */
export const signedPct = (ratio: number | null | undefined, d = 2) =>
  ratio == null || !Number.isFinite(ratio) ? "--" : `${ratio >= 0 ? "+" : ""}${(ratio * 100).toFixed(d)}%`;

/** "1.28%" for a fraction already known to be non-negative in meaning. */
export const pct = (ratio: number | null | undefined, d = 2) =>
  ratio == null || !Number.isFinite(ratio) ? "--" : `${(ratio * 100).toFixed(d)}%`;

/** Whole count with thousands separators. */
export const count = (n: number | null | undefined) =>
  n == null || !Number.isFinite(n) ? "--" : Math.round(n).toLocaleString("en-US");

/** Buddhist-era Thai date: "23 ก.พ. 70" */
const THAI_MONTHS = ["ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค."];
export const thaiDate = (iso: string | null | undefined) => {
  if (!iso || iso.length < 10) return "--";
  const [y, m, d] = iso.slice(0, 10).split("-").map(Number);
  if (!y || !m || !d) return iso;
  return `${d} ${THAI_MONTHS[m - 1] ?? ""} ${String(y + 543).slice(-2)}`;
};

/** Whole days from today (New York) until an ISO date; negative when past. */
export const daysUntil = (iso: string | null | undefined) => {
  if (!iso || iso.length < 10) return null;
  const target = Date.parse(`${iso.slice(0, 10)}T00:00:00Z`);
  if (!Number.isFinite(target)) return null;
  const now = Date.parse(new Intl.DateTimeFormat("en-CA", { timeZone: ET }).format(new Date()) + "T00:00:00Z");
  return Math.round((target - now) / DAY_MS);
};

/** "23 ก.พ. 70 · อีก 92 วัน" / "… · ผ่านแล้ว 3 วัน" */
export const thaiDateWithDays = (iso: string | null | undefined) => {
  if (!iso) return "--";
  const days = daysUntil(iso);
  if (days == null) return iso;
  if (days === 0) return `${thaiDate(iso)} · วันนี้`;
  return `${thaiDate(iso)} · ${days > 0 ? `อีก ${days} วัน` : `ผ่านแล้ว ${-days} วัน`}`;
};

/** Signed money: "+$415" / "-$120". */
export const signedUsd = (n: number | null | undefined, d = 0) =>
  n == null || !Number.isFinite(n)
    ? "--"
    : `${n >= 0 ? "+" : "-"}$${Math.abs(n).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d })}`;

export const usd = (n: number | null | undefined, d = 2) =>
  n == null || !Number.isFinite(n)
    ? "--"
    : `$${n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d })}`;

export type Sign = "pos" | "neg" | "";

/** Colour class for a signed number; zero and unknown stay neutral. */
export const signOf = (n: number | null | undefined): Sign =>
  n == null || !Number.isFinite(n) || n === 0 ? "" : n > 0 ? "pos" : "neg";

/** What the broker's session tag means for the quote header. The tags seen
 *  live from the Thailand host are PRE and ATH; the other spellings are the
 *  ones the reference docs use. */
export type SessionKind = "regular" | "extended" | "closed" | "other";

export const sessionKind = (status: string | null | undefined): SessionKind => {
  const code = (status ?? "").toUpperCase();
  if (["PRE", "ATH", "POST", "AFTER", "AFTER_HOURS", "AH"].includes(code)) return "extended";
  if (["RTH", "REGULAR", "REG", "OPEN", "TRADING"].includes(code)) return "regular";
  if (["CLOSED", "CLOSE"].includes(code)) return "closed";
  return "other";
};

/** Broker session tag → label; unknown tags are shown as returned. */
export const sessionLabel = (status: string | null | undefined): string => {
  const code = (status ?? "").toUpperCase();
  if (code === "PRE") return "Pre-market";
  if (code === "ATH" || code === "POST" || code === "AFTER" || code === "AFTER_HOURS" || code === "AH") return "After-hours";
  if (code === "RTH" || code === "REGULAR" || code === "REG" || code === "OPEN" || code === "TRADING") return "Regular session";
  if (code === "CLOSED" || code === "CLOSE") return "Closed";
  return code || "--";
};
