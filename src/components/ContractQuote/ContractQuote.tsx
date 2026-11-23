//! What Webull itself quotes for the selected contract — sizes, open interest,
//! greeks and the quote's own timestamps — shown next to the model's numbers.
//! The scanner keeps its deterministic greeks; this block is the broker's view
//! of the same contract and never feeds a calculation.

import { useEffect, useRef, useState } from "react";
import type { OptionQuote, OptionTick } from "../../types/webull";
import { count, dateTimeEt, fmt, pct, signedPct, timeEt } from "../../lib/format";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** −0.973 / 0.0030 — small greeks need the extra digit to be readable. */
const greek = (n: number | null) => (n == null ? "--" : n.toFixed(Math.abs(n) < 0.1 ? 4 : 3));
const num = (n: number | null) => (n == null ? "--" : fmt(n));

export default function ContractQuote({
  symbol,
  modelIv,
  modelDelta,
}: {
  symbol: string;
  modelIv: number | null;
  modelDelta: number | null;
}) {
  const [quote, setQuote] = useState<OptionQuote | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [error, setError] = useState<string | null>(null);
  const [fetchedAt, setFetchedAt] = useState<string | null>(null);
  const [ticks, setTicks] = useState<OptionTick[] | null>(null);
  const [ticksOpen, setTicksOpen] = useState(false);
  const [ticksLoading, setTicksLoading] = useState(false);
  const [ticksError, setTicksError] = useState<string | null>(null);
  const request = useRef(0);
  const ticksRequest = useRef(0);

  // One paced call per contract. A short delay means clicking through rows
  // costs a single request, and an answer for a contract already left is dropped.
  useEffect(() => {
    setQuote(null);
    setTicks(null);
    setTicksOpen(false);
    setError(null);
    if (!symbol || !hasTauri) {
      setStatus("ready");
      return;
    }
    const id = ++request.current;
    setStatus("loading");
    const timer = window.setTimeout(() => {
      invokeCmd<{ quote: OptionQuote | null; fetched_at: string }>("get_option_quote", { symbol })
        .then(answer => {
          if (id !== request.current) return;
          setQuote(answer.quote);
          setFetchedAt(answer.fetched_at);
          setStatus("ready");
        })
        .catch((e: unknown) => {
          if (id !== request.current) return;
          setError(message(e));
          setStatus("error");
        });
    }, 350);
    return () => window.clearTimeout(timer);
  }, [symbol]);

  const loadTicks = () => {
    const id = ++ticksRequest.current;
    setTicksOpen(true);
    setTicksLoading(true);
    setTicksError(null);
    invokeCmd<{ ticks: OptionTick[] }>("get_option_ticks", { symbol, count: 12 })
      .then(answer => {
        if (id === ticksRequest.current) setTicks(answer.ticks ?? []);
      })
      .catch((e: unknown) => {
        if (id === ticksRequest.current) setTicksError(message(e));
      })
      .finally(() => {
        if (id === ticksRequest.current) setTicksLoading(false);
      });
  };

  if (!symbol) return null;

  const bid = quote?.bid ?? null;
  const ask = quote?.ask ?? null;
  // same arithmetic as the Rust helper: display only
  const mid = bid != null && ask != null && bid > 0 && ask >= bid ? (bid + ask) / 2 : null;
  const spread = mid != null && mid > 0 && bid != null && ask != null ? (ask - bid) / mid * 100 : null;
  const delayed = (quote?.delay_minutes ?? 0) > 0 ? (quote?.delay_minutes ?? 0) : null;

  return (
    <div className="wq">
      <div className="section-title">
        WEBULL QUOTE
        {quote && fetchedAt && <span>ดึง {timeEt(Date.parse(fetchedAt))}</span>}
      </div>
      {status === "loading" && <div className="wq-status">กำลังดึงราคาจาก Webull …</div>}
      {status === "error" && <div className="wq-status neg">⚠ {error}</div>}
      {status === "ready" && !quote && (
        <div className="wq-status">
          {hasTauri ? "Webull ไม่มีราคาของสัญญานี้" : "ราคาจาก Webull แสดงเฉพาะในแอปเดสก์ท็อป"}
        </div>
      )}
      {quote && (
        <>
          <div className="wq-book num">
            <div>
              <label>Bid</label>
              <strong>{num(bid)}</strong>
              <small>×{count(quote.bid_size)}</small>
            </div>
            <div className="wq-mid">
              <label>Mid</label>
              <strong>{mid == null ? "--" : fmt(mid)}</strong>
              <small className={spread == null ? "" : spread > 10 ? "neg" : spread > 5 ? "warn" : ""}>
                spread {spread == null ? "--" : `${spread.toFixed(1)}%`}
              </small>
            </div>
            <div>
              <label>Ask</label>
              <strong>{num(ask)}</strong>
              <small>×{count(quote.ask_size)}</small>
            </div>
          </div>
          <div className="wq-grid num">
            <div>
              <label>Last</label>
              <span>
                {num(quote.price)}{" "}
                {quote.change_ratio != null && (
                  <small className={quote.change_ratio >= 0 ? "pos" : "neg"}>
                    {signedPct(quote.change_ratio)}
                  </small>
                )}
              </span>
            </div>
            <div>
              <label>Open interest</label>
              <span>{count(quote.open_interest)}</span>
            </div>
            <div>
              <label>Volume</label>
              <span>{count(quote.volume)}</span>
            </div>
            <div>
              <label>IV (Webull)</label>
              <span>{pct(quote.imp_vol)}</span>
            </div>
            <div>
              <label>Delta</label>
              <span>{greek(quote.delta)}</span>
            </div>
            <div>
              <label>Gamma</label>
              <span>{greek(quote.gamma)}</span>
            </div>
            <div>
              <label>Theta</label>
              <span>{greek(quote.theta)}</span>
            </div>
            <div>
              <label>Vega</label>
              <span>{greek(quote.vega)}</span>
            </div>
          </div>
          <div className="wq-compare num">
            เทียบโมเดล: IV {pct(modelIv)} · Delta{" "}
            {modelDelta == null ? "--" : modelDelta.toFixed(3)}
          </div>
          <div className="wq-foot">
            ซื้อขายล่าสุด {dateTimeEt(quote.last_trade_time_ms)}
            {delayed != null && <span className="warn"> · ข้อมูลดีเลย์ {delayed} นาที</span>}
          </div>
          {!ticksOpen ? (
            <button type="button" className="wq-ticks-toggle" onClick={loadTicks}>
              ดูรายการซื้อขายล่าสุดของสัญญานี้
            </button>
          ) : (
            <div className="wq-ticks">
              <div className="section-title">
                RECENT TRADES
                <button type="button" className="wq-ticks-toggle" onClick={() => setTicksOpen(false)}>
                  ซ่อน
                </button>
              </div>
              {ticksLoading && <div className="wq-status">กำลังดึง …</div>}
              {ticksError && <div className="wq-status neg">⚠ {ticksError}</div>}
              {ticks && ticks.length === 0 && <div className="wq-status">ยังไม่มีการซื้อขายของสัญญานี้</div>}
              {ticks && ticks.length > 0 && (
                <table className="wq-tape num">
                  <thead>
                    <tr>
                      <th>เวลา (ET)</th>
                      <th className="num">ราคา</th>
                      <th className="num">จำนวน</th>
                      <th className="num">Side</th>
                    </tr>
                  </thead>
                  <tbody>
                    {ticks.map((tick, index) => (
                      <tr key={`${tick.time_ms ?? index}`}>
                        <td>{timeEt(tick.time_ms)}</td>
                        <td className="num">{num(tick.price)}</td>
                        <td className="num">{count(tick.volume)}</td>
                        <td className="num">{tick.side ?? "--"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </>
      )}
    </div>
  );
}
