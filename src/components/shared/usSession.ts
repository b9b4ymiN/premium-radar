//! Which US trading session the New York clock is in. The market lists carry
//! whatever session Webull is currently ranking, so the page says which one.
//! Session windows are shown in Thai time, computed for the moment asked
//! about — New York shifts with daylight saving, Bangkok does not, so the
//! windows are derived from the clocks, never hard-coded.

export type UsSession = "pre" | "regular" | "after" | "closed";

const NY = "America/New_York";
const TH = "Asia/Bangkok";

/** Regular-session boundaries on the New York clock. Pre-market runs
 *  04:00–09:30 and after-hours 16:00–20:00 around them. */
const REGULAR_OPEN = 9 * 60 + 30;
const REGULAR_CLOSE = 16 * 60;

const THAI_DAYS = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"];
const WEEKDAY_INDEX: Record<string, number> = { Sun: 0, Mon: 1, Tue: 2, Wed: 3, Thu: 4, Fri: 5, Sat: 6 };

interface NyClock {
  year: number;
  month: number;
  day: number;
  /** 0 = Sunday */
  weekday: number;
  /** minutes since New York midnight */
  minutes: number;
}

/** The New York wall clock of an instant. */
function nyClock(at: Date): NyClock {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: NY,
    weekday: "short",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(at);
  const num = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find(part => part.type === type)?.value ?? "0");
  const weekdayName = parts.find(part => part.type === "weekday")?.value ?? "";
  return {
    year: num("year"),
    month: num("month"),
    day: num("day"),
    weekday: WEEKDAY_INDEX[weekdayName] ?? 0,
    minutes: num("hour") * 60 + num("minute"),
  };
}

/** How far New York wall time is behind UTC at `ms`, in milliseconds
 *  (negative — New York always sits west of the zero meridian). */
function nyOffset(ms: number): number {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: NY,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  }).formatToParts(new Date(ms));
  const num = (type: Intl.DateTimeFormatPartTypes) =>
    Number(parts.find(part => part.type === type)?.value ?? "0");
  return Date.UTC(num("year"), num("month") - 1, num("day"), num("hour"), num("minute"), num("second")) - ms;
}

/** The instant when the New York clock reads `hour:minute` on the NY calendar
 *  day `addDays` away from the one in `clock`. Two offset passes absorb the
 *  daylight-saving step; the session hours sit far from the 02:00 jump. */
function nyInstant(clock: NyClock, addDays: number, hour: number, minute: number): number {
  const wall = Date.UTC(clock.year, clock.month - 1, clock.day + addDays, hour, minute);
  return wall - nyOffset(wall - nyOffset(wall));
}

/** "21:30" — the Thai wall clock of an instant. */
function thClock(ms: number): string {
  return new Intl.DateTimeFormat("en-GB", { timeZone: TH, hour: "2-digit", minute: "2-digit", hour12: false }).format(
    new Date(ms),
  );
}

/** The session a moment falls in by the New York clock: pre-market
 *  04:00–09:30, regular 09:30–16:00, after-hours 16:00–20:00 (each window
 *  half-open on its end); nights and weekends are closed. */
export function usSession(at: Date = new Date()): UsSession {
  const { weekday, minutes } = nyClock(at);
  if (weekday === 0 || weekday === 6) return "closed";
  if (minutes >= 4 * 60 && minutes < REGULAR_OPEN) return "pre";
  if (minutes >= REGULAR_OPEN && minutes < REGULAR_CLOSE) return "regular";
  if (minutes >= REGULAR_CLOSE && minutes < 20 * 60) return "after";
  return "closed";
}

/** Thai label with the window as it falls in Thai time on `now`'s New York
 *  day, e.g. "ตลาดปกติ (21:30–04:00 น. เวลาไทย)". */
export function usSessionText(session: UsSession, now: Date = new Date()): string {
  const clock = nyClock(now);
  const windowTh = (fromH: number, fromM: number, toH: number, toM: number) =>
    `${thClock(nyInstant(clock, 0, fromH, fromM))}–${thClock(nyInstant(clock, 0, toH, toM))} น. เวลาไทย`;
  switch (session) {
    case "pre":
      return `ก่อนตลาดเปิด (${windowTh(4, 0, 9, 30)})`;
    case "regular":
      return `ตลาดปกติ (${windowTh(9, 30, 16, 0)})`;
    case "after":
      return `หลังตลาดปิด (${windowTh(16, 0, 20, 0)})`;
    case "closed":
      return `นอกเวลาซื้อขาย (นอกช่วง ${windowTh(4, 0, 20, 0)} หรือสุดสัปดาห์)`;
  }
}

/** "2 ชม. 15 นาที" — rounded up so the last minute never reads as zero. */
function countdown(target: number, now: Date): string {
  const total = Math.max(1, Math.ceil((target - now.getTime()) / 60_000));
  const hours = Math.floor(total / 60);
  const minutes = total % 60;
  if (hours === 0) return `${minutes} นาที`;
  if (minutes === 0) return `${hours} ชม.`;
  return `${hours} ชม. ${minutes} นาที`;
}

/** Plain-Thai market clock for the regular New York session: open/closed,
 *  how long until the next boundary, all in Thai time. Holidays are not
 *  known, so the countdown simply counts to the next weekday open. */
export function marketClockText(now: Date = new Date()): string {
  const clock = nyClock(now);
  const openOn = (addDays: number) => nyInstant(clock, addDays, 9, 30);
  const dayName = (addDays: number) => THAI_DAYS[new Date(Date.UTC(clock.year, clock.month - 1, clock.day + addDays)).getUTCDay()];

  if (clock.weekday === 0 || clock.weekday === 6) {
    // weekend by the New York calendar: next open is Monday
    return `ตลาดปิดวันหยุดสุดสัปดาห์ · เปิดวันจันทร์ ${thClock(openOn(clock.weekday === 6 ? 2 : 1))} น.`;
  }

  const session = usSession(now);
  const closeToday = nyInstant(clock, 0, 16, 0);
  if (session === "regular") {
    return `ตลาดเปิดอยู่ · ปิดอีก ${countdown(closeToday, now)} (${thClock(closeToday)} น.)`;
  }
  if (session === "pre") {
    return `ช่วงซื้อขายนอกเวลา (ก่อนตลาดเปิด) · ตลาดปกติเปิดอีก ${countdown(openOn(0), now)} (${thClock(openOn(0))} น.)`;
  }
  // after-hours, or a weekday night: the next regular open is on a later NY day
  const addDays = clock.minutes < REGULAR_OPEN ? 0 : clock.weekday === 5 ? 3 : 1;
  const open = openOn(addDays);
  const when = addDays === 0 ? `${thClock(open)} น.` : `วัน${dayName(addDays)} ${thClock(open)} น.`;
  const label = session === "after" ? "ช่วงซื้อขายนอกเวลา (หลังตลาดปิด)" : "ตลาดยังไม่เปิด";
  return `${label} · เปิดอีก ${countdown(open, now)} (${when})`;
}
