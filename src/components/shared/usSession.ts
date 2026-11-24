//! Which US trading session the New York clock is in. The market lists carry
//! whatever session Webull is currently ranking, so the page says which one.

export type UsSession = "pre" | "regular" | "after" | "closed";

/** The session a moment falls in by the New York clock: pre-market
 *  04:00–09:30, regular 09:30–16:00, after-hours 16:00–20:00 (each window
 *  half-open on its end); nights and weekends are closed. */
export function usSession(at: Date = new Date()): UsSession {
  const parts = new Intl.DateTimeFormat("en-US", {
    timeZone: "America/New_York",
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  }).formatToParts(at);
  const get = (type: Intl.DateTimeFormatPartTypes) =>
    parts.find(part => part.type === type)?.value ?? "";
  const weekday = get("weekday");
  if (weekday === "Sat" || weekday === "Sun") return "closed";
  const minutes = Number(get("hour")) * 60 + Number(get("minute"));
  if (minutes >= 4 * 60 && minutes < 9 * 60 + 30) return "pre";
  if (minutes >= 9 * 60 + 30 && minutes < 16 * 60) return "regular";
  if (minutes >= 16 * 60 && minutes < 20 * 60) return "after";
  return "closed";
}

/** Thai label with the window it covers, e.g. "ตลาดปกติ (09:30–16:00 ET)". */
export const US_SESSION_TEXT: Record<UsSession, string> = {
  pre: "ก่อนตลาดเปิด (04:00–09:30 ET)",
  regular: "ตลาดปกติ (09:30–16:00 ET)",
  after: "หลังตลาดปิด (16:00–20:00 ET)",
  closed: "ช่วงกลางคืน/ปิด (นอกช่วง 04:00–20:00 ET หรือสุดสัปดาห์)",
};
