import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { SetupScreen } from "./components/SetupScreen";
import OptionLab from "./components/OptionLab/OptionLab";
import PriceChart from "./components/PriceChart/PriceChart";
import PortfolioPage from "./components/PortfolioPage/PortfolioPage";
import WatchlistPage from "./components/WatchlistPage/WatchlistPage";
import HomePage from "./components/HomePage/HomePage";
import MarketPage from "./components/MarketPage/MarketPage";
import QuoteHeader from "./components/QuoteHeader/QuoteHeader";
import ContractQuote from "./components/ContractQuote/ContractQuote";
import { LoadBar, Skel, SkelPanel, SkelRows } from "./components/shared/Skeleton";
import { useStockOverview } from "./lib/useStockOverview";
import { fmt, thaiDate } from "./lib/format";
import type { EnrichedRow, StockUpdate, ChainStats, EngineSnapshot } from "./types/screener";
import "./App.css";
import brandMark from "./assets/thpforge-mark.svg";

/** The pages of the icon rail — the app's single menu. */
type MainTab = "home" | "market" | "monitor" | "lab" | "portfolio" | "watchlist";

/** A watchlist row: only an available row carries numbers. */
interface WatchQuote {
  symbol: string;
  status?: "available" | "no_data" | "error" | "setup_required";
  price?: number | null;
  change_pct?: number | null;
}

const WATCH_STATUS_TEXT: Record<string, string> = {
  no_data: "ไม่มีราคา",
  error: "ดึงไม่สำเร็จ",
  setup_required: "ตั้งค่าก่อน",
};

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

/** A snapshot belongs to `sym` only when its stock root and every row's
 *  underlying match it (case-insensitive); one with neither stock nor rows
 *  is the neutral empty state and always passes. Late events from a scan
 *  that a symbol switch already replaced are dropped here. */
function snapshotIsFor(s: EngineSnapshot, sym: string): boolean {
  // OCC roots differ from tickers (BRK.B → BRKB, adjusted AAPL1, SPX → SPXW),
  // so rows match on a normalised prefix and only when the stock is absent.
  const norm = (v: string) => v.toUpperCase().replace(/[^A-Z0-9]/g, "");
  const want = norm(sym);
  if (s.stock) return norm(s.stock.symbol) === want;
  const all = [...s.rows, ...s.candidates];
  return all.length === 0 || all.some(r => norm(r.q.underlying).startsWith(want));
}

// ---------- mock data (browser preview ONLY — never inside the desktop app) ----------

