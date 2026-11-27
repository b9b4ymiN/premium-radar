/** Agent bus (W6 P3) — the single frontend entry point for an agent turn: it
 *  streams AgentEvents from the Tauri `agent_ask` command, executes ui actions
 *  through per-kind handlers, and acks every action with `agent_ui_result`
 *  (plus a fresh screen context) within the backend's 5 s budget — except
 *  `confirm`, which waits for a human tap on the approval card (P4-T3): the
 *  backend parks up to 120 s, so that handler gets its own, longer budget.
 *  Pointing is just another handler kind, registered later — this module
 *  never imports it. */

import { getScreenContext } from "./anchors";
import type { AgentEvent, UiAction, UiResult } from "./types";

/** Same bridge guard as src/easy/invoke.ts so the browser preview (no Tauri)
 *  imports this module fine; the dynamic imports keep it out of the web build. */
const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Conversation sent to `agent_ask` (mirrors the Rust command input). */
export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

/** Matches the backend's busy error so the UI shows one consistent message. */
const BUSY_TH = "AI กำลังตอบคำถามก่อนหน้าอยู่ — กดหยุดก่อน หรือรอให้ตอบเสร็จ";
const NO_TAURI_TH = "ต้องเปิดจากในแอปก่อนถึงจะคุยกับ AI ได้ (ตัวพรีวิวในเบราว์เซอร์ใช้ไม่ได้)";
const NO_HANDLER_TH = "หน้าจอนี้ยังทำคำสั่งนี้ไม่ได้";
const HANDLER_TIMEOUT_TH = "ใช้เวลานานเกินไป";

/** Handler budget per action kind: 4 s + settle ≤ 250 ms keeps the ack inside
 *  the backend's 5 s limit. `confirm` instead waits for a human tap, so it gets
 *  115 s — the ack (ok:false on timeout) still lands before the backend's own
 *  120 s confirm budget expires. */
const HANDLER_TIMEOUT_MS_BY_KIND: Record<UiAction["kind"], number> = {
  go: 4_000,
  select: 4_000,
  point: 4_000,
  card: 4_000,
  confirm: 115_000,
};
const SETTLE_CAP_MS = 250;

export interface UiActionOutcome {
  ok: boolean;
  /** Thai reason when ok = false. */
  detail?: string;
}

/** A handler owns one action kind: run it, report whether the UI did it. */
type UiHandler<K extends UiAction["kind"]> = (
  action: Extract<UiAction, { kind: K }>,
) => UiActionOutcome | Promise<UiActionOutcome>;

/** Stored flat — the map is keyed by kind, so the action a handler receives
 *  always matches the kind it was registered for. */
type StoredUiHandler = (action: UiAction) => UiActionOutcome | Promise<UiActionOutcome>;

const uiHandlers = new Map<UiAction["kind"], StoredUiHandler>();

/** Register the handler for one ui action kind. One handler per kind; a later
 *  registration replaces an earlier one. Returns its unregister function,
 *  which only removes that exact handler. */
export function registerUiHandler<K extends UiAction["kind"]>(
  kind: K,
  handler: UiHandler<K>,
): () => void {
  uiHandlers.set(kind, handler as StoredUiHandler);
  return () => {
    if (uiHandlers.get(kind) === handler) uiHandlers.delete(kind);
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** Give React two frames to mount new anchors before re-reading the screen.
 *  rAF never fires while the window is hidden, so cap the wait. */
async function settleUi(): Promise<void> {
  const twoFrames = new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
  await Promise.race([twoFrames, sleep(SETTLE_CAP_MS)]);
}

async function withTimeout<T>(p: Promise<T>, ms: number, message: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const expired = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(message)), ms);
  });
  try {
    return await Promise.race([p, expired]);
  } finally {
    clearTimeout(timer);
  }
}

/** Run one ui action and ack it exactly once — success, missing handler, a
 *  thrown error and a timeout all end in a single `agent_ui_result`. */
async function executeUiAction(uiId: string, action: UiAction): Promise<void> {
  let ok = false;
  let detail: string | undefined;
  const handler = uiHandlers.get(action.kind);
  if (handler === undefined) {
    detail = NO_HANDLER_TH;
  } else {
    try {
      const outcome = await withTimeout(
        Promise.resolve(handler(action)),
        HANDLER_TIMEOUT_MS_BY_KIND[action.kind],
        HANDLER_TIMEOUT_TH,
      );
      ok = outcome.ok;
      detail = outcome.detail;
    } catch (err) {
      ok = false;
      detail = err instanceof Error ? err.message : String(err);
    }
  }
  await settleUi();
  const result: UiResult = {
    ui_id: uiId,
    ok,
    detail: detail ?? null,
    screen_context: getScreenContext(),
  };
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("agent_ui_result", { result });
  } catch (err) {
    // The ack could not be delivered (e.g. the turn already ended and the
    // backend stopped waiting); it applies its own 5 s timeout, so just log.
    console.warn("agent_ui_result failed:", err);
  }
}

let turnRunning = false;

/** Run one agent turn. Rejects with the backend's busy text while another
 *  turn is running, and with a Thai message in the browser preview. */
export async function runAgentTurn(
  messages: ChatMessage[],
  onEvent: (event: AgentEvent) => void,
): Promise<"answer" | "cancelled"> {
  if (turnRunning) throw new Error(BUSY_TH);
  if (!hasTauri) throw new Error(NO_TAURI_TH);
  turnRunning = true;
  try {
    const { invoke, Channel } = await import("@tauri-apps/api/core");
    const channel = new Channel<AgentEvent>();
    channel.onmessage = (event) => {
      onEvent(event);
      if (event.type === "ui_action") void executeUiAction(event.ui_id, event.action);
    };
    return await invoke<"answer" | "cancelled">("agent_ask", {
      input: { messages, screen_context: getScreenContext() },
      onEvent: channel,
    });
  } finally {
    turnRunning = false;
  }
}

/** Ask the backend to cancel the running turn (false in the browser preview). */
export async function cancelAgent(): Promise<boolean> {
  if (!hasTauri) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<boolean>("agent_cancel");
}
