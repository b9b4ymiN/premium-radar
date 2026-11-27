// Agent ⇄ UI protocol (W6 P3). Single source of truth for the shapes that
// cross the Tauri boundary; the Rust side (`radar_core::agent::runner::AgentEvent`,
// `radar_core::agent::tools::ui`, `src-tauri/src/agent_cmd.rs`) must match.

export type AgentMode = "easy" | "pro";

/** Pro-mode main tabs (App.tsx `MainTab`). */
export type ProPage = "home" | "market" | "monitor" | "lab" | "portfolio" | "watchlist";

/** Easy-mode screens the agent may open (EasyApp.tsx `Screen` subset). */
export type EasyScreen = "home" | "s2" | "s3" | "s4";

/** One element the agent may point at. `id` = `kind:ref[/field]`, e.g.
 *  `idea:NVDA261218P00175000/premium`, `tab:portfolio`, `whatif:price`. */
export interface AnchorInfo {
  id: string;
  /** Short Thai label of what the element shows (≤ 40 chars). */
  label: string;
}

/** Sent with every `agent_ask` and returned with every ui ack. */
export interface ScreenContext {
  mode: AgentMode;
  /** Pro: the main tab. Easy: the screen. */
  page: ProPage | EasyScreen | string;
  /** Underlying symbol in focus, if any (Pro: scanned symbol, Easy: opened pick). */
  symbol: string | null;
  /** OCC symbol of the selected contract, if any. */
  selected: string | null;
  /** Visible anchors, at most 60, most relevant first. */
  anchors: AnchorInfo[];
}

export type PointEffect =
  | "circle" | "underline" | "highlight" | "box"   // rough-notation
  | "blink" | "pulse" | "spotlight" | "bubble";    // CSS

export type PointColor = "accent" | "warn" | "danger";

export interface PointStep {
  anchor: string;
  effect: PointEffect;
  /** Optional Thai bubble text (≤ 120 chars). */
  message?: string;
  color?: PointColor;
}

/** The structured recommendation card (`present_idea`, W6 P4). Every number
 *  is computed by Rust from the contract `ref` — the model only writes the
 *  five plain-Thai answers. */
export interface IdeaCardData {
  ref: string;
  symbol: string;
  headline: string;
  /** ได้เงินเท่าไร / แลกกับอะไร / โอกาสแค่ไหน / ทำไมช่วงนี้ / แย่สุดคืออะไร */
  answers: { gain: string; tradeoff: string; chance: string; why_now: string; worst: string };
  numbers: {
    strike: number;
    expiry: string;
    days: number;
    premium_usd: number;
    cash_needed_usd: number;
    breakeven: number;
    assign_pct: number | null;
    worst_30_usd: number | null;
    max_loss_usd: number | null;
  };
  as_of: string;
}

/** What the UI must do for one ui tool call. `card` shows a recommendation
 *  card in the chat; `confirm` asks the user to approve a local write (the
 *  ack's ok = approved) and may wait up to 120 s. */
export type UiAction =
  | { kind: "go"; mode: AgentMode; page: string; symbol?: string | null; ref?: string | null }
  | { kind: "select"; ref: string; open?: "detail" | "what_if"; price?: number | null }
  | { kind: "point"; steps: PointStep[] }
  | { kind: "card"; card: IdeaCardData }
  | { kind: "confirm"; title: string; detail: string };

/** Events streamed by `agent_ask` (serde tag = "type", snake_case). */
export type AgentEvent =
  | { type: "thinking"; round: number }
  | { type: "tool_start"; call_id: string; name: string; args: unknown; tier: "read" | "ui" | "write_local" | "trade" }
  | { type: "tool_end"; call_id: string; name: string; ok: boolean; summary: string; ms: number }
  | { type: "ui_action"; ui_id: string; action: UiAction }
  | { type: "answer"; text: string; unverified: boolean; ungrounded: string[] }
  | { type: "error"; message: string }
  | { type: "cancelled" };

/** Payload of the `agent_ui_result` command — the UI's ack for one ui_action.
 *  Must arrive within 5 s or the agent gets `ui_timeout`. */
export interface UiResult {
  ui_id: string;
  ok: boolean;
  /** Thai reason when ok = false (e.g. anchor no longer on screen). */
  detail?: string | null;
  /** Screen after the action (anchors may have changed). */
  screen_context?: ScreenContext | null;
}
