//! Quote header. The big price and its change stay the scanner's regular-session
//! numbers (unchanged). Around them the header shows what the broker reports:
//! which session the market is in, the extended-hours trade when there is one,
//! and the four facts an option seller checks first — earnings, ex-dividend,
//! the 52-week range and the size of the company.

import { useEffect, useState, type ReactNode } from "react";
import type { StockSnapshot } from "../../types/webull";
import type { StockUpdate } from "../../types/screener";
import {
  compact,
  count,
  daysUntil,
  fmt,
  pct,
  sessionKind,
  sessionLabel,
  signedPct,
  signOf,
  thaiDate,
  timeEt,
} from "../../lib/format";
import type { OverviewState } from "../../lib/useStockOverview";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The engine reports 0 for a value it does not have. */
const known = (n: number | null | undefined) => (n != null && n > 0 ? n : null);
const num = (n: number | null) => (n == null ? "--" : fmt(n));

function QStat({ label, value, title }: { label: string; value: ReactNode; title?: string }) {
  return (
    <div className="qstat" title={title}>
      <div className="qk">{label}</div>
      <div className="qv num">{value}</div>
    </div>
  );
}

export default function QuoteHeader({
  symbol,
  stock,
  overview,
}: {
  symbol: string;
  stock: StockUpdate | null;
  overview: OverviewState;
}) {
  const [quote, setQuote] = useState<StockSnapshot | null>(null);
  const [quoteAt, setQuoteAt] = useState<string | null>(null);
  const [quoteError, setQuoteError] = useState<string | null>(null);
  const [companyOpen, setCompanyOpen] = useState(false);

  // The session and the extended-hours trade move during pre/after hours:
  // one cheap request a minute, and an answer for a symbol already left is dropped.
  useEffect(() => {
    let live = true;
    setQuote(null);
    setQuoteError(null);
    if (!hasTauri) return;
    const load = () =>
      import("@tauri-apps/api/core")
        .then(({ invoke }) =>
          invoke<{ quote: StockSnapshot | null; fetched_at: string }>("get_stock_quote", { symbol }),
        )
        .then(answer => {
          if (!live) return;
          setQuote(answer.quote);
          setQuoteAt(answer.fetched_at);
          setQuoteError(null);
        })
        .catch((e: unknown) => {
          if (live) setQuoteError(e instanceof Error ? e.message : String(e));
        });
    load();
    const timer = window.setInterval(load, 60_000);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [symbol]);

  // Until the scan seeds the engine, the broker's own regular-session numbers stand in.
  const engine = stock && stock.price > 0 ? stock : null;
  const price = engine ? engine.price : quote?.close ?? null;
  const change = engine ? engine.change : quote?.change ?? null;
  const ratio = engine ? engine.change_ratio : quote?.change_ratio ?? null;
  const open = engine ? known(engine.open) : quote?.open ?? null;
  const high = engine ? known(engine.high) : quote?.high ?? null;
  const low = engine ? known(engine.low) : quote?.low ?? null;
  const preClose = engine ? known(engine.pre_close) : quote?.pre_close ?? null;
  const changeClass = signOf(change);
  const profile = overview.overview?.profile ?? null;
  const facts = overview.overview;
  const subtitle = profile
    ? [profile.company_name, profile.industries[0], profile.exhibition_code].filter(Boolean).join(" · ")
    : overview.loading
      ? "กำลังโหลดข้อมูลหลักทรัพย์ …"
      : "US OPTIONS · Webull TH feed";

  const status = quote?.trade_status ?? null;
  const kind = quote ? sessionKind(status) : null;
  // only an extended-hours session may show the extended trade
  const extended = kind === "extended" ? (quote?.extended ?? null) : null;
  const sessionLine = !hasTauri
    ? "สถานะตลาด · แสดงเฉพาะในแอป"
    : quoteError
      ? "สถานะตลาด: ไม่ทราบ"
      : quote
        ? extended
          ? `${sessionLabel(status)} · ${num(extended.last_price)} ${signedPct(extended.change_ratio)} · ${timeEt(extended.last_trade_time_ms)}`
          : kind === "extended"
            ? `${sessionLabel(status)} · ยังไม่มีราคา · ล่าสุด ${timeEt(quote.last_trade_time_ms)}`
            : `${sessionLabel(status)} · ล่าสุด ${timeEt(quote.last_trade_time_ms)}`
        : "กำลังดึงสถานะตลาด …";
  const dot = quoteError ? "err" : extended ? "ext" : kind === "regular" ? "rth" : "off";

  const earnings = facts?.next_earnings ?? null;
  const earningsDays = earnings ? daysUntil(earnings.start_date) : null;
  // Rust already split the calendar around today and sorted each side by ex-date
  const dividends = facts?.dividends ?? null;
  const nextDividend = dividends?.upcoming[0] ?? null;
  const lastDividend = dividends && dividends.history.length > 0
    ? dividends.history[dividends.history.length - 1]
    : null;

  return (
    <>
      <section className="quoteheader">
        <div className="qh-id">
          <div className="qh-symbol">
            <span className="symbol">{symbol}</span>
            <button
              type="button"
              className={`qh-info ${companyOpen ? "on" : ""}`}
              aria-expanded={companyOpen}
              aria-label="ข้อมูลบริษัท กำไร ปันผล และความเห็นนักวิเคราะห์"
              title="ข้อมูลบริษัท · กำไร · ปันผล · นักวิเคราะห์"
              onClick={() => setCompanyOpen(open => !open)}
            >
              i
            </button>
          </div>
          <div className="qh-sub" title={subtitle}>
            {subtitle}
          </div>
        </div>

        <div className="qh-price">
          <div className={`bigprice num ${changeClass}`}>
            {price == null ? "--" : fmt(price)}
            {price != null && ratio != null && <small className={changeClass}>{signedPct(ratio)}</small>}
          </div>
          <div className="quotefoot num">
            {price == null || change == null || ratio == null
              ? "รอราคาจาก feed ..."
              : `${change >= 0 ? "+" : "-"}${fmt(Math.abs(change))} (${signedPct(ratio)}) · USD`}
          </div>
          <div
            className={`qh-session num ${extended ? "ext" : ""}`}
            title={quoteError ?? (quoteAt ? `ดึงจาก Webull ${timeEt(Date.parse(quoteAt))}` : undefined)}
          >
            {hasTauri && <i className={`session-dot ${dot}`} />}
            {sessionLine}
          </div>
        </div>

        <div className="head-quote-stats">
          <div className="qh-row">
            <QStat label="Open" value={num(open)} />
            <QStat label="High" value={num(high)} />
            <QStat label="Low" value={num(low)} />
            <QStat label="Prev close" value={num(preClose)} />
          </div>
          <div className="qh-row">
            <QStat
              label="Earnings"
              value={earnings ? thaiDate(earnings.start_date) : overview.loading ? "…" : "--"}
              title={
                earnings
                  ? `ช่วงประกาศผลที่ Webull คาด ${thaiDate(earnings.start_date)}${earnings.end_date !== earnings.start_date ? ` ถึง ${thaiDate(earnings.end_date)}` : ""} · ใน ${earningsDays ?? "--"} วัน`
                  : overview.error ?? "Webull ไม่มีข้อมูลวันประกาศผลของหุ้นนี้"
              }
            />
            <QStat
              label="Ex-dividend"
              value={nextDividend?.ex_div_date ? thaiDate(nextDividend.ex_div_date) : "--"}
              title={
                nextDividend
                  ? `เข้าเทรดแบบไม่ได้รับปันผล (ex) วันที่ ${thaiDate(nextDividend.ex_div_date)}${nextDividend.amount != null ? ` · $${fmt(nextDividend.amount, 2)}/หุ้น` : ""}${nextDividend.pay_date ? ` · จ่าย ${thaiDate(nextDividend.pay_date)}` : ""}`
                  : "Webull ไม่มีข้อมูลปันผลที่จะเกิดขึ้นของหุ้นนี้"
              }
            />
            <QStat
              label="52W"
              value={
                quote?.fifty_two_wk_high != null && quote.fifty_two_wk_low != null
                  ? `${fmt(quote.fifty_two_wk_low)}–${fmt(quote.fifty_two_wk_high)}`
                  : "--"
              }
              title="ช่วงราคา 52 สัปดาห์"
            />
            <QStat
              label="Mkt cap"
              value={compact(quote?.market_value)}
              title="มูลค่าตลาดตาม Webull"
            />
          </div>
        </div>
      </section>

      {companyOpen && (
        <section className="companypanel" aria-label="ข้อมูลบริษัท">
          <div className="cp-head">
            <b>{profile?.company_name ?? symbol}</b>
            <span className="cp-source">
              Webull fundamentals{overview.fetchedAt ? ` · ดึงเมื่อ ${timeEt(Date.parse(overview.fetchedAt))}` : ""}
            </span>
            <button type="button" className="qh-info on" aria-label="ปิดข้อมูลบริษัท" onClick={() => setCompanyOpen(false)}>
              ✕
            </button>
          </div>

          {overview.loading && <div className="cp-status">กำลังดึงข้อมูลบริษัทจาก Webull …</div>}
          {overview.error && <div className="cp-status neg">⚠ {overview.error}</div>}
          {facts && (
            <>
              <div className="cp-facts">
                <div>
                  <label>CEO</label>
                  <span>{profile?.ceo ?? "--"}</span>
                </div>
                <div>
                  <label>พนักงาน</label>
                  <span>{count(profile?.employees)}</span>
                </div>
                <div>
                  <label>ก่อตั้งปี</label>
                  <span>{profile?.establish_date ? profile.establish_date.slice(0, 4) : "--"}</span>
                </div>
                <div>
                  <label>อุตสาหกรรม</label>
                  <span>{profile?.industries.length ? profile.industries.join(", ") : "--"}</span>
                </div>
                <div>
                  <label>Mkt cap</label>
                  <span className="num">{compact(quote?.market_value)}</span>
                </div>
                <div>
                  <label>P/E (TTM)</label>
                  <span className="num">{quote?.pe_ratio == null ? "--" : fmt(quote.pe_ratio, 1)}</span>
                </div>
                <div>
                  <label>EPS (TTM)</label>
                  <span className="num">{quote?.eps_ttm == null ? "--" : fmt(quote.eps_ttm, 2)}</span>
                </div>
                <div>
                  <label>Dividend yield</label>
                  <span className="num">{pct(quote?.dividend_yield)}</span>
                </div>
              </div>

              <div className="cp-columns">
                <div className="cp-section">
                  <div className="section-title">
                    EARNINGS
                    {earningsDays != null && earningsDays >= 0 && earningsDays <= 45 && (
                      <span className="warn">ใกล้ประกาศผล — ตลาด option มักเคลื่อนแรง</span>
                    )}
                  </div>
                  {earnings ? (
                    <>
                      <div className="cp-line">
                        <label>ช่วงที่ Webull คาด</label>
                        <b>{thaiDate(earnings.start_date)}</b>
                        {earnings.end_date !== earnings.start_date && (
                          <>
                            {" "}
                            ถึง <b>{thaiDate(earnings.end_date)}</b>
                          </>
                        )}
                      </div>
                      <div className="cp-sub">
                        {[
                          earnings.fiscal_year != null
                            ? `Q${earnings.fiscal_period ?? "--"} ${earnings.fiscal_year}`
                            : null,
                          earnings.eps_est != null ? `EPS คาด ${fmt(earnings.eps_est, 2)}` : null,
                          earningsDays != null && earningsDays >= 0 ? `ใน ${earningsDays} วัน` : null,
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                    </>
                  ) : (
                    <div className="cp-line">Webull ไม่มีข้อมูลวันประกาศผลของหุ้นนี้</div>
                  )}
                </div>

                <div className="cp-section">
                  <div className="section-title">DIVIDEND</div>
                  {nextDividend?.ex_div_date ? (
                    <>
                      <div className="cp-line">
                        <label>เข้า ex ครั้งถัดไป</label>
                        <b>{thaiDate(nextDividend.ex_div_date)}</b>
                        {nextDividend.amount != null && <> · ${fmt(nextDividend.amount, 2)}/หุ้น</>}
                      </div>
                      {nextDividend.pay_date && (
                        <div className="cp-sub">จ่ายเงิน {thaiDate(nextDividend.pay_date)}</div>
                      )}
                    </>
                  ) : (
                    <div className="cp-line">ไม่มีวันเข้า ex ที่รออยู่</div>
                  )}
                  {lastDividend?.ex_div_date && (
                    <div className="cp-sub">
                      ล่าสุด {thaiDate(lastDividend.ex_div_date)}
                      {lastDividend.amount != null && ` ($${fmt(lastDividend.amount, 2)})`}
                    </div>
                  )}
                </div>

                <div className="cp-section">
                  <div className="section-title">ANALYSTS</div>
                  {facts.analyst_target ? (
                    <>
                      <div className="cp-line num">
                        <label>ราคาเป้าหมาย เฉลี่ย</label>
                        <b>
                          {facts.analyst_target.mean == null
                            ? "--"
                            : `$${fmt(facts.analyst_target.mean, 0)}`}
                        </b>
                        {price != null && facts.analyst_target.mean != null && (
                          <span className={signOf(facts.analyst_target.mean - price)}>
                            {" "}
                            {signedPct(facts.analyst_target.mean / price - 1)}
                          </span>
                        )}
                      </div>
                      <div className="cp-sub">
                        {[
                          facts.analyst_target.median != null
                            ? `มัธยฐาน $${fmt(facts.analyst_target.median, 0)}`
                            : null,
                          facts.analyst_target.low != null && facts.analyst_target.high != null
                            ? `ช่วง $${fmt(facts.analyst_target.low, 0)}–$${fmt(facts.analyst_target.high, 0)}`
                            : null,
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                    </>
                  ) : (
                    <div className="cp-line">ไม่มีข้อมูลราคาเป้าหมายของนักวิเคราะห์</div>
                  )}
                  {facts.analyst_rating && (
                    <RatingBar
                      strongBuy={facts.analyst_rating.strong_buy}
                      buy={facts.analyst_rating.buy}
                      hold={facts.analyst_rating.hold}
                      sell={facts.analyst_rating.sell}
                      total={facts.analyst_rating.number}
                    />
                  )}
                </div>
              </div>

              {profile?.profile && (
                <div className="cp-section">
                  <div className="section-title">BUSINESS</div>
                  <p className="cp-profile">{profile.profile}</p>
                  {profile.address && <div className="cp-line cp-dim">{profile.address}</div>}
                </div>
              )}

              {facts.unavailable.length > 0 && (
                <div className="cp-unavailable">
                  ไม่มีข้อมูล: {facts.unavailable.map(item => `${item.section} (${item.reason})`).join(" · ")}
                </div>
              )}
            </>
          )}
        </section>
      )}
    </>
  );
}

/** Recommendation mix as one stacked bar, widths proportional to the counts. */
function RatingBar({
  strongBuy,
  buy,
  hold,
  sell,
  total,
}: {
  strongBuy: number | null;
  buy: number | null;
  hold: number | null;
  sell: number | null;
  total: number | null;
}) {
  const parts = [
    { label: "Strong buy", value: strongBuy, cls: "sb" },
    { label: "Buy", value: buy, cls: "b" },
    { label: "Hold", value: hold, cls: "h" },
    { label: "Sell", value: sell, cls: "s" },
  ];
  const sum = total && total > 0 ? total : parts.reduce((acc, part) => acc + (part.value ?? 0), 0);
  if (!(sum > 0)) return null;
  return (
    <div className="rating">
      <div className="rating-bar" role="img" aria-label={`ความเห็นนักวิเคราะห์ ${count(sum)} คน`}>
        {parts.map(part =>
          part.value ? (
            <i
              key={part.cls}
              className={part.cls}
              style={{ flex: `${Math.max(part.value / sum, 0.02)} 1 0` }}
              title={`${part.label} ${count(part.value)}`}
            />
          ) : null,
        )}
      </div>
      <div className="rating-legend num">
        <span>{count(sum)} ความเห็น</span>
        {parts.map(part => (
          <span key={part.cls}>
            <i className={`dot ${part.cls}`} />
            {part.label} {count(part.value)}
          </span>
        ))}
      </div>
    </div>
  );
}
