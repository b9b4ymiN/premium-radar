/** EasyApp — the Easy Mode screen state machine (CR-001 / W3).
 *
 *  onboarding 1–2 (money → risk) → home (assistant proposes ideas from the
 *  whole market) → opening a pick focuses that symbol and reuses the decision
 *  flow: ideas → five questions + what-if + ask → checklist → Webull field
 *  map / not-yet.
 *
 *  Data flow: App hands over the scan state (`symbol`/`setSymbol` drive the
 *  engine scan, `loading`/`scanMsg` report progress) and the current quote.
 *  Home renders `easy_universe_ideas` (no arguments; Rust reads the saved
 *  profile). Opening a pick sets the app symbol (which starts the normal
 *  scan), then calls `easy_ideas(symbol)`; while the engine has not finished
 *  scanning, Rust answers with the "…กำลังโหลด" error and this component
 *  retries calmly every 1.5 s (up to ~60 s) and immediately when the scan
 *  settles. Browser preview (no Tauri) renders the prototype's sample data.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EasyIdeas, EasyProfile, Stance, UniverseIdeas } from "../types/easy";
import { Onboarding } from "./Onboarding";
import { EasyHome, type HeldContract } from "./EasyHome";
import { IdeasStep } from "./IdeasStep";
import { FiveQuestions } from "./FiveQuestions";
import { Checklist } from "./Checklist";
import { WebullFieldMap, NotYet } from "./WebullFieldMap";
import { EzSprite } from "./icons";
import { daysUntil } from "./util";
import { SAMPLE_HELD, sampleIdeas, sampleProfile, sampleUniverse } from "./sample";
import { easyInvoke } from "./invoke";
import { AgentPanel } from "../agent/AgentPanel";
import { useScreenState } from "../agent/anchors";
import { registerUiHandler } from "../agent/bus";
import { clearPointing } from "../agent/point";
import { requestWhatIfPrice } from "../agent/requests";
import "./easy.css";

type Screen = "boot" | "ob1" | "ob2" | "home" | "s2" | "s3" | "s4" | "s5" | "notyet";

interface IdeasState {
  data: EasyIdeas | null;
  loading: boolean;
  error: string | null;
}

interface UniverseState {
  data: UniverseIdeas | null;
  loading: boolean;
  error: string | null;
}

/** The broker positions Easy Mode cares about (subset of `get_portfolio`). */
interface PfLeg {
  option_type: string | null;
  expire_date: string | null;
  exercise_price: number | null;
}
interface PfPos {
  symbol: string;
  instrument_type: string | null;
  quantity: number | null;
  legs: PfLeg[] | null;
}

/** Retry bookkeeping shared between the loader and its own timeouts. */
interface RetryBag {
  timers: Record<string, number>;
  tries: Record<string, number>;
}

/** Load one symbol's ideas, retrying the "กำลังโหลด" answer every 1.5 s. */
async function fetchIdeasOnce(
  sym: string,
  ctx: {
    hasTauri: boolean;
    getProfile: () => EasyProfile;
    apply: (updater: (prev: Record<string, IdeasState>) => Record<string, IdeasState>) => void;
    bag: RetryBag;
  },
): Promise<void> {
  const S = sym.toUpperCase();
  if (!ctx.hasTauri) {
    ctx.apply(prev => ({ ...prev, [S]: { data: sampleIdeas(S, ctx.getProfile()), loading: false, error: null } }));
    return;
  }
  window.clearTimeout(ctx.bag.timers[S]);
  ctx.apply(prev => ({ ...prev, [S]: { data: prev[S]?.data ?? null, loading: true, error: null } }));
  try {
    const data = await easyInvoke<EasyIdeas>("easy_ideas", { symbol: S });
    ctx.bag.tries[S] = 0;
    ctx.apply(prev => ({ ...prev, [S]: { data, loading: false, error: null } }));
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (msg.includes("กำลังโหลด") && (ctx.bag.tries[S] ?? 0) < 40) {
      ctx.bag.tries[S] = (ctx.bag.tries[S] ?? 0) + 1;
      ctx.bag.timers[S] = window.setTimeout(() => { void fetchIdeasOnce(S, ctx); }, 1500);
    } else {
      ctx.apply(prev => ({ ...prev, [S]: { data: prev[S]?.data ?? null, loading: false, error: msg } }));
    }
  }
}

