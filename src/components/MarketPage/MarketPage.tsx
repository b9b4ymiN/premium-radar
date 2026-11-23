//! Market discovery — the broker's screener ranks. A discovered symbol opens in
//! the scanner; nothing here trades. Every number comes from Rust as Webull
//! reported it; the page only lays the rows out.

import { useCallback, useEffect, useRef, useState } from "react";
import type { MoverList, MoverRow, SectorRow } from "../../types/webull";
import { compact, count, fmt, pct, signedPct, signOf, timeEt } from "../../lib/format";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface MoversAnswer {
  rows: MoverRow[];
  fetched_at: string;
}

interface SectorsAnswer {
  rows: SectorRow[];
  fetched_at: string;
}

const LISTS: Array<{ id: MoverList; label: string; basis: string }> = [
  { id: "gainers", label: "ขึ้นแรงวันนี้", basis: "เรียงจาก % เปลี่ยนแปลงของ session ล่าสุด มากสุดก่อน" },
  { id: "losers", label: "ลงแรงวันนี้", basis: "เรียงจาก % เปลี่ยนแปลงของ session ล่าสุด ลดมากสุดก่อน" },
  { id: "most_active", label: "ซื้อขายหนาแน่น", basis: "เรียงจากวอลุ่มสูงสุดก่อน" },
  { id: "week52_high", label: "จุดสูงสุด 52 สัปดาห์", basis: "หุ้นที่ทำราคาสูงสุดในรอบ 52 สัปดาห์" },
  { id: "week52_low", label: "จุดต่ำสุด 52 สัปดาห์", basis: "หุ้นที่ทำราคาต่ำสุดในรอบ 52 สัปดาห์" },
  {
    id: "high_dividend",
    label: "ปันผลสูง",
    basis: "เรียงจาก dividend yield สูงสุดก่อน — yield สูงผิดปกติมักมาจากราคาที่ลดฮวย อ่านก่อนเชื่อ",
  },
];

