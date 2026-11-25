// Easy Mode data contract (CR-001 / W3).
// Rust (`radar_core::easy`, Tauri commands `easy_ideas`, `save_easy_profile`)
// produces exactly these shapes as JSON (serde, snake_case). React only renders
// them. Any change here must be mirrored in Rust and its JSON-shape test.

/** Whether the user would be happy to own the stock if assigned. */
export type Stance = "want" | "avoid";

/** How often the user accepts having to buy shares: ~1, 2 or 3 in 10. */
export type RiskLevel = 1 | 2 | 3;

export interface EasyStock {
  symbol: string;
  stance: Stance;
}

/** Saved in settings (not secret). `onboarded=false` ⇒ show the 3 questions. */
export interface EasyProfile {
  onboarded: boolean;
  stocks: EasyStock[];
  /** USD the user is ready to set aside. */
  cash: number;
  risk: RiskLevel;
}

export type IdeaLevel = "safe" | "balanced" | "high";

/** One sell-put idea, all money figures in USD for ONE contract (100 shares). */
export interface EasyIdea {
  /** OCC symbol, e.g. "NVDA261218P00165000". */
  occ: string;
  symbol: string;
  level: IdeaLevel;
  /** True when this is the one the agent recommends for this profile. */
  recommended: boolean;
  strike: number;
  /** ISO date "2026-12-18". */
  expiry: string;
  days: number;
  /** Premium per share actually used (bid-side credit), e.g. 2.20. */
  premium: number;
  /** Money received now: premium × 100. */
  credit: number;
  /** Cash that must be set aside: strike × 100. */
  cost: number;
  /** credit / cost × 100, for the holding period (not annualised). */
  period_pct: number;
  /** Annualised %, shown only as a muted footnote. */
  annual_pct: number | null;
  /** Break-even price per share: strike − premium. */
  breakeven: number;
  /** How far below today's price the strike is, in % (positive = below). */
  below_spot_pct: number;
  /** Chance of having to buy the shares, 0–100 (from p_itm). */
  assign_pct: number;
  /** The three expiry outcomes, each 0–100, summing to 100 (±1 rounding). */
  cases: {
    /** Close above strike: keep the full credit. */
    keep_pct: number;
    /** Close between break-even and strike: get shares, no loss yet. */
    between_pct: number;
    /** Close below break-even: get shares at a paper loss. */
    loss_pct: number;
  };
  /** Paper result if the stock falls 30% by expiry (negative = loss). */
  worst_30_usd: number;
  /** Theoretical max loss if the stock went to $0: cost − credit. */
  max_loss_usd: number;
  /** True when assign_pct ≥ 30 or above the user's comfort — show ⚠. */
  warn: boolean;
  /** Not enough cash for this idea (cost > profile cash). */
  cash_short: boolean;
}

/** "Why now" evidence from real data only; null fields ⇒ say "ยังไม่มีข้อมูล". */
export interface WhyNow {
  /** ATM implied volatility, % per year. */
  iv_pct: number | null;
  /** Realised volatility of daily closes over the last 30 sessions, % per year. */
  realized_30d_pct: number | null;
  /** Plain-Thai sentence built in Rust from the two numbers (or the no-data case). */
  text_th: string;
}

export interface EasyIdeas {
  symbol: string;
  spot: number;
  /** Where the price came from: "LIVE" | "API" | "CLOSE" | "DEMO". */
  source: string;
  stance: Stance;
  /** Agent's one-paragraph Thai sentence for the Home / step 2 header. */
  agent_say_th: string;
  /** Up to three ideas (safe / balanced / high); may be empty. */
  ideas: EasyIdea[];
  /** When no idea fits: the plain-Thai reason (e.g. not enough cash). */
  skip_reason_th: string | null;
  why_now: WhyNow;
  /** RFC3339 time the ideas were computed. */
  computed_at: string;
}

// ---------- universe ideas (the assistant proposes from the whole market) ----------
// Rust command `easy_universe_ideas` (no arguments; it reads the saved profile)
// returns `UniverseIdeas` in exactly this shape and rejects with a Thai string.

/** The 0–100 score breakdown behind a pick; null parts were not computable. */
export interface ScoreParts {
  vrp: number | null;
  premium: number | null;
  fit: number | null;
  cushion: number | null;
  liquidity: number | null;
  event_penalty: number;
}

/** Open-interest context for a pick — unproven signal, shown small and dim. */
export interface OiContext {
  put_wall_strike: number | null;
  put_wall_oi: number | null;
  oi_change_pct: number | null;
  note_th: string;
}

/** One ranked idea the assistant picked out of the scanned universe. */
export interface UniversePick {
  rank: number;
  idea: EasyIdea;
  /** 0..100 */
  score: number;
  parts: ScoreParts;
  reasons_th: string[];
  cautions_th: string[];
  iv_pct: number | null;
  realized_30d_pct: number | null;
  earnings_date: string | null;
  oi: OiContext;
}

/** When the budget cannot afford even the cheapest candidate. */
export interface BudgetShort {
  min_cash_usd: number;
  cheapest_symbol: string;
  text_th: string;
}

export interface UniverseIdeas {
  session_date: string | null;
  /** RFC3339 */
  as_of: string | null;
  /** "API" | "LOCAL" */
  source: string;
  scanned_symbols: number;
  candidate_symbols: number;
  market_note_th: string;
  picks: UniversePick[];
  budget_short: BudgetShort | null;
  agent_say_th: string;
  computed_at: string;
}
