import { useState, useEffect, useCallback, useMemo } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { SetupScreen } from "./components/SetupScreen";
import OptionLab from "./components/OptionLab/OptionLab";
import PriceChart from "./components/PriceChart/PriceChart";
import PortfolioPage from "./components/PortfolioPage/PortfolioPage";
import WatchlistPage from "./components/WatchlistPage/WatchlistPage";
import HomePage from "./components/HomePage/HomePage";
import type { EnrichedRow, StockUpdate, EngineSnapshot } from "./types/screener";
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
  // (side-filter ใช้ strategy เดียวกับ FilterBar แล้ว)
  const [mainTab, setMainTab] = useState<"home" | "monitor" | "lab" | "portfolio" | "watchlist">("home");
  // watchlist จริงสำหรับ left panel (quotes + ชื่อลิสต์ มาจาก Rust ทีเดียว)
  const [wlQuotes, setWlQuotes] = useState<Array<{ symbol: string; price: number; change_pct: number | null }>>([]);
  const [wlTick, setWlTick] = useState(0); // bump เมื่อ Watchlists page แก้ข้อมล
  const [assistantOpen, setAssistantOpen] = useState(false);
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
          // real-time push liveness: the backend emits phase "realtime" once
          // the MQTT subscribe lands; any other real-time message (reconnecting,
          // dropped, no entitlement) means the push feed is down again.
          // (feed liveness สะท้อนผ่าน badge ใน header แล้ว)

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

  // left-panel watchlist: quotes ของลิสต์ active (Rust คำนวณ/จัด)
  useEffect(() => {
    let live = true;
    const load = () => {
      tauriInvoke<{ watchlist_name: string | null; quotes: Array<{ symbol: string; price: number; change_pct: number | null }> }>("get_home_overview")
        .then(o => { if (live) { setWlQuotes(o.quotes ?? []); } })
        .catch(() => { if (live) { setWlQuotes([]); } });
    };
    load();
    const t = window.setInterval(load, 60_000);
    return () => { live = false; window.clearInterval(t); };
  }, [wlTick]);

  // Inside the desktop app we NEVER fall back to mock data — an empty table
  // with the scan status is honest; synthetic rows are browser-preview only.
  const isLive = snap !== null;
  const rows: EnrichedRow[] = isLive ? snap!.candidates : (hasTauri ? [] : (MOCK_ROWS as EnrichedRow[]));
  const stock: StockUpdate | null = snap?.stock ?? (hasTauri ? null : MOCK_STOCK);
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

  const NAV_ITEMS: Array<[typeof mainTab, string, string]> = [
    ["home", "ภาพรวม", "◎"],
    ["monitor", "คัดสัญญา", "▤"],
    ["lab", "Option Lab", "⚗"],
    ["watchlist", "รายการติดตาม", "☰"],
    ["portfolio", "พอร์ตจริง", "▣"],
  ];
  const pageTitle = NAV_ITEMS.find(n => n[0] === mainTab)?.[1] ?? "ตั้งค่า";

  return (
    <div className="app">
      {/* ============ PRIMARY NAV (ทางเข้าเดียวต่อหน้า) ============ */}
      <nav className="primarynav">
        <div className="pn-brand">
          <div className="brand-icon">PR</div>
          <div>
            <div className="brand-text">PREMIUM RADAR</div>
            <div className="brand-sub">OPTIONS OPERATION SYSTEM</div>
          </div>
        </div>
        {NAV_ITEMS.map(([key, label, ico]) => (
          <button
            key={key}
            className={`pn-item ${mainTab === key ? "on" : ""}`}
            aria-current={mainTab === key ? "page" : undefined}
            onClick={() => setMainTab(key)}
          >
            <span className="pn-ico">{ico}</span>
            <span>{label}</span>
          </button>
        ))}
        <div className="pn-gap" />
        <button
          className={`pn-item ${showSetup ? "attention" : ""}`}
          onClick={() => { setSetupMode("edit"); setShowSetup(true); }}
        >
          <span className="pn-ico">⚙</span>
          <span>ตั้งค่า</span>
        </button>
      </nav>

      {/* ============ COLUMN ขวา: HEADER + PAGE ============ */}
      <div className="appcol">
      <header className="appheader">
        <div className="ah-title">{pageTitle}</div>
        <label className="instpicker" title="หุ้นที่ใช้วิเคราะห์ (ใช้ใน คัดสัญญา / Option Lab)">
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
            placeholder={`หุ้นที่วิเคราะห์ · ${symbol}`}
            aria-label="หุ้นที่ใช้วิเคราะห์"
            disabled={mainTab === "portfolio"}
          />
          {wlQuotes.length > 0 && (
            <div className="ip-suggest">
              {wlQuotes.filter(q => searchText && q.symbol.startsWith(searchText.trim().toUpperCase()) && q.symbol !== searchText.trim().toUpperCase()).slice(0, 5).map(q => (
                <button key={q.symbol} onClick={() => { setSymbol(q.symbol); setSearchText(""); }}>{q.symbol}</button>
              ))}
            </div>
          )}
        </label>
        <span className={`connbadge ${badge.cls}`}>{badge.text}</span>
        <button className="ah-assist" onClick={() => setAssistantOpen(o => !o)} aria-expanded={assistantOpen}>ผู้ช่วย</button>
      </header>

      {/* ============ PAGE CANVAS ============ */}
      <div className="workspace">
        {/* ---- Main ---- */}
        <main className="main">
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

          {mainTab === "home" ? (
            <section className="labmodule">
              <HomePage />
            </section>
          ) : mainTab === "watchlist" ? (
            <section className="labmodule">
              <WatchlistPage onChanged={() => setWlTick(t => t + 1)} />
            </section>
          ) : mainTab === "portfolio" ? (
            <section className="labmodule">
              <PortfolioPage />
            </section>
          ) : mainTab === "lab" ? (
            <section className="labmodule">
              <OptionLab row={selectedRow} symbol={symbol} spot={snap?.spot ?? 0} />
            </section>
          ) : (
          <>
          {/* กราฟราคาจริง (P15.4) — chart slot ตาม DESIGN.md */}
          <section className="pricechart-slot">
            <PriceChart symbol={symbol} />
          </section>
          {/* FilterBar — ชุดควบคุมกรองชุดเดียวของ Monitor */}
          <section className="filterbar">
            <div className="fb-group">
              <span className="fb-label">กลยุทธ์</span>
              <div className="toggle-row">
                {["CSP+CC", "CSP", "CC"].map(x => (
                  <button key={x} className={`toggle ${strategy === x ? "on" : ""}`} onClick={() => { setStrategy(x); applyFilters(x, risk, null, parseFloat(budget) || 0); }}>
                    {x === "CSP+CC" ? "ทั้งหมด" : x === "CSP" ? "Put · CSP" : "Call · CC"}
                  </button>
                ))}
              </div>
            </div>
            <div className="fb-group">
              <span className="fb-label">ช่วง Delta</span>
              <select className="fb-select" value={risk} onChange={e => { setRisk(e.target.value); applyFilters(strategy, e.target.value, null, parseFloat(budget) || 0); }}>
                <option value="ALL">ทุกสัญญา (delta ใดๆ)</option>
                <option value="SAFE">ต่ำ · δ .10–.20</option>
                <option value="BALANCED">กลาง · δ .15–.30</option>
                <option value="AGGR">สูง · δ .25–.40</option>
              </select>
            </div>
            <div className="fb-group">
              <span className="fb-label">วันหมดอายุ</span>
              <select className="fb-select" value={expirySel} onChange={e => pickExpiry(e.target.value)}>
                {expiryOptions.map(e => <option key={e} value={e}>{e}</option>)}
              </select>
            </div>
            <div className="fb-group">
              <span className="fb-label">เงินสด (USD)</span>
              <input
                className="fb-input"
                value={budget}
                onChange={e => setBudget(e.target.value)}
                onBlur={() => applyFilters(strategy, risk, expirySel === "ทุกวัน" ? null : expirySel, parseFloat(budget) || 0)}
                onKeyDown={e => { if (e.key === "Enter") applyFilters(strategy, risk, expirySel === "ทุกวัน" ? null : expirySel, parseFloat(budget) || 0); }}
                type="number" min="0" step="500"
              />
            </div>
            <span className="fb-hint">เพดานเงินสดใช้กับ CSP · CC ต้องมีหุ้นครบ 100 หุ้น</span>
          </section>

          {/* Scanner Table */}
          <section className="tablemodule">
            <div className="table-head">
              <span className="th-title">สัญญาที่ผ่านเกณฑ์ · {rows.length} รายการ</span>
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
                    <th>Assign</th><th>Credit $</th><th>Return</th><th>Ann%</th>
                    <th>Collateral $</th><th>B/E</th><th>OI</th><th>Source</th>
                  </tr>
                </thead>
                <tbody>
                  {(() => {
                    const visible = rows.filter(r => strategy === "CSP+CC" || (strategy === "CSP" ? r.q.option_type === "P" : r.q.option_type === "C"));
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
          </>
          )}
        </main>

        {/* ---- Right Panel: Inspector เฉพาะ Monitor ---- */}
        {mainTab === "monitor" && (
        <aside className="rightpanel">
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
                    <div className="ds"><label>Premium / 1 ct</label><strong className="pos">+${selectedRow.credit?.toFixed(0)}</strong></div>
                    <div className="ds"><label>Annualized</label><strong>{selectedRow.ann?.toFixed(1)}%</strong></div>
                    <div className="ds"><label>Collateral</label><strong>${selectedRow.collateral?.toLocaleString()}</strong></div>
                    <div className="ds"><label>Breakeven</label><strong>${selectedRow.be?.toFixed(2)}</strong></div>
                  </div>
                  <div style={{ fontSize: 9.5, color: "#9bacc7", marginBottom: 10 }}>
                    ราคา option ≈ ${selectedRow.prem?.toFixed(2)}/หุ้น × 100 หุ้น/contract
                    = ${selectedRow.credit?.toFixed(0)}/contract
                  </div>
                  {(() => {
                    const r = selectedRow;
                    const k = r.q.strike;
                    const prem = r.prem ?? 0;
                    const S = snap?.spot ?? 0;
                    if (!S || !k || !prem) return null;
                    const usd = (v: number) =>
                      `${v >= 0 ? "+" : "-"}$${Math.abs(Math.round(v)).toLocaleString()}`;
                    const credit = prem * 100;
                    const isPut = r.q.option_type === "P";
                    const be = r.be ?? (isPut ? k - prem : S - prem);
                    const scen = isPut
                      ? ([
                          [`ปิด ≥ ${k.toFixed(2)} — ไม่ถูกเรียก`, `${usd(credit)} เก็บ premium`, "pos"],
                          [`ปิด = ${be.toFixed(2)} — จุดคุ้มทุน (B/E)`, "$0", ""],
                          [`ปิด = ${(be - S * 0.05).toFixed(2)} (ต่ำกว่า B/E 5%)`, usd((be - S * 0.05 - be) * 100), "neg"],
                        ] as Array<[string, string, string]>)
                      : ([
                          [`ปิด ≥ ${k.toFixed(2)} — ถูกเรียก ขาย 100 หุ้น @${k.toFixed(2)}`, `${usd((k - S) * 100 + credit)} กำไรสูงสุด`, "pos"],
                          [`ปิด = ${be.toFixed(2)} — จุดคุ้มทุน (B/E)`, "$0", ""],
                          [`ปิด = ${(be - S * 0.05).toFixed(2)} (ต่ำกว่า B/E 5%)`, usd((be - S * 0.05 - S) * 100 + credit), "neg"],
                        ] as Array<[string, string, string]>);
                    return (
                      <>
                        {!isPut && r.if_called != null && (
                          <div style={{ fontSize: 10, background: "#12251f", border: "1px solid #1e5a47", borderRadius: 6, padding: "6px 8px", marginBottom: 8, color: "#9fd8c4" }}>
                            ถ้าถูกเรียก (if called): กำไรสูงสุด ≈ {usd(r.max_profit ?? (k - S + prem) * 100)} ·
                            ผลตอบแทน {r.if_called.toFixed(1)}% ({r.if_called_ann?.toFixed(0) ?? "--"}% ต่อปี)
                          </div>
                        )}
                        <div className="section-title">PAYOFF AT EXPIRY · ต่อ 1 CONTRACT (100 หุ้น)</div>
                        <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 10, marginBottom: 8 }}>
                          <tbody>
                            {scen.map(([l, v, cls], i) => (
                              <tr key={i} style={{ borderBottom: "1px solid #1e2733" }}>
                                <td style={{ padding: "3px 0", color: "#9bacc7" }}>{l}</td>
                                <td style={{
                                  padding: "3px 0", textAlign: "right", fontWeight: 650,
                                  color: cls === "pos" ? "#3ddc97" : cls === "neg" ? "#f4788c" : "#e9f1ff",
                                }}>{v}</td>
                              </tr>
                            ))}
                            {isPut && (
                              <tr>
                                <td colSpan={2} style={{ padding: "3px 0", color: "#8797aa", fontSize: 9.5 }}>
                                  ถ้าถูกเรียก (ปิด &lt; {k.toFixed(2)}): ซื้อ 100 หุ้น ที่ต้นทุนสุทธิ ${be.toFixed(2)}/หุ้น
                                </td>
                              </tr>
                            )}
                          </tbody>
                        </table>
                      </>
                    );
                  })()}
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

        </aside>
        )}

      </div>

      </div>{/* /appcol */}

      {/* ============ ASSISTANT DRAWER — chat จุดเดียว ใช้ได้ทุกหน้า ============ */}
      {assistantOpen && (
        <div className="assist-scrim" onClick={() => setAssistantOpen(false)}>
          <aside className="assist-drawer" onClick={e => e.stopPropagation()} role="dialog" aria-label="ผู้ช่วย AI">
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
      )}
    </div>
  );
}