export default function MarketPage({ onOpenRadar }: { onOpenRadar: (symbol: string) => void }) {
  const [tab, setTab] = useState<MoverList | "sectors">("gainers");
  // drill-down inside the sectors tab
  const [sector, setSector] = useState<SectorRow | null>(null);
  const [movers, setMovers] = useState<MoverRow[]>([]);
  const [sectors, setSectors] = useState<SectorRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [fetchedAt, setFetchedAt] = useState<string | null>(null);
  // an answer for a tab the user already left is dropped
  const request = useRef(0);
  // the refresh button asks Webull again instead of taking the cached rank
  const bypassCache = useRef(false);
  const [nonce, setNonce] = useState(0);

  const run = useCallback(<T,>(start: () => Promise<T>, apply: (value: T) => void) => {
    if (!hasTauri) {
      setError("เปิดแอปเดสก์ท็อปเพื่อดูข้อมูลตลาดจาก Webull");
      return;
    }
    const id = ++request.current;
    setLoading(true);
    setError(null);
    start()
      .then(value => {
        if (id === request.current) apply(value);
      })
      .catch((e: unknown) => {
        if (id === request.current) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (id === request.current) setLoading(false);
      });
  }, []);

  useEffect(() => {
    const force = bypassCache.current;
    bypassCache.current = false;
    setMovers([]);
    setSectors([]);
    setFetchedAt(null);
    if (tab === "sectors") {
      if (sector) {
        const picked = sector;
        run(
          () => invokeCmd<MoversAnswer>("get_sector_detail", { sectorId: picked.id, refresh: force }),
          answer => {
            setMovers(answer.rows ?? []);
            setFetchedAt(answer.fetched_at);
          },
        );
      } else {
        run(() => invokeCmd<SectorsAnswer>("get_sectors", { refresh: force }), answer => {
          setSectors(answer.rows ?? []);
          setFetchedAt(answer.fetched_at);
        });
      }
      return;
    }
    const list = tab;
    run(() => invokeCmd<MoversAnswer>("get_movers", { list, refresh: force }), answer => {
      setMovers(answer.rows ?? []);
      setFetchedAt(answer.fetched_at);
    });
  }, [tab, sector, run, nonce]);

  const reload = () => {
    bypassCache.current = true;
    setNonce(value => value + 1);
  };
  const pickTab = (next: MoverList | "sectors") => {
    setSector(null);
    setTab(next);
  };

  const current = LISTS.find(entry => entry.id === tab);
  const showSectors = tab === "sectors" && !sector;
  const subtitle = sector
    ? `หุ้นใน ${sector.name ?? "sector นี้"} เรียงตาม % เปลี่ยนแปลง`
    : showSectors
      ? "Sector ตามมูลค่าตลาด · กดแถวเพื่อดูหุ้นใน sector"
      : (current?.basis ?? "");
  const columns = tab === "high_dividend" ? 10 : 9;

  return (
    <div className="marketpage">
      <div className="pf-head">
        <div>
          <div className="pf-title">
            Market · ภาพรวมตลาด US
            {sector && <small> › {sector.name ?? "sector"}</small>}
          </div>
          <div className="pf-sub">{subtitle}</div>
        </div>
        <div className="mk-actions">
          {sector && (
            <button className="pf-refresh" onClick={() => setSector(null)}>
              ‹ กลับไป sector
            </button>
          )}
          <button className="pf-refresh" onClick={reload} disabled={loading}>
            {loading ? "กำลังโหลด…" : "↻ รีเฟรช"}
          </button>
        </div>
      </div>

      <div className="mk-tabs" role="tablist" aria-label="หมวดของตลาด">
        {LISTS.map(entry => (
          <button
            key={entry.id}
            role="tab"
            aria-selected={tab === entry.id}
            className={`t-tab ${tab === entry.id ? "on" : ""}`}
            onClick={() => pickTab(entry.id)}
          >
            {entry.label}
          </button>
        ))}
        <button
          role="tab"
          aria-selected={tab === "sectors"}
          className={`t-tab ${tab === "sectors" ? "on" : ""}`}
          onClick={() => pickTab("sectors")}
        >
          Sectors
        </button>
      </div>

      {error && (
        <div className="pf-err" role="alert">
          ⚠ {error}
          {hasTauri && (
            <button className="wl-open" onClick={reload}>
              ลองใหม่
            </button>
          )}
        </div>
      )}

      {error ? null : showSectors ? (
        <div className="hp-tablewrap">
          <table className="screener-table mk-table mk-sectors">
            <thead>
              <tr>
                <th>Sector</th>
                <th className="num">เปลี่ยน %</th>
                <th className="num">ขึ้น</th>
                <th className="num">ลง</th>
                <th className="num">คงที่</th>
                <th>หุ้นนำ</th>
                <th className="num">มูลค่าตลาด</th>
              </tr>
            </thead>
            <tbody>
              {sectors.length === 0 ? (
                <tr className="empty-row">
                  <td colSpan={7} className="pf-empty">
                    {loading ? "กำลังดึงข้อมูลจาก Webull …" : "Webull ไม่มีข้อมูล sector"}
                  </td>
                </tr>
              ) : (
                sectors.map(row => (
                  <tr key={row.id} className="hp-row" onClick={() => setSector(row)}>
                    <td className="mk-name">
                      <button
                        type="button"
                        className="row-link"
                        onClick={event => {
                          event.stopPropagation();
                          setSector(row);
                        }}
                      >
                        {row.name ?? row.id}
                      </button>
                    </td>
                    <td className={`num ${signOf(row.change_ratio)}`}>{signedPct(row.change_ratio)}</td>
                    <td className="num pos">{count(row.advanced)}</td>
                    <td className="num neg">{count(row.declined)}</td>
                    <td className="num">{count(row.flat)}</td>
                    <td className="mk-name">
                      {row.leader
                        ? `${row.leader.symbol}${row.leader.name ? ` · ${row.leader.name}` : ""}`
                        : "--"}
                    </td>
                    <td className="num">{compact(row.market_value)}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      ) : (
        <MoversTable
          rows={movers}
          loading={loading && movers.length === 0}
          showDividend={tab === "high_dividend"}
          columns={columns}
          onOpen={onOpenRadar}
        />
      )}

      <small className="pc-note">
        Webull screener{fetchedAt ? ` · ดึงเมื่อ ${timeEt(Date.parse(fetchedAt))}` : ""}
        {" · "}อัปเดตได้ทุก 1 นาที · กดแถวเพื่อเปิดใน Options Monitor
      </small>
    </div>
  );
}

function MoversTable({
  rows,
  loading,
  showDividend,
  columns,
  onOpen,
}: {
  rows: MoverRow[];
  loading: boolean;
  showDividend: boolean;
  columns: number;
  onOpen: (symbol: string) => void;
}) {
  return (
    <div className="hp-tablewrap">
      <table className="screener-table mk-table">
        <thead>
          <tr>
            <th>#</th>
            <th>สัญลักษณ์</th>
            <th>ชื่อ</th>
            <th className="num">ราคาล่าสุด</th>
            <th className="num">เปลี่ยน %</th>
            <th className="num">วอลุ่ม</th>
            <th className="num">Vol. เท่ากับ 10 วัน</th>
            <th className="num">มูลค่าตลาด</th>
            <th className="num">P/E</th>
            {showDividend && <th className="num">Dividend yield</th>}
          </tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr className="empty-row">
              <td colSpan={columns} className="pf-empty">
                {loading ? "กำลังดึงข้อมูลจาก Webull …" : "Webull ไม่มีข้อมูลสำหรับลิสต์นี้"}
              </td>
            </tr>
          ) : (
            rows.map((row, index) => (
              <tr key={row.symbol} className="hp-row" onClick={() => onOpen(row.symbol)}>
                <td>{index + 1}</td>
                <td>
                  <button
                    type="button"
                    className="row-link"
                    onClick={event => {
                      event.stopPropagation();
                      onOpen(row.symbol);
                    }}
                  >
                    {row.symbol}
                  </button>
                </td>
                <td className="mk-name">{row.name ?? "--"}</td>
                <td className="num">{row.price == null ? "--" : fmt(row.price)}</td>
                <td className={`num ${signOf(row.change_ratio)}`}>{signedPct(row.change_ratio)}</td>
                <td className="num">{compact(row.volume)}</td>
                <td className="num">{row.relative_volume_10d == null ? "--" : `${fmt(row.relative_volume_10d, 2)}×`}</td>
                <td className="num">{compact(row.market_value)}</td>
                <td className="num">{row.pe_ttm == null ? "--" : fmt(row.pe_ttm, 1)}</td>
                {showDividend && (
                  <td className="num pos">
                    {pct(row.dividend_yield == null ? null : row.dividend_yield / 100)}
                  </td>
                )}
              </tr>
            ))
          )}
        </tbody>
      </table>
    </div>
  );
}