export interface EasyAppProps {
  tauriReady: boolean;
  hasTauri: boolean;
  symbol: string;
  setSymbol: (s: string) => void;
  loading: boolean;
  scanMsg: string;
  stock: { symbol: string; price: number } | null;
  llmInfo: { label: string; model: string } | null;
  openSetup: () => void;
}

export function EasyApp({ tauriReady, hasTauri, symbol, setSymbol, loading, stock, llmInfo, openSetup }: EasyAppProps) {
  const browserPreview = !hasTauri;
  const [screen, setScreen] = useState<Screen>(() => (hasTauri ? "boot" : "ob1"));
  const [profile, setProfile] = useState<EasyProfile>(() => sampleProfile(false));
  const [profileLoaded, setProfileLoaded] = useState(false);
  const [ideasState, setIdeasState] = useState<Record<string, IdeasState>>({});
  const [universe, setUniverse] = useState<UniverseState>({ data: null, loading: false, error: null });
  const [focused, setFocused] = useState<string | null>(null);
  const [pickedOcc, setPickedOcc] = useState<string | null>(null);
  const [positions, setPositions] = useState<PfPos[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const mainRef = useRef<HTMLDivElement | null>(null);
  const toastTimer = useRef(0);
  const profileRef = useRef(profile);
  const ideasStateRef = useRef(ideasState);
  const universeRef = useRef(universe);
  const saveChain = useRef<Promise<void>>(Promise.resolve());
  const bag = useRef<RetryBag>({ timers: {}, tries: {} });
  // mirrors for the agent ui handlers (registered once, must read fresh state)
  const focusedRef = useRef(focused);
  const pickedOccRef = useRef(pickedOcc);
  const openPickRef = useRef<(sym: string) => void>(() => {});

  // mirrors for async callbacks (never written during render)
  useEffect(() => { profileRef.current = profile; }, [profile]);
  useEffect(() => { ideasStateRef.current = ideasState; }, [ideasState]);
  useEffect(() => { universeRef.current = universe; }, [universe]);
  useEffect(() => { focusedRef.current = focused; }, [focused]);
  useEffect(() => { pickedOccRef.current = pickedOcc; }, [pickedOcc]);

  const showToast = useCallback((msg: string) => {
    setToast(msg);
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), 3200);
  }, []);

  const fetchIdeas = useCallback((sym: string) =>
    fetchIdeasOnce(sym, {
      hasTauri,
      getProfile: () => profileRef.current,
      apply: setIdeasState,
      bag: bag.current,
    }), [hasTauri]);

  // ---------- the assistant's whole-market picks ----------
  const fetchUniverse = useCallback(async () => {
    if (!hasTauri) {
      setUniverse({ data: sampleUniverse(profileRef.current), loading: false, error: null });
      return;
    }
    setUniverse(prev => ({ data: prev.data, loading: true, error: null }));
    try {
      await saveChain.current; // Rust ranks from the SAVED profile
      const data = await easyInvoke<UniverseIdeas>("easy_universe_ideas");
      setUniverse({ data, loading: false, error: null });
    } catch (e) {
      setUniverse(prev => ({ data: prev.data, loading: false, error: e instanceof Error ? e.message : String(e) }));
    }
  }, [hasTauri]);

  useEffect(() => () => {
    Object.values(bag.current.timers).forEach(t => window.clearTimeout(t));
    window.clearTimeout(toastTimer.current);
  }, []);

  // ---------- bootstrap: settings (profile) and portfolio ----------
  useEffect(() => {
    if (!tauriReady || !hasTauri) return;
    let live = true;
    (async () => {
      try {
        const st = await easyInvoke<Record<string, unknown>>("get_settings");
        if (!live) return;
        const ep = st?.easy_profile as Partial<EasyProfile> | null | undefined;
        // an empty stock list is normal now — the answers are given on the
        // home idea cards, so cash/risk/onboarded still count as a profile
        if (ep && Array.isArray(ep.stocks)) {
          setProfile({
            onboarded: !!ep.onboarded,
            stocks: ep.stocks.map(s => ({ symbol: String(s.symbol).toUpperCase(), stance: (s.stance === "avoid" ? "avoid" : "want") as Stance })),
            cash: typeof ep.cash === "number" && ep.cash > 0 ? ep.cash : 18_000,
            risk: ep.risk === 2 || ep.risk === 3 ? ep.risk : 1,
          });
          setScreen(ep.onboarded ? "home" : "ob1");
        } else {
          setScreen("ob1");
        }
      } catch {
        setScreen("ob1");
      } finally {
        if (live) setProfileLoaded(true);
      }
      try {
        const pf = await easyInvoke<{ positions?: PfPos[] | null }>("get_portfolio");
        if (live) setPositions(pf?.positions ?? []);
      } catch {
        if (live) setPositions([]); // friendly empty state
      }
    })();
    return () => { live = false; };
  }, [tauriReady, hasTauri]);

  // persist profile edits (skip the bootstrap value itself)
  useEffect(() => {
    if (!profileLoaded || !hasTauri) return;
    saveChain.current = easyInvoke("save_easy_profile", { profile }).then(() => undefined, () => { /* keep the session going */ });
  }, [profile, profileLoaded, hasTauri]);

  // entering home loads the universe picks; profile cash/risk edits change
  // what the agent should propose, so once the cached picks were computed for
  // a different cash/risk they are refetched (stance answers do not)
  const uniSig = `${profile.cash}|${profile.risk}`;
  const uniFetchedSig = useRef<string | null>(null);
  useEffect(() => {
    if (screen !== "home") return;
    const stale = uniFetchedSig.current !== null && uniFetchedSig.current !== uniSig;
    const st = universeRef.current;
    if ((!st.data || stale) && !st.loading) void fetchUniverse();
    uniFetchedSig.current = uniSig;
  }, [screen, uniSig, fetchUniverse]);

  // point the engine at the assistant's top pick while on home, so opening it
  // usually finds the scan already done (same pre-warm the old home did)
  useEffect(() => {
    if (screen !== "home") return;
    const top = universeRef.current.data?.picks[0]?.idea.symbol.toUpperCase();
    if (top && symbol.toUpperCase() !== top) setSymbol(top);
  }, [screen, symbol, setSymbol, universe]);

  // the scan settled: retry any ideas still waiting on the engine, now
  const prevLoading = useRef(loading);
  useEffect(() => {
    if (!hasTauri) return;
    if (prevLoading.current && !loading) {
      Object.entries(ideasStateRef.current).forEach(([S, st]) => {
        if (st.loading) void fetchIdeas(S);
      });
    }
    prevLoading.current = loading;
  }, [loading, hasTauri, fetchIdeas]);

  // new screen: back to the top, focus the heading (as the prototype does);
  // whatever the agent was pointing at left the screen with the old one
  useEffect(() => {
    clearPointing();
    mainRef.current?.scrollTo({ top: 0 });
    const h = mainRef.current?.querySelector<HTMLElement>(".ez-body h1, .ez-body .ez-say");
    if (h) {
      h.setAttribute("tabindex", "-1");
      h.focus({ preventScroll: true });
    }
  }, [screen]);

  // ---------- held contracts (portfolio) ----------
  const held = useMemo<HeldContract[]>(() => {
    if (browserPreview) return [SAMPLE_HELD];
    return (positions ?? [])
      .filter(pos => (pos.instrument_type ?? "").toLowerCase().includes("option"))
      .filter(pos => pos.quantity == null || pos.quantity < 0)
      .map(pos => {
        const leg = (pos.legs ?? []).find(l => (l.option_type ?? "").toUpperCase() === "P") ?? (pos.legs ?? [])[0];
        if (!leg?.exercise_price || !leg.expire_date) return null;
        return {
          symbol: pos.symbol,
          strike: leg.exercise_price,
          expiry: leg.expire_date,
          daysLeft: daysUntil(leg.expire_date),
          credit: null,
          spot: stock && stock.symbol.toUpperCase() === pos.symbol.toUpperCase() ? stock.price : null,
          avoid: !profile.stocks.some(s => s.symbol.toUpperCase() === pos.symbol.toUpperCase() && s.stance === "want"),
        } as HeldContract;
      })
      .filter((h): h is HeldContract => h != null);
  }, [browserPreview, positions, stock, profile]);

  // ---------- navigation ----------
  const focusedState = focused
    ? (ideasState[focused] ?? { data: null, loading: false, error: null })
    : { data: null, loading: false, error: null };
  const pickedIdea = focusedState.data?.ideas.find(i => i.occ === pickedOcc) ?? null;

  const go = (s: Screen) => setScreen(s);
  const patchProfile = (patch: Partial<EasyProfile>) => setProfile(cur => ({ ...cur, ...patch }));
  const retryUniverse = () => { void fetchUniverse(); };
  const retryFocused = () => {
    if (!focused) return;
    bag.current.tries[focused] = 0;
    void fetchIdeas(focused);
  };

  const finishOnboarding = () => {
    setProfile(cur => ({ ...cur, onboarded: true }));
    go("home");
  };

  const openPick = (sym: string) => {
    const S = sym.toUpperCase();
    setFocused(S);
    setPickedOcc(null);
    if (symbol.toUpperCase() !== S) setSymbol(S);
    bag.current.tries[S] = 0;
    void fetchIdeas(S);
    go("s2");
  };
  useEffect(() => { openPickRef.current = openPick; });

  // ---------- agent ui wiring (easy) ----------
  // the agent chat and its handlers exist only once onboarding is done; pro
  // mode unmounts this component and registers its own handlers instead
  const postOnboarding = screen !== "boot" && screen !== "ob1" && screen !== "ob2";
  useScreenState({ mode: "easy", page: screen, symbol: focused, selected: pickedOcc });

  useEffect(() => {
    if (!postOnboarding) return;
    const NOT_ON_SCREEN = "ยังไม่มีไอเดียนี้บนจอ — เปิดรายการไอเดียของหุ้นนั้นก่อน";
    const ideaLoaded = (ref: string) => {
      const f = focusedRef.current;
      const ideas = f ? ideasStateRef.current[f]?.data?.ideas : undefined;
      return ideas?.some(i => i.occ === ref) ?? false;
    };
    const unGo = registerUiHandler("go", a => {
      if (a.mode !== "easy") return { ok: false, detail: "คำสั่งนี้เป็นของโหมดละเอียด — สลับไปโหมดละเอียดก่อน" };
      if (a.page === "home") {
        setScreen("home");
        return { ok: true };
      }
      if (a.page === "s2") {
        if (!a.symbol) return { ok: false, detail: "ต้องบอกชื่อหุ้นด้วย เช่น NVDA" };
        openPickRef.current(a.symbol);
        return { ok: true };
      }
      if (a.page === "s3" || a.page === "s4") {
        const ref = a.ref ?? pickedOccRef.current;
        if (!ref) return { ok: false, detail: "ยังไม่ได้เลือกสัญญา — เปิดรายการไอเดียของหุ้นก่อน" };
        if (!ideaLoaded(ref)) return { ok: false, detail: NOT_ON_SCREEN };
        setPickedOcc(ref);
        setScreen(a.page);
        return { ok: true };
      }
      return { ok: false, detail: `ไม่มีหน้า "${a.page}" ในโหมดง่าย — มี home, s2, s3, s4` };
    });
    const unSelect = registerUiHandler("select", a => {
      if (!ideaLoaded(a.ref)) return { ok: false, detail: NOT_ON_SCREEN };
      setPickedOcc(a.ref);
      setScreen("s3");
      if (a.open === "what_if" && a.price != null) requestWhatIfPrice({ ref: a.ref, price: a.price });
      return { ok: true };
    });
    return () => { unGo(); unSelect(); };
  }, [postOnboarding]);

  // the stance answers given on the home idea cards — upsert, never hide the card
  const onStance = (sym: string, stance: Stance) => {
    const S = sym.toUpperCase();
    setProfile(cur => {
      const has = cur.stocks.some(s => s.symbol.toUpperCase() === S);
      return {
        ...cur,
        stocks: has
          ? cur.stocks.map(s => (s.symbol.toUpperCase() === S ? { ...s, stance } : s))
          : [...cur.stocks, { symbol: S, stance }],
      };
    });
  };

  // ---------- flow bar ----------
  const flow = (() => {
    if (screen === "ob1") return { total: 2, step: 1, label: "ตั้งค่า 1 จาก 2" };
    if (screen === "ob2") return { total: 2, step: 2, label: "ตั้งค่า 2 จาก 2" };
    const dec: Partial<Record<Screen, number>> = { s2: 1, s3: 2, s4: 3, s5: 3 };
    const step = dec[screen];
    return step ? { total: 3, step, label: `ขั้น ${step} จาก 3` } : null;
  })();

  return (
    <div className="ez-main" ref={mainRef}>
      <EzSprite />
      {flow && (
        <div className="ez-flowbar" aria-hidden="true">
          {[0, 1, 2, 3].map(i => (
            <span key={i} className={`ez-seg ${i < flow.step ? "done" : ""}`} style={{ display: i < flow.total ? undefined : "none" }} />
          ))}
          <span className="ez-count">{flow.label}</span>
        </div>
      )}

      <div className="ez-body">
        {screen === "boot" && (
          <div className="ez-panel ez-loading" style={{ marginTop: 80 }} role="status">
            <span className="ez-spin" />
            <span>กำลังเปิดโหมดง่าย…</span>
          </div>
        )}

        {(screen === "ob1" || screen === "ob2") && (
          <Onboarding
            step={screen === "ob1" ? 1 : 2}
            profile={profile}
            onPatch={patchProfile}
            onGo={s => (s === "home" ? finishOnboarding() : go(s))}
          />
        )}

        {screen === "home" && (
          <EasyHome
            profile={profile}
            universe={universe.data}
            universeError={universe.error}
            held={held}
            browserPreview={browserPreview}
            onRetry={retryUniverse}
            onOpenPick={openPick}
            onStance={onStance}
            onGo={s => go(s)}
          />
        )}

        {screen === "s2" && (
          <IdeasStep
            ideas={focusedState.data}
            ideasBusy={focusedState.loading}
            ideasError={focusedState.error}
            browserPreview={browserPreview}
            onRetry={retryFocused}
            onBack={() => go("home")}
            onPick={idea => { setPickedOcc(idea.occ); go("s3"); }}
          />
        )}

        {screen === "s3" && focusedState.data && pickedIdea && (
          <FiveQuestions
            idea={pickedIdea}
            ideas={focusedState.data}
            profile={profile}
            browserPreview={browserPreview}
            onBack={() => go("s2")}
            onNext={() => go("s4")}
          />
        )}

        {screen === "s4" && focusedState.data && pickedIdea && (
          <Checklist
            idea={pickedIdea}
            ideas={focusedState.data}
            profile={profile}
            onBack={() => go("s3")}
            onReady={() => go("s5")}
            onNotYet={() => go("notyet")}
          />
        )}

        {screen === "s5" && focusedState.data && pickedIdea && (
          <WebullFieldMap
            idea={pickedIdea}
            ideas={focusedState.data}
            onBack={() => go("s4")}
            onDone={() => {
              if (browserPreview) showToast("บันทึกไว้แล้ว ผู้ช่วยจะติดตามสัญญานี้ให้ในหน้าแรก (ตัวอย่าง)");
              go("home");
            }}
          />
        )}

        {screen === "notyet" && <NotYet onHome={() => go("home")} />}
      </div>

      {toast && (
        <div className="ez-toast on" role="status" aria-live="polite">{toast}</div>
      )}

      {postOnboarding && <AgentPanel mode="easy" llmInfo={llmInfo} openSetup={openSetup} />}
    </div>
  );
}
