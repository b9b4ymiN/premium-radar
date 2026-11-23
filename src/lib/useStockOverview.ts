/** Company facts for the scanned symbol (profile, earnings window, dividends,
 *  analysts). Five paced calls behind one command, so the answer is reused for
 *  ten minutes on the Rust side; a symbol change drops any answer in flight. */

import { useEffect, useState } from "react";
import type { StockOverview } from "../types/webull";

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface OverviewState {
  overview: StockOverview | null;
  /** RFC 3339 moment the answer was really fetched. */
  fetchedAt: string | null;
  loading: boolean;
  error: string | null;
}

const EMPTY: OverviewState = { overview: null, fetchedAt: null, loading: false, error: null };

export function useStockOverview(symbol: string): OverviewState {
  const [state, setState] = useState<OverviewState>(EMPTY);

  useEffect(() => {
    let live = true;
    if (!hasTauri) {
      setState(EMPTY);
      return;
    }
    setState({ overview: null, fetchedAt: null, loading: true, error: null });
    import("@tauri-apps/api/core")
      .then(({ invoke }) =>
        invoke<{ overview: StockOverview; fetched_at: string }>("get_stock_overview", { symbol }),
      )
      .then(answer => {
        if (!live) return;
        setState({ overview: answer.overview, fetchedAt: answer.fetched_at, loading: false, error: null });
      })
      .catch((e: unknown) => {
        if (!live) return;
        setState({
          overview: null,
          fetchedAt: null,
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      live = false;
    };
  }, [symbol]);

  return state;
}
