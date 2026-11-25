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

export const LEVEL_TH: Record<IdeaLevel, string> = {
  safe: "ปลอดภัย",
  balanced: "สมดุล",
  high: "ได้เงินมากต่อวัน",
};

/** The prototype's market clock line, refreshed each minute. */
export function clockLine(now: Date): { open: boolean; text: string } {
  const fmt = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    weekday: "short", year: "numeric", month: "2-digit", day: "2-digit",
    hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  });
  const thClock = (ms: number) =>
    new Intl.DateTimeFormat("en-GB", { timeZone: "Asia/Bangkok", hour: "2-digit", minute: "2-digit", hour12: false }).format(new Date(ms));
  const parts = Object.fromEntries(fmt.formatToParts(now).map(p => [p.type, p.value]));
  const wd = { Sun: 0, Mon: 1, Tue: 2, Wed: 3, Thu: 4, Fri: 5, Sat: 6 }[parts.weekday as "Sun"] ?? 0;
  const min = Number(parts.hour) * 60 + Number(parts.minute);
  const left = (ms: number) => {
    const t = Math.max(1, Math.ceil((ms - now.getTime()) / 60_000));
    const h = Math.floor(t / 60), m = t % 60;
    return h ? `${h} ชม.${m ? ` ${m} นาที` : ""}` : `${m} นาที`;
  };
  if (wd === 0 || wd === 6) {
    const monday = Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day) + (wd === 6 ? 2 : 1), 13, 30);
    return { open: false, text: `ตลาดหุ้นอเมริกาปิดวันหยุดสุดสัปดาห์ · เปิดคืนวันจันทร์ ${thClock(monday)} น.` };
  }
  if (min >= 570 && min < 960) {
    const close = Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day), 20, 0);
    return { open: true, text: `ตลาดหุ้นอเมริกาเปิดอยู่ · ปิดอีก ${left(close)} (${thClock(close)} น.)` };
  }
  const add = min < 570 ? 0 : wd === 5 ? 3 : 1;
  const open = Date.UTC(Number(parts.year), Number(parts.month) - 1, Number(parts.day) + add, 13, 30);
  return { open: false, text: `ตลาดหุ้นอเมริกายังไม่เปิด · เปิดอีก ${left(open)} (${thClock(open)} น. เวลาไทย)` };
}
