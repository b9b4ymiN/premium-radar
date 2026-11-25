/** EasyApp — the Easy Mode screen state machine (CR-001 / W3).
 *
 *  onboarding 1–3 → home → step1 stocks → step2 ideas → step3 five questions
 *  + what-if + ask → step4 checklist → step5 Webull field map / not-yet.
 *
 *  Data flow: App hands over the scan state (`symbol`/`setSymbol` drive the
 *  engine scan, `loading`/`scanMsg` report progress) and the current quote.
 *  Opening a stock idea sets the app symbol (which starts the normal scan),
 *  then calls `easy_ideas(symbol)`; while the engine has not finished
 *  scanning, Rust answers with the "…กำลังโหลด" error and this component
 *  retries calmly every 1.5 s (up to ~60 s) and immediately when the scan
 *  settles. Browser preview (no Tauri) renders the prototype's sample data.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { EasyIdeas, EasyProfile, Stance } from "../types/easy";
import { Onboarding, StockStanceList } from "./Onboarding";
import { EasyHome, type HeldContract } from "./EasyHome";
import { IdeasStep } from "./IdeasStep";
import { FiveQuestions } from "./FiveQuestions";
import { Checklist } from "./Checklist";
import { WebullFieldMap, NotYet } from "./WebullFieldMap";
import { EzIcon, EzSprite } from "./icons";
import { daysUntil, usd } from "./util";
import { SAMPLE_HELD, sampleIdeas, sampleProfile } from "./sample";
import { easyInvoke } from "./invoke";
import "./easy.css";

type Screen = "boot" | "ob1" | "ob2" | "ob3" | "home" | "s1" | "s2" | "s3" | "s4" | "s5" | "notyet";

interface IdeasState {
  data: EasyIdeas | null;
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
}

export function EasyApp({ tauriReady, hasTauri, symbol, setSymbol, loading, stock }: EasyAppProps) {
  const browserPreview = !hasTauri;
  const [screen, setScreen] = useState<Screen>(() => (hasTauri ? "boot" : "ob1"));
  const [profile, setProfile] = useState<EasyProfile>(() => sampleProfile(false));
  const [profileLoaded, setProfileLoaded] = useState(false);
  const [ideasState, setIdeasState] = useState<Record<string, IdeasState>>({});
  const [pickedOcc, setPickedOcc] = useState<string | null>(null);
  const [positions, setPositions] = useState<PfPos[] | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const mainRef = useRef<HTMLDivElement | null>(null);
  const toastTimer = useRef(0);
  const profileRef = useRef(profile);
  const ideasStateRef = useRef(ideasState);
  const bag = useRef<RetryBag>({ timers: {}, tries: {} });

  // mirrors for async callbacks (never written during render)
  useEffect(() => { profileRef.current = profile; }, [profile]);
  useEffect(() => { ideasStateRef.current = ideasState; }, [ideasState]);

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
        if (ep && Array.isArray(ep.stocks) && ep.stocks.length) {
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
    easyInvoke("save_easy_profile", { profile }).catch(() => { /* keep the session going */ });
  }, [profile, profileLoaded, hasTauri]);

  // ---------- the stock the agent works on ----------
  const primary = (profile.stocks.find(s => s.stance === "want") ?? profile.stocks[0])?.symbol?.toUpperCase() ?? "NVDA";
  const second = profile.stocks.map(s => s.symbol.toUpperCase()).find(s => s !== primary) ?? null;

  // entering home/s1/s2 points the engine at the primary stock and makes sure
  // its ideas are loaded (home also wants the second stock's skip reason)
  useEffect(() => {
    if (screen !== "home" && screen !== "s1" && screen !== "s2") return;
    const need = screen === "home" && second ? [primary, second] : [primary];
    need.forEach(S => {
      const st = ideasStateRef.current[S];
      if (!st?.data && !st?.loading) void fetchIdeas(S);
    });
    if (symbol.toUpperCase() !== primary) setSymbol(primary);
  }, [screen, primary, second, fetchIdeas, symbol, setSymbol]);

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

  // new screen: back to the top, focus the heading (as the prototype does)
  useEffect(() => {
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
  const primaryState = ideasState[primary] ?? { data: null, loading: false, error: null };
  const secondState = second ? ideasState[second] : undefined;
  const pickedIdea = primaryState.data?.ideas.find(i => i.occ === pickedOcc) ?? null;

  const go = (s: Screen) => setScreen(s);
  const patchProfile = (patch: Partial<EasyProfile>) => setProfile(cur => ({ ...cur, ...patch }));
  const retryPrimary = () => {
    bag.current.tries[primary] = 0;
    void fetchIdeas(primary);
  };

  const finishOnboarding = () => {
    setProfile(cur => ({ ...cur, onboarded: true }));
    go("home");
  };

  const openIdeas = () => {
    setPickedOcc(null);
    if (symbol.toUpperCase() !== primary) setSymbol(primary);
    retryPrimary();
    go("s2");
  };

  const onStance = (sym: string, stance: Stance) =>
    patchProfile({ stocks: profile.stocks.map(s => (s.symbol === sym ? { ...s, stance } : s)) });

  // ---------- flow bar ----------
  const flow = (() => {
    if (screen === "ob1") return { total: 3, step: 1, label: "ตั้งค่า 1 จาก 3" };
    if (screen === "ob2") return { total: 3, step: 2, label: "ตั้งค่า 2 จาก 3" };
    if (screen === "ob3") return { total: 3, step: 3, label: "ตั้งค่า 3 จาก 3" };
    const dec: Partial<Record<Screen, number>> = { s1: 1, s2: 2, s3: 3, s4: 4, s5: 4 };
    const step = dec[screen];
    return step ? { total: 4, step, label: `ขั้น ${step} จาก 4` } : null;
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

        {(screen === "ob1" || screen === "ob2" || screen === "ob3") && (
          <Onboarding
            step={screen === "ob1" ? 1 : screen === "ob2" ? 2 : 3}
            profile={profile}
            onPatch={patchProfile}
            onGo={s => (s === "home" ? finishOnboarding() : go(s))}
            browserPreview={browserPreview}
          />
        )}

        {screen === "home" && (
          <EasyHome
            profile={profile}
            ideas={primaryState.data}
            ideasBusy={primaryState.loading}
            ideasError={primaryState.error}
            skip={secondState?.data?.skip_reason_th ? secondState.data : null}
            held={held}
            browserPreview={browserPreview}
            onRetry={retryPrimary}
            onOpenIdeas={openIdeas}
            onGo={s => go(s)}
          />
        )}

        {screen === "s1" && (
          <section className="ez-screen">
            <button className="ez-back" onClick={() => go("home")}><EzIcon name="back" />หน้าแรก</button>
            <h1>หุ้นที่ฉันสนใจ</h1>
            <p className="ez-lead">เลือกหุ้นที่อยากให้ผู้ช่วยหาไอเดีย ตอนนี้ตั้งเงินไว้ <strong>{usd(profile.cash)}</strong></p>
            <div className="ez-panel ez-gap-l">
              <StockStanceList stocks={profile.stocks} onStance={onStance} />
            </div>
            <div className="ez-actions"><button className="ez-btn primary" onClick={openIdeas}>ให้ผู้ช่วยหาไอเดีย</button></div>
          </section>
        )}

        {screen === "s2" && (
          <IdeasStep
            ideas={primaryState.data}
            ideasBusy={primaryState.loading}
            ideasError={primaryState.error}
            browserPreview={browserPreview}
            onRetry={retryPrimary}
            onBack={() => go("home")}
            onPick={idea => { setPickedOcc(idea.occ); go("s3"); }}
          />
        )}

        {screen === "s3" && primaryState.data && pickedIdea && (
          <FiveQuestions
            idea={pickedIdea}
            ideas={primaryState.data}
            profile={profile}
            browserPreview={browserPreview}
            onBack={() => go("s2")}
            onNext={() => go("s4")}
          />
        )}

        {screen === "s4" && primaryState.data && pickedIdea && (
          <Checklist
            idea={pickedIdea}
            ideas={primaryState.data}
            profile={profile}
            onBack={() => go("s3")}
            onReady={() => go("s5")}
            onNotYet={() => go("notyet")}
          />
        )}

        {screen === "s5" && primaryState.data && pickedIdea && (
          <WebullFieldMap
            idea={pickedIdea}
            ideas={primaryState.data}
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
    </div>
  );
}
