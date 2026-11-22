import { useState } from "react";
import type { EnrichedRow, StockUpdate, ChainStats } from "./types/screener";
import "./App.css";

// ---------- mock data (will be replaced by Tauri events) ----------

const MOCK_STOCK: StockUpdate = {
  price: 182.45, open: 180.02, high: 183.72, low: 179.10,
  pre_close: 180.14, volume: 128_400_000,
  change: 2.31, change_ratio: 0.0128, src: "DEMO",
};

const MOCK_ROWS: Partial<EnrichedRow>[] = [
  { q: { symbol: "NVDA261218P00175000", strike: 175, option_type: "P", expiry: "2026-12-18", bid: 4.15, ask: 4.25, oi: 3120, volume: 1842, price: 4.20, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "B", delta: -0.18, credit: 415, collateral: 17500, ann: 26.0, pop: 0.76, cushion: 6.4, p_itm: 0.22, score: 19.6, dte: 26, iv: 0.48, be: 170.85, prem: 4.15, spread_pct: 2.4 },
  { q: { symbol: "NVDA261218P00165000", strike: 165, option_type: "P", expiry: "2026-12-18", bid: 2.20, ask: 2.30, oi: 5480, volume: 2100, price: 2.25, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "A", delta: -0.10, credit: 220, collateral: 16500, ann: 15.0, pop: 0.85, cushion: 11.8, p_itm: 0.13, score: 12.8, dte: 26, iv: 0.48, be: 162.80, prem: 2.20, spread_pct: 4.4 },
  { q: { symbol: "NVDA261218C00195000", strike: 195, option_type: "C", expiry: "2026-12-18", bid: 2.15, ask: 2.25, oi: 2910, volume: 1200, price: 2.20, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "A", delta: 0.16, credit: 215, collateral: 18245, ann: 13.0, pop: 0.82, cushion: 6.9, p_itm: 0.16, score: 10.7, dte: 26, iv: 0.48, be: 180.30, prem: 2.15, spread_pct: 4.5 },
  { q: { symbol: "NVDA261218P00172500", strike: 172.5, option_type: "P", expiry: "2026-12-18", bid: 3.45, ask: 3.55, oi: 4204, volume: 1600, price: 3.50, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "B", delta: -0.22, credit: 345, collateral: 17250, ann: 22.0, pop: 0.72, cushion: 5.1, p_itm: 0.25, score: 15.8, dte: 26, iv: 0.48, be: 169.05, prem: 3.45, spread_pct: 2.9 },
  { q: { symbol: "NVDA261218C00192500", strike: 192.5, option_type: "C", expiry: "2026-12-18", bid: 2.60, ask: 2.70, oi: 1876, volume: 900, price: 2.65, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "B", delta: 0.21, credit: 260, collateral: 18245, ann: 16.0, pop: 0.78, cushion: 5.5, p_itm: 0.19, score: 12.5, dte: 26, iv: 0.48, be: 179.85, prem: 2.60, spread_pct: 3.8 },
  { q: { symbol: "NVDA261218P00170000", strike: 170, option_type: "P", expiry: "2026-12-18", bid: 3.05, ask: 3.15, oi: 6730, volume: 2800, price: 3.10, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "B", delta: -0.13, credit: 305, collateral: 17000, ann: 20.0, pop: 0.81, cushion: 9.1, p_itm: 0.17, score: 16.2, dte: 26, iv: 0.48, be: 166.95, prem: 3.05, spread_pct: 3.2 },
  { q: { symbol: "NVDA261204P00178000", strike: 178, option_type: "P", expiry: "2026-12-04", bid: 2.85, ask: 2.95, oi: 2046, volume: 800, price: 2.90, underlying: "NVDA", src: "DEMO", change: 0, change_ratio: 0 }, grade: "C", delta: -0.28, credit: 285, collateral: 17800, ann: 35.0, pop: 0.68, cushion: 3.2, p_itm: 0.30, score: 23.8, dte: 12, iv: 0.48, be: 175.15, prem: 2.85, spread_pct: 3.4 },
  { q: { symbol: "NVDA261218C00187500", strike: 187.5, option_type: "C", expiry: "2026-12-18", bid: 3.75, ask: 3.85, oi: 1390, volume: 600, price: 3.80, underlying: "NVDA", src: "API", change: 0, change_ratio: 0 }, grade: "C", delta: 0.27, credit: 375, collateral: 18245, ann: 22.0, pop: 0.71, cushion: 2.8, p_itm: 0.24, score: 15.6, dte: 26, iv: 0.48, be: 178.70, prem: 3.75, spread_pct: 2.6 },
];

const WATCHLIST = ["NVDA", "TSLA", "AAPL", "AMD", "SPY", "QQQ", "MSFT", "META"];
const EXPIRIES = ["ทุกวัน", "12 ก.พ.", "26 ก.พ.", "05 มี.ค.", "02 เม.ย."];

const fmt = (n: number, d = 2) =>
  n.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });

// ---------- component helpers ----------

function GradeBadge({ grade }: { grade: string | null }) {
  if (!grade) return <span className="muted">--</span>;
  return <span className={`grade ${grade}`}>{grade}</span>;
}

function SourceDot({ src }: { src: string }) {
  const cls = src === "สด" ? "ws" : src === "API" ? "rest" : "bars";
  return (
    <>
      <i className={`data-dot ${cls}`} />
      <span style={{ fontSize: "9px", color: "var(--dim)" }}>{src}</span>
    </>
  );
}

// ---------- main app ----------

export default function App() {
  const [symbol, setSymbol] = useState("NVDA");
  const [strategy, setStrategy] = useState("CSP+CC");
  const [risk, setRisk] = useState("BALANCED");
  const [expiry, setExpiry] = useState("ทุกวัน");
  const [budget, setBudget] = useState("18000");
  const [tab, setTab] = useState("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [searchText, setSearchText] = useState("");

  const rows = MOCK_ROWS as EnrichedRow[];
  const stock = MOCK_STOCK;
  const selectedRow = rows.find(r => r.q.symbol === selected) ?? rows[0];
  const chg = stock.change;
  const chgPct = stock.change_ratio * 100;
  const chgCls = chg >= 0 ? "pos" : "neg";

  const stats: ChainStats = {
    atm_iv: 0.482,
    em: 14.9,
    skew: 3.1,
  };


  return (
    <div className="app">
      {/* ============ TOP BAR ============ */}
      <header className="appbar">
        <div className="brand">
          <div className="brand-icon">PR</div>
          <div>
            <div className="brand-text">PREMIUM RADAR</div>
            <div className="brand-sub">OPTIONS OPERATION SYSTEM</div>
          </div>
        </div>
        <nav className="headnav">
          <button className="navtab active">Options</button>
          <button className="navtab">Analytics</button>
        </nav>
        <label className="global-search">
          <span>⌕</span>
          <input
            value={searchText}
            onChange={e => setSearchText(e.target.value)}
            placeholder={`Search (${symbol})`}
            aria-label="Search symbol"
          />
        </label>
        <span className="tag-demo">● DEMO DATA</span>
      </header>

      {/* ============ WORKSPACE ============ */}
      <div className="workspace">
        {/* ---- Icon Rail ---- */}
        <nav className="iconbar">
          <button className="iconbtn" title="Home">⌂</button>
          <button className="iconbtn" title="Market">◔</button>
          <button className="iconbtn on" title="Options">|||</button>
          <button className="iconbtn" title="Watchlist">≡</button>
          <button className="iconbar bottom iconbtn" title="Settings">⚙</button>
        </nav>

        {/* ---- Left Panel ---- */}
        <aside className="leftpanel">
          <div className="panel-heading">Watchlist</div>
          <div className="watch-grid">
            {WATCHLIST.map(s => (
              <button
                key={s}
                className={`watch-btn ${s === symbol ? "on" : ""}`}
                onClick={() => setSymbol(s)}
              >
                {s}
              </button>
            ))}
          </div>

          <div className="filterhead">STRATEGY SCANNER</div>
          <div className="filtergroup">
            <div className="filterlabel">Strategy</div>
            <div className="toggle-row">
              {["CSP+CC", "CSP", "CC"].map(s => (
                <button key={s} className={`toggle ${strategy === s ? "on" : ""}`} onClick={() => setStrategy(s)}>
                  {s === "CSP+CC" ? "All" : s === "CSP" ? "Put" : "Call"}
                </button>
              ))}
            </div>
          </div>

          <div className="filtergroup">
            <div className="filterlabel">Assignment risk</div>
            <div className="choice-list">
              {[
                ["ALL", "All contracts", "Any delta"],
                ["SAFE", "Low", "δ .10–.20"],
                ["BALANCED", "Moderate", "δ .15–.30"],
                ["AGGR", "High", "δ .25–.40"],
              ].map(([v, label, meta]) => (
                <button key={v} className={`choice ${risk === v ? "on" : ""}`} onClick={() => setRisk(v)}>
                  <span>{label}</span>
                  <small>{meta}</small>
                </button>
              ))}
            </div>
          </div>

          <div className="filtergroup">
            <div className="filterlabel">Expiration</div>
            <div className="expiries">
              {EXPIRIES.map(e => (
                <button key={e} className={`exp ${expiry === e ? "on" : ""}`} onClick={() => setExpiry(e)}>
                  {e}
                </button>
              ))}
            </div>
          </div>

          <div className="filtergroup">
            <div className="filterlabel">Available cash (USD)</div>
            <label className="cashfield">
              <span>$</span>
              <input value={budget} onChange={e => setBudget(e.target.value)} type="number" min="0" step="500" />
            </label>
          </div>

          <div className="leftfoot">
            <div><b>DATA MODE</b> <span style={{ float: "right", color: "#e0b15f" }}>Synthetic</span></div>
            <div>Reference: 22 Nov 2026</div>
          </div>
        </aside>

        {/* ---- Main ---- */}
        <main className="main">
          <div className="main-tabs">
            <button className="mtab on">Options Monitor</button>
            <button className="mtab">Strategy Lab</button>
          </div>

          {/* Quote Header */}
          <section className="quoteheader">
            <div>
              <div className="symbol">{symbol}</div>
              <div style={{ fontSize: 10, color: "#8797aa", marginTop: 4 }}>NASDAQ · NVIDIA Corporation</div>
            </div>
            <div>
              <div className={`bigprice num ${chgCls}`}>
                {fmt(stock.price)} <small className={chgCls}>{chg >= 0 ? "+" : ""}{chgPct.toFixed(2)}%</small>
              </div>
              <div className="quotefoot num">
                {chg >= 0 ? "+" : ""}{fmt(chg)} ({chg >= 0 ? "+" : ""}{chgPct.toFixed(2)}%) · USD
              </div>
            </div>
            <div className="head-quote-stats">
              {[["Open", stock.open], ["High", stock.high], ["Low", stock.low], ["Prev", stock.pre_close]].map(([k, v]) => (
                <div key={k as string} className="qstat">
                  <div className="qk">{k}</div>
                  <div className="qv num">{fmt(v as number)}</div>
                </div>
              ))}
            </div>
          </section>

          {/* 6 Metric Cards */}
          <section className="summary">
            {[
              { label: "IV (ATM)", value: `${(stats.atm_iv! * 100).toFixed(1)}%`, suffix: "48.2 IV Rank" },
              { label: "Expected move ±1σ", value: `±$${fmt(stats.em!)}`, suffix: "8.2%", cls: "warn" },
              { label: "Put/Call skew", value: `+${stats.skew!.toFixed(1)} pts`, suffix: "hypothetical", cls: "neg" },
              { label: "Put/Call OI", value: "1.24", suffix: "synthetic" },
              { label: "Closest expiry", value: "12 ก.พ.", suffix: "26d", cls: "pos" },
              { label: "Contracts loaded", value: "310", suffix: "model-generated" },
            ].map(m => (
              <div key={m.label} className="metric">
                <div className="metric-label">{m.label}</div>
                <div className={`metric-value ${m.cls ?? ""}`}>
                  {m.value} <span className="metric-suffix">{m.suffix}</span>
                </div>
              </div>
            ))}
          </section>

          {/* 3 Opportunity Cards */}
          <div className="opportunities">
            {[
              { label: "LOWEST DELTA", tag: "Defensive", contract: `${symbol} 165P · 12 ก.พ.`, meta: `+$220 premium · ${fmt(15.0, 1)}% annualized · δ 0.10` },
              { label: "BALANCED SETUP", tag: "Balanced", contract: `${symbol} 175P · 12 ก.พ.`, meta: `+$415 premium · ${fmt(26.0, 1)}% annualized · δ 0.18` },
              { label: "HIGHEST ANNUALIZED", tag: "High yield", contract: `${symbol} 178P · 04 ธ.ค.`, meta: `+$285 premium · ${fmt(35.0, 1)}% annualized · δ 0.28` },
            ].map((o, i) => (
              <button key={o.label} className={`opportunity ${i === 1 ? "on" : ""}`}>
                <div className="otop"><span>{o.label}</span><span style={{ color: "#9caec3" }}>{o.tag}</span></div>
                <div className="ocontract">{o.contract}</div>
                <div className="ometas"><b className="pos">{o.meta.split("·")[0]}</b> · {o.meta.split("·").slice(1).join("·")}</div>
              </button>
            ))}
          </div>

          {/* Scanner Table */}
          <section className="tablemodule">
            <div className="table-head">
              <button className={`t-tab ${tab === "all" ? "on" : ""}`} onClick={() => setTab("all")}>All ({rows.length})</button>
              <button className={`t-tab ${tab === "put" ? "on" : ""}`} onClick={() => setTab("put")}>Put · CSP</button>
              <button className={`t-tab ${tab === "call" ? "on" : ""}`} onClick={() => setTab("call")}>Call · CC</button>
              <div className="tabletools">
                <button className="tool-pill on">Scanner</button>
                <button className="tool-pill">Option Chain</button>
              </div>
            </div>
            <div className="table-status">
              <span>Funnel: 310 → OTM → delta → DTE → spread → premium → budget → <strong>{rows.length}</strong></span>
              <span>Rows <strong>{rows.length}</strong> · USD</span>
            </div>
            <div className="tblscroll">
              <table className="screener-table">
                <thead>
                  <tr>
                    <th>Grade</th><th>Side</th><th>Expiry</th><th>Strike</th><th>Delta</th>
                    <th>Assign</th><th>Premium</th><th>Return</th><th>Ann%</th>
                    <th>Capital</th><th>B/E</th><th>OI</th><th>Source</th>
                  </tr>
                </thead>
                <tbody>
                  {rows
                    .filter(r => tab === "all" || (tab === "put" ? r.q.option_type === "P" : r.q.option_type === "C"))
                    .map(r => (
                    <tr
                      key={r.q.symbol}
                      className={r.q.symbol === selectedRow?.q.symbol ? "selected" : ""}
                      onClick={() => setSelected(r.q.symbol)}
                    >
                      <td><GradeBadge grade={r.grade} /></td>
                      <td style={{ color: r.q.option_type === "P" ? "#f4788c" : "#24c99a", fontWeight: 650 }}>
                        {r.q.option_type === "P" ? "Put" : "Call"}
                      </td>
                      <td>{r.q.expiry.slice(5).replace("-", "/")} <small style={{ color: "#6b7d98" }}>({r.dte}d)</small></td>
                      <td style={{ color: "#e9f1ff", fontWeight: 750 }}>{r.q.strike}</td>
                      <td>{r.delta?.toFixed(2)}</td>
                      <td className={((r.p_itm ?? 0) > 0.25) ? "neg" : ""}>{((r.p_itm ?? 0) * 100).toFixed(0)}%</td>
                      <td className="pos">+{r.credit?.toFixed(0)}</td>
                      <td>{((r.prem ?? 0) / (r.q.strike ?? 1) * 100).toFixed(2)}%</td>
                      <td className="pos">{r.ann?.toFixed(0)}%</td>
                      <td>${r.collateral?.toLocaleString()}</td>
                      <td>{r.be?.toFixed(2)}</td>
                      <td>{r.q.oi?.toLocaleString()}</td>
                      <td><SourceDot src={r.q.src} /></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="table-footer">
              <span>Select a contract to inspect payoff and risk</span>
              <span>US Options · 100 shares/contract</span>
            </div>
          </section>
        </main>

        {/* ---- Right Panel ---- */}
        <aside className="rightpanel">
          {/* Contract Analysis */}
          <section className="detailpanel">
            <div className="panel-heading" style={{ minHeight: "36px" }}>Contract Analysis</div>
            <div className="detail-body">
              {selectedRow && (
                <>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                    <div>
                      <h2>
                        {selectedRow.q.option_type === "P" ? "ขาย Put · CSP" : "ขาย Call · CC"}{" "}
                        {symbol} ${selectedRow.q.strike}
                      </h2>
                      <p>หมดอายุ {selectedRow.q.expiry} ({selectedRow.dte} วัน) · {selectedRow.grade}</p>
                    </div>
                    <GradeBadge grade={selectedRow.grade} />
                  </div>
                  <div className="detail-stats">
                    <div className="ds"><label>Premium</label><strong className="pos">+${selectedRow.credit?.toFixed(0)}</strong></div>
                    <div className="ds"><label>Annualized</label><strong>{selectedRow.ann?.toFixed(1)}%</strong></div>
                    <div className="ds"><label>Collateral</label><strong>${selectedRow.collateral?.toLocaleString()}</strong></div>
                    <div className="ds"><label>Breakeven</label><strong>${selectedRow.be?.toFixed(2)}</strong></div>
                  </div>
                  <div className="section-title">PROBABILITY MODEL</div>
                  <div style={{ display: "flex", gap: 3, height: 7, marginBottom: 8 }}>
                    <div style={{ flex: (selectedRow.pop ?? 0), background: "#13a98a" }} />
                    <div style={{ flex: 0.1, background: "#d9a64b" }} />
                    <div style={{ flex: 1, background: "#cc4869" }} />
                  </div>
                  <div style={{ display: "flex", justifyContent: "space-between", fontSize: "9.5", color: "#9bacc7" }}>
                    <span>OTM {((selectedRow.pop ?? 0) * 100).toFixed(0)}%</span>
                    <span>Near 10%</span>
                    <span>ITM {((selectedRow.p_itm ?? 0) * 100).toFixed(0)}%</span>
                  </div>
                  <div className="section-title">RISK NOTE</div>
                  <div style={{ fontSize: 10, color: "#ebc591", lineHeight: 1.6, borderLeft: "2px solid #bc8c45", paddingLeft: 8 }}>
                    ⚠ IV สูง (48%) — ระวังวันประกาศงบ · กันชน {selectedRow.cushion?.toFixed(1)}%
                  </div>
                </>
              )}
            </div>
          </section>

          {/* Radar Assistant */}
          <section className="copilot">
            <div className="panel-heading" style={{ minHeight: "34px" }}>
              <span style={{ color: "#94beff" }}>✦</span> Radar Assistant
              <span style={{ marginLeft: "auto", fontSize: 9, color: "#8da7ca", fontWeight: 400 }}>DEMO</span>
            </div>
            <div className="chatlog">
              <div className="bubble user">มีเงิน $18,000 ตัวไหนน่าทำ?</div>
              <div className="bubble assistant">
                <b>จากข้อมูลตัวอย่าง</b> — แนะนำ <b>NVDA 175P · 12 ก.พ.</b>{" "}
                รับ $415 ทันที หุ้นลงได้อีก 6.4% ก่อนขาดทุน โอกาสเก็บเงินครบ 76%
              </div>
            </div>
            <div className="chatinput">
              <input placeholder="Ask about this contract..." />
              <button aria-label="Send">➤</button>
            </div>
          </section>
        </aside>
      </div>

      {/* ============ STATUS BAR ============ */}
      <footer className="statusbar">
        <span><b>OFFLINE · DESIGN PROTOTYPE</b></span>
        <span>{symbol} {fmt(stock.price)} <b className={chgCls}>{chg >= 0 ? "+" : ""}{chgPct.toFixed(2)}%</b></span>
        <span>Reference data · 22 Nov 2026</span>
        <span className="statusright">
          <span>Contracts: {rows.length}</span>
          <span>Feed: <b className="warn">DISCONNECTED</b></span>
          <span>PREMIUM RADAR · v0.1</span>
        </span>
      </footer>
    </div>
  );
}
