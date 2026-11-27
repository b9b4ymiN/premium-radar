/** AgentPanel (W6 P3-T4) — the streaming agent chat for both modes. It runs
 *  turns through the agent bus, shows each turn's tool timeline live while
 *  the agent works, and owns the `point` ui handler wherever it is mounted.
 *  The conversation lives in a module store, so it survives closing the panel
 *  and switching easy/pro — even for a turn that is still running. */

import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { ForgeBird } from "../components/RadarAssistant/ForgeBird";
import { getAnchorElement } from "./anchors";
import { cancelAgent, registerUiHandler, runAgentTurn, type ChatMessage } from "./bus";
import { IdeaCard } from "./IdeaCard";
import { point } from "./point";
import type { AgentEvent, IdeaCardData } from "./types";
import "./agent.css";

interface Props {
  mode: "easy" | "pro";
  llmInfo: { label: string; model: string } | null;
  openSetup: () => void;
}

/** Friendly Thai label per tool name; an unknown tool shows its raw name. */
const TOOL_LABELS: Record<string, string> = {
  market_pulse: "ดูภาพรวมตลาด",
  stock_brief: "ดูข้อมูลหุ้น",
  price_behavior: "ดูการแกว่งของราคา",
  find_ideas: "หาไอเดีย",
  scan_chain: "สแกนสัญญา",
  contract_detail: "ดูรายละเอียดสัญญา",
  chain_stats: "ดูภาพรวมสัญญาของหุ้น",
  what_if: "คำนวณถ้าราคาเปลี่ยน",
  my_portfolio: "ดูพอร์ต",
  my_profile: "ดูโปรไฟล์",
  my_watchlists: "ดูลิสต์หุ้น",
  read_playbook: "เปิดคู่มือ",
  ui_go: "พาไปหน้าจอ",
  ui_select: "เลือกสัญญา",
  ui_point: "ชี้บนจอ",
  present_idea: "สรุปคำแนะนำ",
};

interface TimelineStep {
  key: string;
  label: string;
  state: "run" | "done" | "fail";
  ms?: number;
}

interface TurnItem {
  id: number;
  question: string;
  steps: TimelineStep[];
  collapsed: boolean;
  /** The recommendation card `present_idea` attached to this turn. */
  card: IdeaCardData | null;
  answer: { text: string; unverified: boolean; ungrounded: string[] } | null;
  error: string | null;
  cancelled: boolean;
  running: boolean;
}

// The one conversation of the whole app. Events of a turn keep landing here
// even while no panel is mounted (e.g. the user switched mode mid-turn).
const store = { turns: [] as TurnItem[] };
let nextTurnId = 1;
const listeners = new Set<() => void>();

function setTurns(update: (prev: TurnItem[]) => TurnItem[]): void {
  store.turns = update(store.turns);
  for (const l of listeners) l();
}

function useTurns(): TurnItem[] {
  return useSyncExternalStore(
    (onChange) => {
      listeners.add(onChange);
      return () => { listeners.delete(onChange); };
    },
    () => store.turns,
  );
}

/** Fold one streamed event into the running turn. */
function applyEvent(t: TurnItem, ev: AgentEvent): TurnItem {
  switch (ev.type) {
    case "thinking":
    case "tool_start": {
      // a new step implies every earlier one finished (even without its
      // tool_end) — close leftovers so the timeline never stalls
      const steps = t.steps.map(s => (s.state === "run" ? { ...s, state: "done" as const } : s));
      const step: TimelineStep = ev.type === "thinking"
        ? { key: `think-${t.id}-${ev.round}`, label: "กำลังคิด…", state: "run" }
        : { key: ev.call_id, label: TOOL_LABELS[ev.name] ?? ev.name, state: "run" };
      return { ...t, steps: [...steps, step] };
    }
    case "tool_end":
      return {
        ...t,
        steps: t.steps.map(s => (s.key === ev.call_id
          ? { ...s, state: ev.ok ? "done" as const : "fail" as const, ms: ev.ms }
          : s)),
      };
    case "answer":
      return {
        ...t,
        answer: { text: ev.text, unverified: ev.unverified, ungrounded: ev.ungrounded },
        running: false,
        collapsed: true,
      };
    case "error":
      return { ...t, error: ev.message, running: false };
    case "cancelled":
      return { ...t, cancelled: true, running: false };
    case "ui_action":
      // the bus executes the action itself; the timeline follows the
      // ui_go/ui_select/ui_point tool_start/tool_end pair around it
      return t;
    default: {
      const exhaustive: never = ev;
      void exhaustive;
      return t;
    }
  }
}

const POS_KEY = "premium-radar:assistant-pos";
const OPEN_KEY = "premium-radar:assistant-open";

type Pos = { right: number; bottom: number };
const DEFAULT_POS: Pos = { right: 18, bottom: 46 };

