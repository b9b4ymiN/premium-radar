import { useState, useEffect, useCallback, useMemo } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { SetupScreen } from "./components/SetupScreen";
import type { EnrichedRow, StockUpdate, ChainStats, EngineSnapshot } from "./types/screener";
import "./App.css";

// Tauri bridge (graceful fallback when running without the Tauri shell)
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!hasTauri) throw new Error("Tauri not available");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}
async function tauriListen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  const un = await listen<T>(event, (e) => handler(e.payload));
  return un;
}

interface ScanProgress {
  phase: string; // discovery | seed | bars | live | error
  message: string;
}

// ---------- mock data (browser preview ONLY — never inside the desktop app) ----------

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

const MOCK_STATS: ChainStats = { atm_iv: 0.482, em: 14.9, skew: 3.1 };

const WATCHLIST = ["NVDA", "TSLA", "AAPL", "AMD", "SPY", "QQQ", "MSFT", "META"];

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
  const [budget, setBudget] = useState("18000");
  const [tab, setTab] = useState("all");
  const [selected, setSelected] = useState<string | null>(null);
  const [searchText, setSearchText] = useState("");
  const [tauriReady, setTauriReady] = useState(false);
  const [showSetup, setShowSetup] = useState(false);
  const [setupMode, setSetupMode] = useState<"first-run" | "edit">("first-run");

  // live data from the Rust scan worker
  const [snap, setSnap] = useState<EngineSnapshot | null>(null);
  const [scanMsg, setScanMsg] = useState("กำลังเชื่อมต่อ backend ...");
  const [scanPhase, setScanPhase] = useState("idle");

  // AI copilot — its configuration is INDEPENDENT of the data feed
  const [llmInfo, setLlmInfo] = useState<{ label: string; model: string } | null>(null);
  const [chat, setChat] = useState<Array<{ role: "user" | "assistant"; content: string }>>([]);
  const [chatInput, setChatInput] = useState("");
  const [chatBusy, setChatBusy] = useState(false);
  const [chatError, setChatError] = useState("");

  // Connect listeners + restore last session once
  useEffect(() => {
    if (!hasTauri) return;
    const uns: Array<() => void> = [];
    let disposed = false;
    (async () => {
      try {
        uns.push(await tauriListen<ScanProgress>("scan://progress", (p) => {
          setScanPhase(p.phase);
          setScanMsg(p.message);
        }));
        uns.push(await tauriListen<EngineSnapshot>("scan://snapshot", (s) => setSnap(s)));

        const needs = await tauriInvoke<boolean>("needs_setup");
        setSetupMode("first-run");
        setShowSetup(needs);

        const st = await tauriInvoke<Record<string, unknown>>("get_settings");
        const lastSym = typeof st?.last_symbol === "string" && st.last_symbol ? st.last_symbol : null;
        const lastStrat = typeof st?.last_strategy === "string" && st.last_strategy ? st.last_strategy : "CSP+CC";
        const lastRisk = typeof st?.last_risk === "string" && st.last_risk ? st.last_risk : "BALANCED";
        const lastBudget = typeof st?.last_budget === "number" && st.last_budget > 0 ? st.last_budget : 18000;
        setStrategy(lastStrat);
        setRisk(lastRisk);
        setBudget(String(lastBudget));
        if (lastSym) setSymbol(lastSym);
        // sync engine filters with the restored UI state (no expiry pinned yet)
        await tauriInvoke("set_filters", { strategy: lastStrat, risk: lastRisk, expiry: null, budget: lastBudget }).catch(() => {});
        if (!disposed) setTauriReady(true);
      } catch {
        setTauriReady(false);
      }
    })();
    return () => { disposed = true; uns.forEach((u) => u()); };
  }, []);

  // Start / restart the scan whenever the symbol changes (or setup completes);
  // opening Setup stops any running scan (it may be failing on bad keys).
  useEffect(() => {
    if (!tauriReady || showSetup) return;
    setExpirySel("ทุกวัน");
    tauriInvoke("start_scan", { symbol }).catch((e: unknown) => {
      const msg = e instanceof Error ? e.message : String(e);
      setScanMsg(msg);
      setScanPhase("error");
      if (msg.includes("Setup")) setShowSetup(true);
    });
  }, [symbol, tauriReady, showSetup]);

  useEffect(() => {
    if (showSetup && tauriReady) tauriInvoke("stop_scan").catch(() => {});
  }, [showSetup, tauriReady]);

  // Track AI configuration separately from the data feed (setup may change it)
  useEffect(() => {
    if (!tauriReady || showSetup) return;
    tauriInvoke<Record<string, unknown>>("get_settings")
      .then((st) => {
        const llm = st?.llm as Record<string, unknown> | null | undefined;
        const ready = !!llm
          && typeof llm.base_url === "string" && llm.base_url
          && typeof llm.model === "string" && llm.model
          && typeof llm.api_key === "string" && llm.api_key;
        setLlmInfo(ready
          ? { label: (typeof llm!.label === "string" && llm!.label) || "AI", model: llm!.model as string }
          : null);
      })
      .catch(() => {});
  }, [tauriReady, showSetup]);

  // Send a question to the copilot (function-calling over live engine data)
  const sendChat = useCallback(async () => {
    const q = chatInput.trim();
    if (!q || chatBusy || !hasTauri) return;
    setChatInput("");
    setChatError("");
    const next = [...chat, { role: "user" as const, content: q }];
    setChat(next);
    setChatBusy(true);
    try {
      const answer = await tauriInvoke<string>("ask_copilot", { messages: next });
      setChat([...next, { role: "assistant", content: answer }]);
    } catch (e) {
      setChatError(e instanceof Error ? e.message : String(e));
    } finally {
      setChatBusy(false);
    }
  }, [chat, chatInput, chatBusy]);

  const applyFilters = useCallback(async (s: string, r: string, e: string | null, b: number) => {
    if (!tauriReady) return;
    try {
      const res = await tauriInvoke<EngineSnapshot>("set_filters", { strategy: s, risk: r, expiry: e, budget: b });
      if (res) setSnap(res);
    } catch { /* browser preview mode */ }
  }, [tauriReady]);

  // Inside the desktop app we NEVER fall back to mock data — an empty table
  // with the scan status is honest; synthetic rows are browser-preview only.
  const isLive = snap !== null;
  const rows: EnrichedRow[] = isLive ? snap!.candidates : (hasTauri ? [] : (MOCK_ROWS as EnrichedRow[]));
  const stock: StockUpdate | null = snap?.stock ?? (hasTauri ? null : MOCK_STOCK);
  const stats: ChainStats | null = snap?.stats ?? (hasTauri ? null : MOCK_STATS);
  const selectedRow = rows.find(r => r.q.symbol === selected) ?? rows[0];
  const chg = stock?.change ?? 0;
  const chgPct = (stock?.change_ratio ?? 0) * 100;
  const chgCls = chg >= 0 ? "pos" : "neg";

  // expiry chips: "ทุกวัน" + real expiry dates from the loaded rows
  const [expirySel, setExpirySel] = useState<string>("ทุกวัน");
  const expiryOptions = useMemo(() => {
    const dates = Array.from(new Set(rows.map(r => r.q.expiry))).sort().slice(0, 6);
    return ["ทุกวัน", ...dates];
  }, [rows]);
  const pickExpiry = (e: string) => {
    setExpirySel(e);
    const iso = e === "ทุกวัน" ? null : e;
    applyFilters(strategy, risk, iso, parseFloat(budget) || 0);
  };

  // derived metric values
  const pcOi = useMemo(() => {
    const p = rows.filter(r => r.q.option_type === "P").reduce((s, r) => s + (r.q.oi || 0), 0);
    const c = rows.filter(r => r.q.option_type === "C").reduce((s, r) => s + (r.q.oi || 0), 0);
    return c > 0 ? p / c : null;
  }, [rows]);
  const closest = useMemo(
    () => (rows.length ? rows.reduce((a, b) => ((a.dte ?? 1e9) <= (b.dte ?? 1e9) ? a : b)) : null),
    [rows],
  );

  // top-3 opportunity cards from the screener output
  const opps = useMemo(() => {
    const cand = rows.filter(r => r.delta != null && r.credit != null);
    if (cand.length === 0) return null;
    const byAbsDelta = [...cand].sort((a, b) => Math.abs(a.delta!) - Math.abs(b.delta!));
    const defensive = byAbsDelta[0];
    const balanced = byAbsDelta.find(r => Math.abs(r.delta!) >= 0.15 && Math.abs(r.delta!) <= 0.30)
      ?? byAbsDelta[Math.min(1, byAbsDelta.length - 1)];
    const topAnn = [...cand].sort((a, b) => (b.ann ?? 0) - (a.ann ?? 0))[0];
    const fc = (r: EnrichedRow) => `${symbol} ${r.q.strike}${r.q.option_type} · ${r.q.expiry.slice(5).replace("-", "/")}`;
    const fm = (r: EnrichedRow) => `+$${r.credit?.toFixed(0)} premium · ${(r.ann ?? 0).toFixed(1)}% annualized · δ ${Math.abs(r.delta ?? 0).toFixed(2)}`;
    return [
      { label: "LOWEST DELTA", tag: "Defensive", contract: fc(defensive), meta: fm(defensive) },
      { label: "BALANCED SETUP", tag: "Balanced", contract: fc(balanced), meta: fm(balanced) },
      { label: "HIGHEST ANNUALIZED", tag: "High yield", contract: fc(topAnn), meta: fm(topAnn) },
    ];
  }, [rows, symbol]);
  const oppCards = opps ?? (hasTauri
    ? [
        { label: "LOWEST DELTA", tag: "—", contract: "รอข้อมูลจริง", meta: " " },
        { label: "BALANCED SETUP", tag: "—", contract: "รอข้อมูลจริง", meta: " " },
        { label: "HIGHEST ANNUALIZED", tag: "—", contract: "รอข้อมูลจริง", meta: " " },
      ]
    : [
        { label: "LOWEST DELTA", tag: "Defensive", contract: `${symbol} 165P · 12 ก.พ.`, meta: `+$220 premium · ${fmt(15.0, 1)}% annualized · δ 0.10` },
        { label: "BALANCED SETUP", tag: "Balanced", contract: `${symbol} 175P · 12 ก.พ.`, meta: `+$415 premium · ${fmt(26.0, 1)}% annualized · δ 0.18` },
        { label: "HIGHEST ANNUALIZED", tag: "High yield", contract: `${symbol} 178P · 04 ธ.ค.`, meta: `+$285 premium · ${fmt(35.0, 1)}% annualized · δ 0.28` },
      ]);

  const feed = !tauriReady
    ? { t: "DEMO", cls: "warn" }
    : scanPhase === "error" ? { t: "ERROR", cls: "neg" }
    : scanPhase === "live" ? { t: "REST · LIVE", cls: "pos" }
    : ["discovery", "seed", "bars"].includes(scanPhase) ? { t: "SYNCING", cls: "warn" }
    : { t: "IDLE", cls: "" };

  const isError = scanPhase === "error";
  const badge = !hasTauri
    ? { text: "● BROWSER PREVIEW", cls: "" }
    : isLive ? { text: "● LIVE DATA", cls: "live" }
    : isError ? { text: "● FEED ERROR", cls: "err" }
    : { text: "● กำลังโหลด", cls: "wait" };

  if (showSetup) {
    return (
      <SetupScreen
        mode={setupMode}
        onComplete={() => setShowSetup(false)}
        onSkip={() => setShowSetup(false)}
      />
    );
  }

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
            onKeyDown={e => {
              if (e.key === "Enter") {
                const q = searchText.trim().toUpperCase();
                if (q) setSymbol(q);
              }
            }}
            placeholder={`Search (${symbol})`}
            aria-label="Search symbol"
          />
        </label>
        <span className={`tag-demo ${badge.cls}`}>{badge.text}</span>
      </header>

      {/* ============ WORKSPACE ============ */}
      <div className="workspace">
        {/* ---- Icon Rail ---- */}
        <nav className="iconbar">
          <button className="iconbtn" title="Home">⌂</button>
          <button className="iconbtn" title="Market">◔</button>
          <button className="iconbtn on" title="Options">|||</button>
          <button className="iconbtn" title="Watchlist">≡</button>
          <button
            className="iconbar bottom iconbtn"
            title="Settings / API keys"
            onClick={() => { setSetupMode("edit"); setShowSetup(true); }}
          >⚙</button>
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
                <button key={s} className={`toggle ${strategy === s ? "on" : ""}`} onClick={() => { setStrategy(s); applyFilters(s, risk, null, parseFloat(budget) || 0); }}>
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
                <button key={v} className={`choice ${risk === v ? "on" : ""}`} onClick={() => { setRisk(v); applyFilters(strategy, v, null, parseFloat(budget) || 0); }}>
                  <span>{label}</span>
                  <small>{meta}</small>
                </button>
              ))}
            </div>
          </div>

          <div className="filtergroup">
            <div className="filterlabel">Expiration</div>
            <div className="expiries">
              {expiryOptions.map(e => (
                <button key={e} className={`exp ${expirySel === e ? "on" : ""}`} onClick={() => pickExpiry(e)}>
                  {e === "ทุกวัน" ? e : e.slice(5).replace("-", "/")}
                </button>
              ))}
            </div>
          </div>

          <div className="filtergroup">
            <div className="filterlabel">Available cash (USD)</div>
            <label className="cashfield">
              <span>$</span>
              <input
                value={budget}
                onChange={e => setBudget(e.target.value)}
                onBlur={() => applyFilters(strategy, risk, expirySel === "ทุกวัน" ? null : expirySel, parseFloat(budget) || 0)}
                onKeyDown={e => { if (e.key === "Enter") applyFilters(strategy, risk, expirySel === "ทุกวัน" ? null : expirySel, parseFloat(budget) || 0); }}
                type="number"
                min="0"
                step="500"
              />
            </label>
          </div>

          <div className="leftfoot">
            <div><b>DATA MODE</b> <span style={{ float: "right", color: isLive ? "#3ddc97" : isError ? "#f4788c" : "#e0b15f" }}>{isLive ? "Live · Webull TH" : isError ? "Feed error" : hasTauri ? "รอข้อมูล" : "Synthetic"}</span></div>
            <div>{isLive ? `Feed: REST snapshot · ${new Date().toLocaleDateString()}` : "REST snapshot · 3 นาที/รอบ"}</div>
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
              <div style={{ fontSize: 10, color: "#8797aa", marginTop: 4 }}>US OPTIONS · Webull TH feed</div>
            </div>
            <div>
              <div className={`bigprice num ${stock ? chgCls : ""}`}>
                {stock ? fmt(stock.price) : "--"}
                {stock && <small className={chgCls}>{chg >= 0 ? "+" : ""}{chgPct.toFixed(2)}%</small>}
              </div>
              <div className="quotefoot num">
                {stock
                  ? <>{chg >= 0 ? "+" : ""}{fmt(chg)} ({chg >= 0 ? "+" : ""}{chgPct.toFixed(2)}%) · USD</>
                  : "รอราคาจาก feed ..."}
              </div>
            </div>
            <div className="head-quote-stats">
              {(stock
                ? [["Open", stock.open], ["High", stock.high], ["Low", stock.low], ["Prev", stock.pre_close]]
                : [["Open", null], ["High", null], ["Low", null], ["Prev", null]]
              ).map(([k, v]) => (
                <div key={k as string} className="qstat">
                  <div className="qk">{k}</div>
                  <div className="qv num">{v == null ? "--" : fmt(v as number)}</div>
                </div>
              ))}
            </div>
          </section>

          {/* 6 Metric Cards */}
          <section className="summary">
            {[
              { label: "IV (ATM)", value: stats?.atm_iv != null ? `${(stats.atm_iv * 100).toFixed(1)}%` : "--", suffix: isLive ? "ATM IV" : "รอข้อมูล" },
              { label: "Expected move ±1σ", value: stats?.em != null ? `±$${fmt(stats.em)}` : "--", suffix: "to expiry", cls: "warn" },
              { label: "Put/Call skew", value: stats?.skew != null ? `${stats.skew >= 0 ? "+" : ""}${stats.skew.toFixed(1)} pts` : "--", suffix: isLive ? "OTM strikes" : "รอข้อมูล", cls: stats?.skew != null && stats.skew < 0 ? "neg" : "" },
              { label: "Put/Call OI", value: pcOi != null ? pcOi.toFixed(2) : "--", suffix: isLive ? "open interest" : "รอข้อมูล" },
              { label: "Closest expiry", value: closest ? closest.q.expiry.slice(5).replace("-", "/") : "--", suffix: closest ? `${Math.round(closest.dte)}d` : "", cls: "pos" },
              { label: "Contracts loaded", value: String(rows.length), suffix: isLive ? "live feed" : hasTauri ? "รอข้อมูล" : "model-generated" },
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
            {oppCards.map((o, i) => (
              <button key={o.label} className={`opportunity ${i === 1 ? "on" : ""} ${opps ? "" : "muted-card"}`}>
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
              <span>{hasTauri ? scanMsg : "Funnel: OTM → delta → DTE → spread → premium → budget"}</span>
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
                  {(() => {
                    const visible = rows.filter(r => tab === "all" || (tab === "put" ? r.q.option_type === "P" : r.q.option_type === "C"));
                    if (visible.length === 0 && hasTauri) {
                      return (
                        <tr className="empty-row">
                          <td colSpan={13} style={{ textAlign: "center", padding: "34px 16px" }}>
                            {isError ? (
                              <div>
                                <div style={{ color: "#f4788c", fontSize: 12, marginBottom: 12 }}>⚠ {scanMsg}</div>
                                <button
                                  onClick={() => setShowSetup(true)}
                                  style={{
                                    background: "#2b4a75", color: "#dfe9f7", border: "1px solid #4a91fa",
                                    borderRadius: 6, padding: "7px 18px", fontSize: 11.5, fontWeight: 650, cursor: "pointer",
                                  }}
                                >
                                  เปิดหน้า Setup แก้ไข API keys
                                </button>
                              </div>
                            ) : scanPhase === "live" ? (
                              <span style={{ color: "#8797aa", fontSize: 11.5 }}>
                                ไม่พบ contracts ที่ผ่าน filters — ลองเพิ่ม Budget, เลือก All contracts หรือเปลี่ยน Expiration
                              </span>
                            ) : (
                              <span style={{ color: "#8797aa", fontSize: 11.5 }}>{scanMsg}</span>
                            )}
                          </td>
                        </tr>
                      );
                    }
                    return visible.map(r => (
                      <tr
                        key={r.q.symbol}
                        className={r.q.symbol === selectedRow?.q.symbol ? "selected" : ""}
                        onClick={() => setSelected(r.q.symbol)}
                      >
                        <td><GradeBadge grade={r.grade} /></td>
                        <td style={{ color: r.q.option_type === "P" ? "#f4788c" : "#24c99a", fontWeight: 650 }}>
                          {r.q.option_type === "P" ? "Put" : "Call"}
                        </td>
                        <td>{r.q.expiry.slice(5).replace("-", "/")} <small style={{ color: "#6b7d98" }}>({Math.round(r.dte)}d)</small></td>
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
                    ));
                  })()}
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
              {selectedRow ? (
                <>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
                    <div>
                      <h2>
                        {selectedRow.q.option_type === "P" ? "ขาย Put · CSP" : "ขาย Call · CC"}{" "}
                        {symbol} ${selectedRow.q.strike}
                      </h2>
                      <p>หมดอายุ {selectedRow.q.expiry} ({Math.round(selectedRow.dte)} วัน) · {selectedRow.grade ?? "--"}</p>
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
                    {selectedRow.iv != null
                      ? `IV ${((selectedRow.iv) * 100).toFixed(0)}% · กันชน ${selectedRow.cushion?.toFixed(1)}% · โอกาสถูกแตะ ${((selectedRow.p_touch ?? 0) * 100).toFixed(0)}%`
                      : `กันชน ${selectedRow.cushion?.toFixed(1)}% จากราคาปัจจุบัน`}
                  </div>
                </>
              ) : (
                <div style={{ color: "#8797aa", fontSize: 11.5, padding: "20px 4px" }}>
                  {isError ? "ไม่มีข้อมูล — แก้ไข API keys ในหน้า Setup ก่อน" : "รอข้อมูลจาก feed ..."}
                </div>
              )}
            </div>
          </section>

          {/* Radar Assistant — real LLM chat, status independent of the feed */}
          <section className="copilot">
            <div className="panel-heading" style={{ minHeight: "34px" }}>
              <span style={{ color: "#94beff" }}>✦</span> Radar Assistant
              <span
                style={{
                  marginLeft: "auto",
                  fontSize: 9,
                  fontWeight: 400,
                  color: llmInfo ? "#3ddc97" : "#e0b15f",
                }}
              >
                {llmInfo ? `AI · ${llmInfo.model}` : "AI ยังไม่ตั้งค่า"}
              </span>
            </div>
            {!llmInfo ? (
              <div className="chatlog" style={{ display: "block", padding: "10px 12px" }}>
                <p style={{ fontSize: 10.5, color: "#9bacc7", lineHeight: 1.6, marginBottom: 10 }}>
                  ยังไม่ได้ตั้งค่า AI assistant — ใส่ LLM API key (Z.ai / OpenAI / Ollama)
                  ในหน้า Setup ได้เลย ไม่เกี่ยวกับสถานะ feed ข้อมูล
                </p>
                <button
                  onClick={() => { setSetupMode("edit"); setShowSetup(true); }}
                  style={{
                    background: "#2b4a75", color: "#dfe9f7", border: "1px solid #4a91fa",
                    borderRadius: 6, padding: "6px 14px", fontSize: 10.5, fontWeight: 650, cursor: "pointer",
                  }}
                >
                  เปิดหน้า Setup
                </button>
              </div>
            ) : (
              <>
                {!isLive && (
                  <div style={{ fontSize: 9.5, color: "#e0b15f", padding: "6px 12px 0" }}>
                    ⚠ feed ข้อมูลยังไม่ live — AI จะบอกว่าไม่มีข้อมูลจนกว่า scan สำเร็จ
                  </div>
                )}
                <div className="chatlog">
                  {chat.length === 0 && (
                    <div className="bubble assistant">
                      <Markdown remarkPlugins={[remarkGfm]}>
                        {`ถามเกี่ยวกับสัญญาในตารางได้เลย เช่น "มีเงิน $18,000 ตัวไหนน่าทำ?"
                        หรือ "NVDA 175P มีความเสี่ยงยังไง" — คำตอบอ้างอิงข้อมูลจริงเท่านั้น`}
                      </Markdown>
                    </div>
                  )}
                  {chat.map((m, i) => (
                    <div key={i} className={`bubble ${m.role}`}>
                      {m.role === "assistant"
                        ? <Markdown remarkPlugins={[remarkGfm]}>{m.content}</Markdown>
                        : m.content}
                    </div>
                  ))}
                  {chatBusy && (
                    <div className="bubble assistant">AI กำลังดึงข้อมูลและวิเคราะห์ ...</div>
                  )}
                  {chatError && (
                    <div className="bubble assistant" style={{ color: "#f4788c" }}>⚠ {chatError}</div>
                  )}
                </div>
                <div className="chatinput">
                  <input
                    value={chatInput}
                    onChange={(e) => setChatInput(e.target.value)}
                    onKeyDown={(e) => { if (e.key === "Enter") sendChat(); }}
                    placeholder={isLive ? "ถามเกี่ยวกับสัญญาในตาราง..." : "ถามได้เลย (จะตอบเมื่อมีข้อมูล)..."}
                  />
                  <button aria-label="Send" onClick={sendChat} disabled={chatBusy}>➤</button>
                </div>
              </>
            )}
          </section>
        </aside>
      </div>

      {/* ============ STATUS BAR ============ */}
      <footer className="statusbar">
        <span><b>{tauriReady ? (isLive ? "LIVE · WEBULL TH FEED" : scanMsg) : "OFFLINE · BROWSER PREVIEW"}</b></span>
        <span>{symbol} {stock ? fmt(stock.price) : "--"} <b className={stock ? chgCls : ""}>{stock ? `${chg >= 0 ? "+" : ""}${chgPct.toFixed(2)}%` : ""}</b></span>
        <span>{isLive ? "REST snapshot · รอบถัดไป 3 นาที" : "REST snapshot feed"}</span>
        <span className="statusright">
          <span>Contracts: {rows.length}</span>
          <span>Feed: <b className={feed.cls}>{feed.t}</b></span>
          <span>PREMIUM RADAR · v0.1</span>
        </span>
      </footer>
    </div>
  );
}
