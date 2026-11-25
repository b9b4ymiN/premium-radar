/** Easy Mode display helpers — money in the prototype's exact shapes and
 *  expiry dates as Thai weekday + short date ("ศุกร์ 18 ธ.ค."), which is how
 *  every screen in the approved prototype writes them. */

import type { IdeaLevel } from "../types/easy";

export const usd = (n: number, d = 0) =>
  "$" + Math.abs(n).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });

/** "+$220" / "−$4,313" — the minus is the typographic U+2212, as in the prototype. */
export const signed = (n: number) => (n >= 0 ? "+" : "−") + usd(n);

const THAI_DAYS = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัส", "ศุกร์", "เสาร์"];
const THAI_MONTHS = ["ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค."];

/** "ศุกร์ 18 ธ.ค." from an ISO date; the date is a New York calendar date. */
export function expiryTh(iso: string): string {
  const d = new Date(`${iso.slice(0, 10)}T12:00:00Z`);
  if (Number.isNaN(d.getTime())) return iso;
  return `${THAI_DAYS[d.getUTCDay()]} ${d.getUTCDate()} ${THAI_MONTHS[d.getUTCMonth()]}`;
}

/** Whole days from today until an ISO date; negative when past. */
export function daysUntil(iso: string): number {
  const target = Date.parse(`${iso.slice(0, 10)}T12:00:00Z`);
  const now = Date.parse(new Date().toISOString().slice(0, 10) + "T12:00:00Z");
  return Math.round((target - now) / 86_400_000);
}

/** Data-source badge shared with pro mode: LIVE / API / ราคาปิด (CLOSE) / ตัวอย่าง (DEMO). */
export function sourceLabel(source: string): string {
  const key = (source ?? "").toUpperCase();
  if (key === "LIVE") return "LIVE";
  if (key === "API") return "API";
  if (key === "CLOSE" || key === "ราคาปิด") return "ราคาปิด";
  return "ตัวอย่าง";
}

/** One data badge, never two: sample data says "ข้อมูลตัวอย่าง", a real source
 *  says its own LIVE / API / ราคาปิด label. */
export function badgeText(source: string, sample: boolean): string {
  return sample ? "ข้อมูลตัวอย่าง" : sourceLabel(source);
}

export const LEVEL_TH: Record<IdeaLevel, string> = {
  safe: "ปลอดภัย",
  balanced: "สมดุล",
  high: "ได้เงินมากต่อวัน",
};
