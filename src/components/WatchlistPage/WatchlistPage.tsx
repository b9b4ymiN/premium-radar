//! หน้า Watchlists (PLAN-APP2 P16) — CRUD ผ่าน Rust (local persistence)

import { useCallback, useEffect, useState } from "react";

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface List { id: string; name: string; symbols: string[] }
interface Store { lists: List[]; active: string | null }

export default function WatchlistPage({
  onOpenRadar,
}: { onOpenRadar: (symbol: string) => void }) {
  const [store, setStore] = useState<Store | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [addText, setAddText] = useState("");
  const [newListName, setNewListName] = useState("");

  const load = useCallback(() => {
    invokeCmd<Store>("list_watchlists").then(setStore).catch(e => setErr(String(e)));
  }, []);
  useEffect(() => { load(); }, [load]);

  const act = async (action: string, payload: string, listId?: string) => {
    setErr(null);
    try {
      setStore(await invokeCmd<Store>("save_watchlist", { listId: listId ?? null, action, payload }));
    } catch (e) {
      setErr(String(e));
    }
  };

  const active = store?.lists.find(l => l.id === store.active) ?? store?.lists[0];

  return (
    <div className="wlpage">
      <div className="wl-head">
        <select
          className="wl-select"
          value={active?.id ?? ""}
          onChange={e => act("setactive", e.target.value)}
        >
          {store?.lists.map(l => (
            <option key={l.id} value={l.id}>{l.name} ({l.symbols.length})</option>
          ))}
        </select>
        <div className="wl-newlist">
          <input
            value={newListName}
            onChange={e => setNewListName(e.target.value)}
            onKeyDown={e => { if (e.key === "Enter" && newListName.trim()) { act("newlist", newListName.trim()); setNewListName(""); } }}
            placeholder="ชื่อลิสต์ใหม่…"
          />
          <button onClick={() => { if (newListName.trim()) { act("newlist", newListName.trim()); setNewListName(""); } }}>+ สร้าง</button>
        </div>
        {store && store.lists.length > 1 && active && (
          <button className="wl-danger" onClick={() => act("deletelist", "", active.id)}>ลบลิสต์นี้</button>
        )}
      </div>

      {err && <div className="pf-err">{err}</div>}

      <div className="wl-addrow">
        <input
          value={addText}
          onChange={e => setAddText(e.target.value.toUpperCase())}
          onKeyDown={e => { if (e.key === "Enter" && addText.trim()) { act("add", addText.trim()); setAddText(""); } }}
          placeholder="เพิ่มหุ้น เช่น NVDA, AAPL, TSLA"
        />
        <button onClick={() => { if (addText.trim()) { act("add", addText.trim()); setAddText(""); } }}>เพิ่ม</button>
      </div>

      <table className="screener-table">
        <thead><tr><th>#</th><th>สัญลักษณ์</th><th></th><th></th></tr></thead>
        <tbody>
          {active && active.symbols.length > 0 ? (
            active.symbols.map((sym, i) => (
              <tr key={sym}>
                <td className="num">{i + 1}</td>
                <td><b>{sym}</b></td>
                <td className="wl-actions">
                  <button title="ขึ้น" onClick={() => act("up", sym)}>↑</button>
                  <button title="ลง" onClick={() => act("down", sym)}>↓</button>
                  <button title="ลบ" className="wl-danger" onClick={() => act("remove", sym)}>✕</button>
                </td>
                <td>
                  <button className="wl-open" onClick={() => onOpenRadar(sym)}>เปิด Radar →</button>
                </td>
              </tr>
            ))
          ) : (
            <tr><td colSpan={4} className="pf-empty">ลิสต์ว่าง — เพิ่มหุ้นด้านบน (หน้าแรก/quotes หลายหุ้นมาใน phase ถัดไป)</td></tr>
          )}
        </tbody>
      </table>
      <small className="pc-note">เก็บไว้ในเครื่อง (ไม่มี watchlist API ฝั่ง Webull) · การเพิ่มจะตรวจรูปแบบสัญลักษณ์ · กด Enter เพิ่มได้</small>
    </div>
  );
}