const MOCK_STOCK: StockUpdate = {
  symbol: "NVDA",
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

// ---------- scanner table sorting (ordering only — no value is recomputed) ----------

type SortKey =
  | "grade" | "side" | "expiry" | "delta" | "assign" | "credit"
  | "ret" | "ann" | "collateral" | "be" | "oi";
type Sort = { key: SortKey; dir: 1 | -1 };

const GRADE_RANK: Record<string, number> = { A: 0, B: 1, C: 2 };

/** The sort value of a row per column; `null` always sinks to the end. */
function sortValue(row: EnrichedRow, key: SortKey): number | string | null {
  switch (key) {
    case "grade": return row.grade ? GRADE_RANK[row.grade] ?? 9 : null;
    case "side": return row.q.option_type;
    case "expiry": return row.q.expiry;
    case "delta": return row.delta;
    case "assign": return row.p_itm;
    case "credit": return row.credit;
    case "ret": return row.prem != null && row.q.strike ? row.prem / row.q.strike : null;
    case "ann": return row.ann;
    case "collateral": return row.collateral;
    case "be": return row.be;
    case "oi": return row.q.oi;
  }
}

/** Orders rows by the chosen column; rows without a value stay last. */
function sortRows(rows: EnrichedRow[], sort: Sort | null): EnrichedRow[] {
  if (!sort) return rows;
  const { key, dir } = sort;
  return [...rows].sort((a, b) => {
    const left = sortValue(a, key);
    const right = sortValue(b, key);
    if (left == null && right == null) return 0;
    if (left == null) return 1;
    if (right == null) return -1;
    if (left < right) return -dir;
    if (left > right) return dir;
    return 0;
  });
}

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

/** A sortable column header: click cycles ascending → descending → screener order. */
function SortTh({ label, sortKey, sort, onSort }: {
  label: string;
  sortKey: SortKey;
  sort: Sort | null;
  onSort: (key: SortKey) => void;
}) {
  const active = sort?.key === sortKey ? sort : null;
  return (
    <th aria-sort={active ? (active.dir === 1 ? "ascending" : "descending") : "none"}>
      <button
        type="button"
        className="th-sort"
        onClick={() => onSort(sortKey)}
        title={active ? "เรียงอยู่ — คลิกเพื่อสลับ/ยกเลิก" : "เรียงตามคอลัมน์นี้"}
      >
        {label}
        <i className={active ? "on" : ""}>{active ? (active.dir === 1 ? "▲" : "▼") : "⇅"}</i>
      </button>
    </th>
  );
}

// ---------- main app ----------

export default function App() {
  const [symbol, setSymbol] = useState("NVDA");
  const [strategy, setStrategy] = useState("CSP+CC");
  const [risk, setRisk] = useState("BALANCED");
  const [budget, setBudget] = useState("18000");
  const [tab, setTab] = useState("all");
  const [mainTab, setMainTab] = useState<MainTab>("home");
  // watchlist จริงสำหรับ left panel (quotes + ชื่อลิสต์ มาจาก Rust ทีเดียว · แถวที่ไม่มีราคาบอกเหตุผลแทนตัวเลข)
  const [wlQuotes, setWlQuotes] = useState<WatchQuote[]>([]);
  const [wlName, setWlName] = useState<string | null>(null);
  const [wlTick, setWlTick] = useState(0); // bump เมื่อ Watchlists page แก้ข้อมล
  const [selected, setSelected] = useState<string | null>(null);
  const [searchText, setSearchText] = useState("");
  const [sort, setSort] = useState<Sort | null>(null);
  const [tauriReady, setTauriReady] = useState(false);
  const [showSetup, setShowSetup] = useState(false);
  const [setupMode, setSetupMode] = useState<"first-run" | "edit">("first-run");

  // live data from the Rust scan worker
  const [snap, setSnap] = useState<EngineSnapshot | null>(null);
  const [scanMsg, setScanMsg] = useState("กำลังเชื่อมต่อ backend ...");
  const [scanPhase, setScanPhase] = useState("idle");
  const [wsLive, setWsLive] = useState(false);
  // the symbol every arriving snapshot must match (listeners outlive renders)
  const symbolRef = useRef(symbol);

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
          if (p.phase === "realtime") setWsLive(true);
          else if (p.message.includes("real-time")) setWsLive(false);
        }));
        uns.push(await tauriListen<EngineSnapshot>("scan://snapshot", (s) => {
          if (snapshotIsFor(s, symbolRef.current)) setSnap(s);
        }));

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

  // A symbol switch drops the previous chain immediately: rows, selection and
  // sort reset, and the scan status becomes a loading text naming the new
  // symbol — until the new scan's first event arrives, nothing of the old
  // symbol stays on screen. (Declared before the scan-restart effect so the
  // ref is current before `start_scan` fires.)
  useEffect(() => {
    symbolRef.current = symbol;
    setSnap(null);
    setSelected(null);
    setSort(null);
    setScanPhase("discovery");
    setScanMsg(`กำลังโหลด ${symbol} …`);
  }, [symbol]);

  // Start / restart the scan whenever the symbol changes (or setup completes);
  // opening Setup stops any running scan (it may be failing on bad keys).
  useEffect(() => {
    if (!tauriReady || showSetup) return;
    setExpirySel("ทุกวัน");
    setWsLive(false);
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
        // get_settings returns a redacted SettingsView: the LLM key arrives
        // only as has_api_key, never as a value
        const ready = !!llm
          && typeof llm.base_url === "string" && llm.base_url
          && typeof llm.model === "string" && llm.model
          && llm.has_api_key === true;
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
      if (res && snapshotIsFor(res, symbolRef.current)) setSnap(res);
    } catch { /* browser preview mode */ }
  }, [tauriReady]);

  // left-panel watchlist: quotes ของลิสต์ active (Rust คำนวณ/จัด — คำขอเดียวทั้งลิสต์)
  useEffect(() => {
    let live = true;
    const load = () => {
      tauriInvoke<{ watchlist_name: string | null; quotes: WatchQuote[] }>("get_home_overview")
        .then(o => { if (live) { setWlQuotes(o.quotes ?? []); setWlName(o.watchlist_name); } })
        .catch(() => { if (live) { setWlQuotes([]); } });
    };
    load();
    const t = window.setInterval(load, 60_000);
    return () => { live = false; window.clearInterval(t); };
  }, [wlTick]);

  // company facts (earnings window, dividends, analysts) for the header and the risk notes
  const overview = useStockOverview(symbol);
  const earningsWindow = overview.overview?.next_earnings ?? null;
  const nextExDivDate = overview.overview?.dividends?.upcoming[0]?.ex_div_date ?? null;

  // Inside the desktop app we NEVER fall back to mock data — an empty table
  // with the scan status is honest; synthetic rows are browser-preview only.
  const isLive = snap !== null;
  // browser preview shows the mock chain only for the mock's own symbol;
  // any other symbol gets the same honest empty state as the desktop app
  const rows: EnrichedRow[] = useMemo(
    () => snap?.candidates
      ?? (hasTauri ? [] : (MOCK_ROWS as EnrichedRow[]).filter(r => r.q.underlying.toUpperCase() === symbol.toUpperCase())),
    [snap, symbol],
  );
  const toggleSort = (key: SortKey) => setSort(current =>
    current?.key !== key ? { key, dir: 1 } : current.dir === 1 ? { key, dir: -1 } : null,
  );
  // The cheapest contract of the loaded chain (the engine's `rows` are every
  // enriched row, before any filter): when it costs more than the budget cap,
  // that alone empties the table, and the empty state can say so with numbers.
  const cheapestCollateral = useMemo(() => {
    const priced = (snap?.rows ?? []).map(r => r.collateral).filter((c): c is number => c != null && c > 0);
    return priced.length ? Math.min(...priced) : null;
  }, [snap]);
  const budgetCap = parseFloat(budget) || 0;
  // A stock snapshot from a different underlying (a scan raced by a symbol
  // switch) is not this symbol's price — show nothing rather than the old one.
  const sameSymbol = (s: StockUpdate) => s.symbol.toUpperCase() === symbol.toUpperCase();
  const engineStock = snap?.stock ?? null;
  const stock: StockUpdate | null =
    engineStock != null
      ? sameSymbol(engineStock)
        ? engineStock
        : null
      : hasTauri
        ? null
        : sameSymbol(MOCK_STOCK)
          ? MOCK_STOCK
          : null;
  const mockOn = !hasTauri && sameSymbol(MOCK_STOCK);
  const stats: ChainStats | null = snap?.stats ?? (mockOn ? MOCK_STATS : null);
  const selectedRow = rows.find(r => r.q.symbol === selected) ?? rows[0];
  const chg = stock?.change ?? null;
  const chgPct = stock?.change_ratio != null ? stock.change_ratio * 100 : null;
  const chgCls = chg == null || chg === 0 ? "" : chg > 0 ? "pos" : "neg";

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
  const oppCards = opps ?? (!mockOn
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
    : scanPhase === "realtime" ? { t: "LIVE · PUSH", cls: "pos" }
    : scanPhase === "live" ? { t: "REST · LIVE", cls: "pos" }
    : ["discovery", "seed", "bars"].includes(scanPhase) ? { t: "SYNCING", cls: "warn" }
    : { t: "IDLE", cls: "" };

  // the chain is loaded and the scan is keeping it fresh: an empty table is
  // now about the filters, not about data still on its way
  const scanSettled = scanPhase === "live" || scanPhase === "realtime";

  const isError = scanPhase === "error";
  // The symbol's data is on its way: a scan phase that is still filling the
  // chain, or no snapshot yet without an error. Desktop app only — the browser
  // preview always shows mock data and never skeletons.
  const loading =
    hasTauri &&
    (["discovery", "seed", "bars"].includes(scanPhase) || (snap == null && !isError));
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
          <img className="brand-icon" src={brandMark} alt="THPForge" />
          <div>
            <div className="brand-text">PREMIUM RADAR</div>
            <div className="brand-sub">OPTIONS OPERATION SYSTEM</div>
          </div>
        </div>
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
          <button className={`iconbtn ${mainTab === "home" ? "on" : ""}`} title="หน้าแรก" onClick={() => setMainTab("home")}>⌂</button>
          <button className={`iconbtn ${mainTab === "market" ? "on" : ""}`} title="Market · ภาพรวมตลาด (หุ้นขึ้นแรง ลงแรง ซื้อขายหนาแน่น และ sector)" onClick={() => setMainTab("market")}>◔</button>
          <button className={`iconbtn ${mainTab === "monitor" ? "on" : ""}`} title="Options Monitor" onClick={() => setMainTab("monitor")}>|||</button>
          <button className={`iconbtn ${mainTab === "lab" ? "on" : ""}`} title="Option Lab" onClick={() => setMainTab("lab")}>⚗</button>
          <button className={`iconbtn ${mainTab === "watchlist" ? "on" : ""}`} title="Watchlists" onClick={() => setMainTab("watchlist")}>≡</button>
          <button className={`iconbtn ${mainTab === "portfolio" ? "on" : ""}`} title="Portfolio · พอร์ตจริง (อ่านอย่างเดียว)" onClick={() => setMainTab("portfolio")}>◫</button>
          <button
            className="iconbar bottom iconbtn"
            title="Settings / API keys"
            onClick={() => { setSetupMode("edit"); setShowSetup(true); }}
          >⚙</button>
        </nav>

        {/* ---- Left Panel ---- */}
        <aside className="leftpanel">
          <div className="panel-heading">
            Watchlist
            <div className="end">
              <button className="tiny-icon" title="จัดการลิสต์เต็มรูปแบบ" onClick={() => setMainTab("watchlist")}>⤢</button>
            </div>
          </div>
          <div className="watch-header"><span>Symbol</span><span>Last / Chg%</span></div>
          <div className="watchlist">
            {wlQuotes.map(q => {
              const priced = q.status === "available" && q.price != null;
              const note = priced ? undefined : WATCH_STATUS_TEXT[q.status ?? ""] ?? "ไม่มีราคา";
              return (
                <button
                  key={q.symbol}
                  className={`watch-row ${q.symbol === symbol ? "on" : ""}`}
                  title={note ? `${q.symbol} — ${note} (ราคาจาก Webull ของ session ปกติ)` : undefined}
                  onClick={() => { setSymbol(q.symbol); setMainTab("monitor"); }}
                >
                  <b>{q.symbol}</b>
                  <span className={`num ${priced ? "" : "muted"}`}>
                    {priced ? q.price!.toFixed(2) : "--"}
                    {" "}
                    {priced
                      ? <small className={(q.change_pct ?? 0) >= 0 ? "pos" : "neg"}>
                          {q.change_pct != null ? `${q.change_pct >= 0 ? "+" : ""}${q.change_pct.toFixed(1)}%` : "--"}
                        </small>
                      : <small>{note}</small>}
                  </span>
                </button>
              );
            })}
            {wlQuotes.length === 0 && (
              <button className="watch-row" onClick={() => setMainTab("watchlist")}>
                <b>+ เพิ่มหุ้น</b>
                <span className="num muted">จัดลิสต์ →</span>
              </button>
            )}
          </div>
          <div className="watch-bottom" onClick={() => setMainTab("watchlist")}>
            {wlName ?? "Watchlist"} <span style={{ marginLeft: "auto" }}>{wlQuotes.length} ตัว</span>
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
            <div>{isLive ? `Feed: ${wsLive ? "📡 real-time push" : "REST snapshot"} · ${new Date().toLocaleDateString()}` : "REST snapshot · 3 นาที/รอบ"}</div>
          </div>
        </aside>

        {/* ---- Main ---- */}
        <main className="main">

          {/* Quote Header — scanner price/change + broker session, extended hours and key facts */}
          <QuoteHeader symbol={symbol} stock={stock} overview={overview} loading={loading} />
          {loading && <LoadBar symbol={symbol} message={scanMsg} />}

          {mainTab === "home" ? (
            <section className="labmodule">
              <HomePage
                onOpenRadar={sym => { setSymbol(sym.toUpperCase()); setMainTab("monitor"); }}
                goTab={t => setMainTab(t)}
              />
            </section>
          ) : mainTab === "market" ? (
            <section className="labmodule">
              <MarketPage onOpenRadar={sym => { setSymbol(sym.toUpperCase()); setMainTab("monitor"); }} />
            </section>
          ) : mainTab === "watchlist" ? (
            <section className="labmodule">
              <WatchlistPage onOpenRadar={sym => { setSymbol(sym.toUpperCase()); setMainTab("monitor"); }} onChanged={() => setWlTick(t => t + 1)} />
            </section>
          ) : mainTab === "portfolio" ? (
            <section className="labmodule">
              <PortfolioPage />
            </section>
          ) : mainTab === "lab" ? (
            <section className="labmodule">
              <OptionLab row={selectedRow} symbol={symbol} spot={snap?.spot ?? 0} loading={loading} />
            </section>
          ) : (
          <>
          {/* กราฟราคาจริง (P15.4) — chart slot ตาม DESIGN.md */}
          <section className="pricechart-slot">
            <PriceChart symbol={symbol} />
          </section>
          {/* 6 Metric Cards */}
          <section className="summary">
            {[
              { label: "IV (ATM)", value: stats?.atm_iv != null ? `${(stats.atm_iv * 100).toFixed(1)}%` : "--", missing: stats?.atm_iv == null, suffix: isLive ? "ATM IV" : "รอข้อมูล" },
              { label: "Expected move ±1σ", value: stats?.em != null ? `±$${fmt(stats.em)}` : "--", missing: stats?.em == null, suffix: "to expiry", cls: "warn" },
              { label: "Put/Call skew", value: stats?.skew != null ? `${stats.skew >= 0 ? "+" : ""}${stats.skew.toFixed(1)} pts` : "--", missing: stats?.skew == null, suffix: isLive ? "OTM strikes" : "รอข้อมูล", cls: stats?.skew != null && stats.skew < 0 ? "neg" : "" },
              { label: "Put/Call OI", value: pcOi != null ? pcOi.toFixed(2) : "--", missing: pcOi == null, suffix: isLive ? "open interest" : "รอข้อมูล" },
              { label: "Closest expiry", value: closest ? closest.q.expiry.slice(5).replace("-", "/") : "--", missing: closest == null, suffix: closest ? `${Math.round(closest.dte)}d` : "", cls: "pos" },
              { label: "Contracts loaded", value: String(rows.length), missing: rows.length === 0, suffix: isLive ? "live feed" : hasTauri ? "รอข้อมูล" : "model-generated" },
            ].map(m => (
              <div key={m.label} className="metric">
                <div className="metric-label">{m.label}</div>
                <div className={`metric-value ${m.cls ?? ""}`}>
                  {loading && m.missing
                    ? <Skel w="3.5em" />
                    : <>{m.value} <span className="metric-suffix">{m.suffix}</span></>}
                </div>
              </div>
            ))}
          </section>

          {/* 3 Opportunity Cards */}
          <div className="opportunities">
            {loading && !opps
              ? [0, 1, 2].map(i => (
                  <div key={i} className={`opportunity ${i === 1 ? "on" : ""}`} aria-hidden="true">
                    <div className="otop"><Skel w="55%" /></div>
                    <div className="ocontract"><Skel w="72%" /></div>
                    <div className="ometas"><Skel w="88%" /></div>
                  </div>
                ))
              : oppCards.map((o, i) => (
              <button key={o.label} className={`opportunity ${i === 1 ? "on" : ""} ${opps ? "" : "muted-card"}`} onClick={() => { const hit = rows.find(r => r.q.symbol === (o as { contract?: string }).contract); if (hit) setSelected(hit.q.symbol); }}>
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
                <button className="tool-pill" disabled title="มุม Option Chain ยังไม่เปิดใช้ — ใช้แท็บ Option Lab วิเคราะห์รายสัญญาแทน">Option Chain</button>
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
                    <SortTh label="Grade" sortKey="grade" sort={sort} onSort={toggleSort} />
                    <SortTh label="Side" sortKey="side" sort={sort} onSort={toggleSort} />
                    <SortTh label="Expiry" sortKey="expiry" sort={sort} onSort={toggleSort} />
                    <th>Strike</th>
                    <SortTh label="Delta" sortKey="delta" sort={sort} onSort={toggleSort} />
                    <SortTh label="Assign" sortKey="assign" sort={sort} onSort={toggleSort} />
                    <SortTh label="Credit $" sortKey="credit" sort={sort} onSort={toggleSort} />
                    <SortTh label="Return" sortKey="ret" sort={sort} onSort={toggleSort} />
                    <SortTh label="Ann%" sortKey="ann" sort={sort} onSort={toggleSort} />
                    <SortTh label="Collateral $" sortKey="collateral" sort={sort} onSort={toggleSort} />
                    <SortTh label="B/E" sortKey="be" sort={sort} onSort={toggleSort} />
                    <SortTh label="OI" sortKey="oi" sort={sort} onSort={toggleSort} />
                    <th>Source</th>
                  </tr>
                </thead>
                <tbody>
                  {(() => {
                    // client-side ordering of the screener's own values; nothing is recomputed
                    const visible = sortRows(
                      rows.filter(r => tab === "all" || (tab === "put" ? r.q.option_type === "P" : r.q.option_type === "C")),
                      sort,
                    );
                    // the chain is still arriving — placeholder rows stand in for it
                    if (loading && rows.length === 0) {
                      return <SkelRows rows={8} cols={13} />;
                    }
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
                            ) : scanSettled ? (
                              <span style={{ color: "#8797aa", fontSize: 11.5, lineHeight: 1.6 }}>
                                {rows.length > 0
                                  ? `ไม่มี ${tab === "put" ? "Put" : "Call"} ที่ผ่าน filters ในมุมมองนี้`
                                  : cheapestCollateral != null && budgetCap > 0 && cheapestCollateral > budgetCap
                                    ? <>ไม่พบ contracts ที่ผ่าน filters — Collateral ต่ำสุดของ {symbol} คือ ${fmt(cheapestCollateral, 0)} สูงกว่า Budget ${fmt(budgetCap, 0)}<br />เพิ่ม Budget (หรือใส่ 0 = ไม่จำกัด) แล้วตรวจ Strategy · Assignment risk · Expiration ด้วย</>
                                    : "ไม่พบ contracts ที่ผ่าน filters — ลองเพิ่ม Budget, เลือก All contracts หรือเปลี่ยน Expiration"}
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
                        <td>
                          <button
                            type="button"
                            className="contract-select"
                            aria-label={`Inspect ${r.q.option_type === "P" ? "put" : "call"} ${r.q.strike}, expires ${r.q.expiry}`}
                            aria-pressed={r.q.symbol === selectedRow?.q.symbol}
                            onClick={event => {
                              event.stopPropagation();
                              setSelected(r.q.symbol);
                            }}
                          >{r.q.strike}</button>
                        </td>
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
                    <div className="ds"><label>Premium / 1 ct</label><strong className="pos">+${selectedRow.credit?.toFixed(0)}</strong></div>
                    <div className="ds"><label>Annualized</label><strong>{selectedRow.ann?.toFixed(1)}%</strong></div>
                    <div className="ds"><label>Collateral</label><strong>${selectedRow.collateral?.toLocaleString()}</strong></div>
                    <div className="ds"><label>Breakeven</label><strong>${selectedRow.be?.toFixed(2)}</strong></div>
                  </div>
                  <div style={{ fontSize: 9.5, color: "#9bacc7", marginBottom: 10 }}>
                    ราคา option ≈ ${selectedRow.prem?.toFixed(2)}/หุ้น × 100 หุ้น/contract
                    = ${selectedRow.credit?.toFixed(0)}/contract
                  </div>

                  {/* calendar risks the seller plans around (dates only, no projection) */}
                  {earningsWindow && selectedRow.q.expiry >= earningsWindow.start_date && (
                    <div className="risk-flag" role="status">
                      ⚠ สัญญานี้ครบกำหนดหลังวันประกาศผลงาน
                      (Webull คาดช่วง {thaiDate(earningsWindow.start_date)}
                      {earningsWindow.end_date !== earningsWindow.start_date && ` ถึง ${thaiDate(earningsWindow.end_date)}`}
                      ) — ต้องถือข้ามวันประกาศผล ราคาหุ้นมักเคลื่อนแรงกว่าปกติ
                    </div>
                  )}
                  {nextExDivDate && nextExDivDate <= selectedRow.q.expiry && (
                    <div className="risk-flag" role="status">
                      ⚠ ก่อนครบกำหนด หุ้นเข้า Ex-dividend วันที่ {thaiDate(nextExDivDate)}
                      {selectedRow.q.option_type === "P"
                        ? " — ราคาหุ้นจะปรับลงประมาณมูลค่าปันผลในวันเข้า ex"
                        : " — Covered Call ยิ่งใกล้วัน ex ยิ่งมีโอกาสถูกเรียกคืนหุ้นก่อนกำหนด"}
                    </div>
                  )}

                  {/* what Webull itself quotes for this contract (display only) */}
                  <ContractQuote
                    symbol={selectedRow.q.symbol}
                    modelIv={selectedRow.iv}
                    modelDelta={selectedRow.delta}
                  />
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
              ) : loading ? (
                <SkelPanel lines={6} />
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
        <span>{symbol} {stock ? fmt(stock.price) : loading ? <Skel w="3.5em" /> : "--"} <b className={chgCls}>{chg != null && chgPct != null ? `${chg >= 0 ? "+" : ""}${chgPct.toFixed(2)}%` : loading ? <Skel w="2.5em" /> : "--"}</b></span>
        <span>{isLive ? (wsLive ? "📡 real-time push" : "REST snapshot · รอบถัดไป 3 นาที") : "REST snapshot feed"}</span>
        <span className="statusright">
          <span>Contracts: {rows.length}</span>
          <span>Feed: <b className={feed.cls}>{feed.t}</b></span>
          <span>PREMIUM RADAR · v0.1</span>
        </span>
      </footer>
    </div>
  );
}