function readPos(): Pos {
  try {
    const p = JSON.parse(localStorage.getItem(POS_KEY) ?? "null") as Pos | null;
    if (p && Number.isFinite(p.right) && Number.isFinite(p.bottom)) return p;
  } catch { /* storage unavailable — default position */ }
  return DEFAULT_POS;
}

function readOpen(): boolean {
  try { return localStorage.getItem(OPEN_KEY) === "1"; } catch { return false; }
}

export function AgentPanel(p: Props) {
  const turns = useTurns();
  const [open, setOpen] = useState(readOpen);
  const [pos, setPos] = useState<Pos>(readPos);
  const [input, setInput] = useState("");
  const [unread, setUnread] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const busy = turns.some(t => t.running);
  const runningTurn = turns.find(t => t.running) ?? null;
  const currentStep = runningTurn
    ? [...runningTurn.steps].reverse().find(s => s.state === "run")?.label ?? "กำลังคิด…"
    : null;
  const finishedCount = turns.filter(t => t.answer || t.error || t.cancelled).length;

  // the panel owns pointing and the recommendation card for whichever mode it
  // is mounted in; the card handler attaches it to the turn that asked for it
  // (the running turn, or the newest one as a fallback)
  useEffect(() => registerUiHandler("card", a => {
    setTurns(prev => {
      if (prev.length === 0) return prev;
      const running = prev.findIndex(t => t.running);
      const target = running === -1 ? prev.length - 1 : running;
      return prev.map((t, i) => (i === target ? { ...t, card: a.card } : t));
    });
    return { ok: true };
  }), []);
  useEffect(() => registerUiHandler("point", a => point(a.steps, getAnchorElement)), []);

  useEffect(() => {
    try { localStorage.setItem(OPEN_KEY, open ? "1" : "0"); } catch { /* ignore */ }
    if (open) {
      setUnread(false);
      inputRef.current?.focus();
    }
  }, [open]);

  // keep the newest message in view; flag finished answers that arrive while closed
  const prevFinished = useRef(finishedCount);
  useEffect(() => {
    const grew = finishedCount > prevFinished.current;
    prevFinished.current = finishedCount;
    if (open) logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
    else if (grew) setUnread(true);
  }, [turns, finishedCount, open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  const send = () => {
    const q = input.trim();
    if (!q || busy) return;
    setInput("");
    const history: ChatMessage[] = turns
      .filter(t => t.answer)
      .flatMap(t => [
        { role: "user" as const, content: t.question },
        { role: "assistant" as const, content: t.answer!.text },
      ]);
    const turn: TurnItem = {
      id: nextTurnId++,
      question: q,
      steps: [],
      collapsed: false,
      card: null,
      answer: null,
      error: null,
      cancelled: false,
      running: true,
    };
    const applyToRunning = (patch: (t: TurnItem) => TurnItem) => {
      setTurns(prev => {
        const i = prev.findIndex(t => t.running);
        return i === -1 ? prev : prev.map((t, j) => (j === i ? patch(t) : t));
      });
    };
    setTurns(prev => [...prev, turn]);
    runAgentTurn([...history, { role: "user", content: q }], ev => {
      applyToRunning(t => applyEvent(t, ev));
    }).catch(err => {
      // rejected promise (busy / browser preview / backend error) — same place
      applyToRunning(t => ({ ...t, running: false, error: err instanceof Error ? err.message : String(err) }));
    });
  };

  const undoLast = () => {
    if (busy || turns.length === 0) return;
    setTurns(prev => prev.slice(0, -1));
  };

  // drag by the header; the panel is anchored to the bottom-right corner
  const startDrag = (e: React.PointerEvent<HTMLDivElement>) => {
    if ((e.target as HTMLElement).closest("button")) return;
    const start = { x: e.clientX, y: e.clientY, ...pos };
    const panel = e.currentTarget.parentElement as HTMLElement;
    const w = panel.offsetWidth;
    const h = panel.offsetHeight;
    const move = (ev: PointerEvent) => {
      const right = Math.min(Math.max(start.right - (ev.clientX - start.x), 4), window.innerWidth - w - 4);
      const bottom = Math.min(Math.max(start.bottom - (ev.clientY - start.y), 4), window.innerHeight - h - 4);
      setPos({ right, bottom });
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setPos(cur => {
        try { localStorage.setItem(POS_KEY, JSON.stringify(cur)); } catch { /* ignore */ }
        return cur;
      });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const intro = p.mode === "easy"
    ? `ถามอะไรก็ได้ค่ะ เช่น "อยากได้เดือนละ $200 มีอะไรดี" หรือ "หุ้นที่ถืออยู่ตอนนี้เสี่ยงไหม" — เราจะดูข้อมูลจริงแล้วตอบทีละขั้น พร้อมชี้ให้ดูบนจอ`
    : `ถามเกี่ยวกับสัญญาในตารางได้เลย เช่น "มีเงิน $18,000 ตัวไหนน่าทำ?"
      หรือ "NVDA 175P มีความเสี่ยงยังไง" — คำตอบอ้างอิงข้อมูลจริงเท่านั้น`;
  const placeholder = p.mode === "easy"
    ? "ถามได้เลย เช่น อยากได้เดือนละ $200 มีอะไรดี"
    : "ถามเกี่ยวกับสัญญาในตาราง...";
  const title = p.mode === "easy" ? "ผู้ช่วย AI" : "Radar Assistant";

  return (
    <>
      <button
        type="button"
        className={`ra-fab${open ? " on" : ""}`}
        onClick={() => setOpen(o => !o)}
        aria-expanded={open}
        aria-label={title}
        title={`${title} — ถาม AI ได้เลย (Esc ปิด)`}
      >
        <ForgeBird size={24} />
        {(unread || busy) && <i className={busy ? "busy" : ""} aria-hidden="true" />}
      </button>

      {open && (
        <section
          className="ra-panel agent-panel"
          role="dialog"
          aria-label={title}
          style={{ right: pos.right, bottom: pos.bottom }}
        >
          <div className="ra-head" onPointerDown={startDrag}>
            <span className="ra-spark"><ForgeBird size={16} /></span>
            <b>{title}</b>
            {p.mode === "pro" && (
              <span className={`ra-model ${p.llmInfo ? "ok" : "warn"}`}>
                {p.llmInfo ? `AI · ${p.llmInfo.model}` : "AI ยังไม่ตั้งค่า"}
              </span>
            )}
            {!busy && turns.length > 0 && (
              <button
                type="button"
                className="agent-panel-undo"
                onClick={undoLast}
                title="ลบคำถามล่าสุดพร้อมคำตอบของมัน"
              >ย้อน</button>
            )}
            <button type="button" className="ra-close" onClick={() => setOpen(false)} aria-label="ปิด">✕</button>
          </div>

          {!p.llmInfo ? (
            <div className="ra-empty">
              <p>
                ยังไม่ได้ตั้งค่า AI assistant — ใส่ LLM API key (Z.ai / OpenAI / Ollama)
                ในหน้า Setup ได้เลย ไม่เกี่ยวกับสถานะ feed ข้อมูล
              </p>
              <button type="button" className="ra-setup" onClick={p.openSetup}>เปิดหน้า Setup</button>
            </div>
          ) : (
            <>
              <div className="chatlog" ref={logRef}>
                {turns.length === 0 && (
                  <div className="bubble assistant">
                    <Markdown remarkPlugins={[remarkGfm]}>{intro}</Markdown>
                  </div>
                )}
                {turns.map(t => (
                  <div key={t.id} className="agent-panel-turn">
                    <div className="bubble user">{t.question}</div>
                    {t.steps.length > 0 && (
                      <>
                        {!t.running && (
                          <button
                            type="button"
                            className="agent-panel-steps-toggle"
                            onClick={() => setTurns(prev => prev.map(x => (x.id === t.id ? { ...x, collapsed: !x.collapsed } : x)))}
                          >ขั้นตอน {t.steps.length} ขั้น {t.collapsed ? "▸" : "▾"}</button>
                        )}
                        {!t.collapsed && (
                          <ul className="agent-panel-steps">
                            {t.steps.map(s => (
                              <li key={s.key} className={s.state}>
                                <i aria-hidden="true" />
                                <span>{s.label}</span>
                                {s.state !== "run" && s.ms != null && <span className="agent-panel-ms">{s.ms}ms</span>}
                              </li>
                            ))}
                          </ul>
                        )}
                      </>
                    )}
                    {t.card && <IdeaCard card={t.card} mode={p.mode} />}
                    {t.answer && (
                      <div className="bubble assistant">
                        <Markdown remarkPlugins={[remarkGfm]}>{t.answer.text}</Markdown>
                        {t.answer.unverified && (
                          <div
                            className="agent-panel-unverified"
                            title={t.answer.ungrounded.length > 0 ? `ไม่มีที่มา: ${t.answer.ungrounded.join(", ")}` : undefined}
                          >
                            ⚠ ตัวเลขบางตัวยังหาที่มาไม่ได้ — ตรวจกับหน้าจอก่อนตัดสินใจ
                          </div>
                        )}
                      </div>
                    )}
                    {t.cancelled && <div className="bubble assistant agent-panel-note">หยุดแล้ว</div>}
                    {t.error && <div className="bubble assistant ra-error">⚠ {t.error}</div>}
                  </div>
                ))}
              </div>
              {busy && currentStep && (
                <div className="agent-panel-status" role="status">
                  <span className="agent-panel-dot" aria-hidden="true" />
                  <span>AI กำลัง…{currentStep}</span>
                  <button type="button" className="agent-panel-stop" onClick={() => { void cancelAgent(); }}>หยุด</button>
                </div>
              )}
              <div className="chatinput">
                <input
                  ref={inputRef}
                  value={input}
                  onChange={e => setInput(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") send(); }}
                  placeholder={placeholder}
                />
                <button type="button" aria-label="Send" onClick={send} disabled={busy}>➤</button>
              </div>
            </>
          )}
        </section>
      )}
    </>
  );
}
