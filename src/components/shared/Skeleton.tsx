import type { CSSProperties } from "react";

/** Loading placeholders — see src/styles/skeleton.css. */

export function Skel({ w = "4em", block = false, lg = false }: { w?: string; block?: boolean; lg?: boolean }) {
  return <span className={`skel${block ? " block" : ""}${lg ? " lg" : ""}`} style={{ width: w }} aria-hidden="true" />;
}

/** `rows` placeholder rows for a table with `cols` columns. */
export function SkelRows({ rows = 8, cols }: { rows?: number; cols: number }) {
  const widths = ["70%", "45%", "55%", "60%", "40%", "50%"];
  return (
    <>
      {Array.from({ length: rows }, (_, r) => (
        <tr key={r} className="skel-row" aria-hidden="true">
          {Array.from({ length: cols }, (_, c) => (
            <td key={c}>
              <Skel w={widths[(r + c) % widths.length]} />
            </td>
          ))}
        </tr>
      ))}
    </>
  );
}

/** Stacked lines standing in for a panel's content. */
export function SkelPanel({ lines = 5 }: { lines?: number }) {
  const widths = ["62%", "88%", "74%", "50%", "80%", "66%"];
  return (
    <div className="skel-panel" role="status" aria-label="กำลังโหลด">
      <Skel w="38%" lg block />
      {Array.from({ length: lines }, (_, i) => (
        <Skel key={i} w={widths[i % widths.length]} block />
      ))}
    </div>
  );
}

/** `n/total` in a scan progress message ("กำลังดึงราคาสัญญา 40/93"), or null. */
export function progressFraction(message: string): number | null {
  const m = /(\d+)\s*\/\s*(\d+)/.exec(message);
  if (!m) return null;
  const done = Number(m[1]);
  const total = Number(m[2]);
  return total > 0 ? Math.min(1, done / total) : null;
}

/** Thin progress line + caption while a symbol loads. */
export function LoadBar({ symbol, message }: { symbol: string; message: string }) {
  const p = progressFraction(message);
  return (
    <div role="status" aria-live="polite">
      <div
        className={`load-bar${p == null ? " indeterminate" : ""}`}
        style={p == null ? undefined : ({ "--load-p": `${Math.round(p * 100)}%` } as CSSProperties)}
      >
        <span />
      </div>
      <div className="load-caption">
        <b>{symbol}</b>
        <span>{message}</span>
      </div>
    </div>
  );
}
