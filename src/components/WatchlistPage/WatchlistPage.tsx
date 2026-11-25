//! หน้า Watchlists (PLAN-APP2 P16) — list ในเครื่อง (CRUD ผ่าน Rust) และ watchlist
//! ฝั่ง Webull แบบอ่านอย่างเดียว: ดูได้และคัดลอกหุ้นเข้า list ในเครื่อง แต่แอปไม่แก้
//! ของโบรกเกอร์

import { useCallback, useEffect, useRef, useState } from "react";
import type { BrokerInstrument, BrokerWatchlist } from "../../types/webull";
import { timeTh } from "../../lib/format";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Exchange codes of US listings, the only ones this scanner has option
 *  chains for (seen from the host: `NYSE`, `NAS`). A row without an exchange
 *  is left usable: the scanner says so itself if the symbol is not US. */
const US_EXCHANGES = ["NYSE", "NYS", "NAS", "NASDAQ", "NSD", "AMEX", "ASE", "ARCA", "PCX", "BATS", "OTC", "PNK"];
const hasOptionsChain = (exchangeCode: string | null) =>
  exchangeCode == null || US_EXCHANGES.includes(exchangeCode.toUpperCase());

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface List { id: string; name: string; symbols: string[] }
interface Store { lists: List[]; active: string | null }

/** The watchlists the broker holds. Reading only; the one action copies a
 *  symbol into the local list that is open above. */
