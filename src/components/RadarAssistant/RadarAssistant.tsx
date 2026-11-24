/** Radar Assistant as a floating chat: a button bottom-right opens a panel
 *  that floats above the workspace, can be dragged by its header, and keeps
 *  the conversation while closed (state lives in App). */

import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

interface Props {
  llmInfo: { label: string; model: string } | null;
  isLive: boolean;
  chat: ChatMessage[];
  chatBusy: boolean;
  chatError: string;
  chatInput: string;
  setChatInput: (v: string) => void;
  sendChat: () => void;
  openSetup: () => void;
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

export function RadarAssistant(p: Props) {
  const [open, setOpen] = useState(readOpen);
  const [pos, setPos] = useState<Pos>(readPos);
  const [unread, setUnread] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const lastCount = useRef(p.chat.length);

  useEffect(() => {
    try { localStorage.setItem(OPEN_KEY, open ? "1" : "0"); } catch { /* ignore */ }
    if (open) {
      setUnread(false);
      inputRef.current?.focus();
    }
  }, [open]);

  // keep the newest message in view; flag answers that arrive while closed
  useEffect(() => {
    const grew = p.chat.length > lastCount.current;
    lastCount.current = p.chat.length;
    if (open) logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
    else if (grew && p.chat[p.chat.length - 1]?.role === "assistant") setUnread(true);
  }, [p.chat, p.chatBusy, open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

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

  return (
    <>
      <button
        type="button"
        className={`ra-fab${open ? " on" : ""}`}
        onClick={() => setOpen(o => !o)}
        aria-expanded={open}
        aria-label="Radar Assistant"
        title="Radar Assistant — ถาม AI เกี่ยวกับสัญญาในตาราง (Esc ปิด)"
      >
        <span aria-hidden="true">✦</span>
        {(unread || p.chatBusy) && <i className={p.chatBusy ? "busy" : ""} aria-hidden="true" />}
      </button>

      {open && (
        <section
          className="ra-panel"
          role="dialog"
          aria-label="Radar Assistant"
          style={{ right: pos.right, bottom: pos.bottom }}
        >
          <div className="ra-head" onPointerDown={startDrag}>
            <span className="ra-spark" aria-hidden="true">✦</span>
            <b>Radar Assistant</b>
            <span className={`ra-model ${p.llmInfo ? "ok" : "warn"}`}>
              {p.llmInfo ? `AI · ${p.llmInfo.model}` : "AI ยังไม่ตั้งค่า"}
            </span>
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
              {!p.isLive && (
                <div className="ra-note">⚠ feed ข้อมูลยังไม่ live — AI จะบอกว่าไม่มีข้อมูลจนกว่า scan สำเร็จ</div>
              )}
              <div className="chatlog" ref={logRef}>
                {p.chat.length === 0 && (
                  <div className="bubble assistant">
                    <Markdown remarkPlugins={[remarkGfm]}>
                      {`ถามเกี่ยวกับสัญญาในตารางได้เลย เช่น "มีเงิน $18,000 ตัวไหนน่าทำ?"
                      หรือ "NVDA 175P มีความเสี่ยงยังไง" — คำตอบอ้างอิงข้อมูลจริงเท่านั้น`}
                    </Markdown>
                  </div>
                )}
                {p.chat.map((m, i) => (
                  <div key={i} className={`bubble ${m.role}`}>
                    {m.role === "assistant"
                      ? <Markdown remarkPlugins={[remarkGfm]}>{m.content}</Markdown>
                      : m.content}
                  </div>
                ))}
                {p.chatBusy && <div className="bubble assistant">AI กำลังดึงข้อมูลและวิเคราะห์ ...</div>}
                {p.chatError && <div className="bubble assistant ra-error">⚠ {p.chatError}</div>}
              </div>
              <div className="chatinput">
                <input
                  ref={inputRef}
                  value={p.chatInput}
                  onChange={e => p.setChatInput(e.target.value)}
                  onKeyDown={e => { if (e.key === "Enter") p.sendChat(); }}
                  placeholder={p.isLive ? "ถามเกี่ยวกับสัญญาในตาราง..." : "ถามได้เลย (จะตอบเมื่อมีข้อมูล)..."}
                />
                <button type="button" aria-label="Send" onClick={p.sendChat} disabled={p.chatBusy}>➤</button>
              </div>
            </>
          )}
        </section>
      )}
    </>
  );
}