function BrokerWatchlists({
  localName,
  localSymbols,
  onCopy,
  onOpenRadar,
}: {
  localName: string | undefined;
  localSymbols: string[];
  onCopy: (symbol: string) => void;
  onOpenRadar: (symbol: string) => void;
}) {
  const [lists, setLists] = useState<BrokerWatchlist[]>([]);
  const [fetchedAt, setFetchedAt] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [instruments, setInstruments] = useState<BrokerInstrument[] | null>(null);
  const [instrumentsLoading, setInstrumentsLoading] = useState(false);
  const [instrumentsError, setInstrumentsError] = useState<string | null>(null);
  // the refresh button asks the broker again instead of taking the cached answer
  const [nonce, setNonce] = useState(0);
  const refresh = useRef(false);
  const request = useRef(0);

  useEffect(() => {
    let live = true;
    if (!hasTauri) return;
    const force = refresh.current;
    refresh.current = false;
    setLoading(true);
    setError(null);
    invokeCmd<{ lists: BrokerWatchlist[]; fetched_at: string }>("get_broker_watchlists", { refresh: force })
      .then(answer => {
        if (!live) return;
        setLists(answer.lists ?? []);
        setFetchedAt(answer.fetched_at);
      })
      .catch((e: unknown) => {
        if (live) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => { live = false; };
  }, [nonce]);

  const toggle = (id: string) => {
    if (open === id) {
      setOpen(null);
      return;
    }
    // an answer for a list the user already closed is dropped
    const current = ++request.current;
    setOpen(id);
    setInstruments(null);
    setInstrumentsError(null);
    setInstrumentsLoading(true);
    invokeCmd<{ instruments: BrokerInstrument[] }>("get_broker_watchlist", { watchlistId: id })
      .then(answer => {
        if (current === request.current) setInstruments(answer.instruments ?? []);
      })
      .catch((e: unknown) => {
        if (current === request.current) setInstrumentsError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (current === request.current) setInstrumentsLoading(false);
      });
  };

  const openName = lists.find(list => list.id === open)?.name ?? "";

  return (
    <section className="brokerwl" aria-label="Watchlist ใน Webull (อ่านอย่างเดียว)">
      <div className="bwl-head">
        <div className="section-title">
          WATCHLIST ใน WEBULL · อ่านอย่างเดียว
          {fetchedAt && <span>ดึง {timeTh(Date.parse(fetchedAt))}</span>}
        </div>
        {hasTauri && (
          <button type="button" className="wl-open" onClick={() => setNonce(value => value + 1)} disabled={loading}>
            {loading ? "กำลังโหลด…" : "↻ รีเฟรช"}
          </button>
        )}
      </div>

      {!hasTauri ? (
        <div className="pc-note">เปิดผ่านแอปเดสก์ท็อปเพื่อดู watchlist ฝั่ง Webull</div>
      ) : error ? (
        <div className="pf-err" role="alert">⚠ {error}</div>
      ) : lists.length === 0 ? (
        <div className="pc-note">{loading ? "กำลังดึง…" : "Webull ไม่มี watchlist ให้แสดง"}</div>
      ) : (
        <div className="bwl-lists">
          {lists.map(list => (
            <button
              key={list.id}
              type="button"
              className={`bwl-chip ${open === list.id ? "on" : ""}`}
              aria-expanded={open === list.id}
              onClick={() => toggle(list.id)}
            >
              {list.name}
            </button>
          ))}
        </div>
      )}

      {open && (
        <div className="bwl-body">
          {instrumentsLoading && <div className="pc-note">กำลังดึงรายการของ “{openName}” …</div>}
          {instrumentsError && <div className="pf-err" role="alert">⚠ {instrumentsError}</div>}
          {instruments && instruments.length === 0 && <div className="pc-note">“{openName}” ว่าง</div>}
          {instruments && instruments.length > 0 && (
            <div className="hp-tablewrap">
              <table className="screener-table">
                <thead>
                  <tr><th>#</th><th>สัญลักษณ์</th><th>ชื่อ</th><th>ตลาด</th><th></th><th></th></tr>
                </thead>
                <tbody>
                  {instruments.map((row, index) => {
                    const copied = localSymbols.includes(row.symbol);
                    if (!hasOptionsChain(row.exchange_code)) {
                      return (
                        <tr key={`${row.symbol}-${index}`}>
                          <td className="num">{index + 1}</td>
                          <td><b>{row.symbol}</b></td>
                          <td className="mk-name">{row.name ?? "--"}</td>
                          <td className="num">{row.exchange_code ?? "--"}</td>
                          <td colSpan={2}>
                            <span className="bwl-copied" title="เรดาร์นี้สแกนหุ้นที่จดทะเบียนในสหรัฐฯ เท่านั้น">
                              ตลาด {row.exchange_code} — นอกขอบเขตของเรดาร์
                            </span>
                          </td>
                        </tr>
                      );
                    }
                    return (
                      <tr key={`${row.symbol}-${index}`}>
                        <td className="num">{index + 1}</td>
                        <td><b>{row.symbol}</b></td>
                        <td className="mk-name">{row.name ?? "--"}</td>
                        <td className="num">{row.exchange_code ?? "--"}</td>
                        <td>
                          {copied ? (
                            <span className="bwl-copied">อยู่ใน list แล้ว</span>
                          ) : (
                            <button type="button" className="wl-open" onClick={() => onCopy(row.symbol)}>
                              คัดลอกเข้า “{localName ?? "list นี้"}”
                            </button>
                          )}
                        </td>
                        <td>
                          <button type="button" className="wl-open" onClick={() => onOpenRadar(row.symbol)}>เปิด Radar →</button>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      <small className="pc-note">แอปไม่ได้แก้ watchlist ฝั่ง Webull · ปุ่มคัดลอกเพิ่มหุ้นเข้า list ในเครื่องเท่านั้น · เรดาร์สแกนเฉพาะหุ้น US</small>
    </section>
  );
}

export default function WatchlistPage({
  onOpenRadar, onChanged,
}: { onOpenRadar: (symbol: string) => void; onChanged?: () => void }) {
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
      onChanged?.(); // ให้ left panel รีเฟรช quotes ทันที
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
            <tr><td colSpan={4} className="pf-empty">ลิสต์ว่าง — เพิ่มหุ้นด้านบน หรือคัดลอกจาก watchlist ใน Webull ด้านล่าง</td></tr>
          )}
        </tbody>
      </table>
      <small className="pc-note">เก็บไว้ในเครื่อง · การเพิ่มจะตรวจรูปแบบสัญลักษณ์ · กด Enter เพิ่มได้</small>

      <BrokerWatchlists
        localName={active?.name}
        localSymbols={active?.symbols ?? []}
        onCopy={symbol => act("add", symbol)}
        onOpenRadar={onOpenRadar}
      />
    </div>
  );
}
