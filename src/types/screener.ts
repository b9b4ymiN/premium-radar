/** Shared TypeScript types matching the Rust engine payloads. */

export type OptType = 'C' | 'P';
export type Grade = 'A' | 'B' | 'C';
export type DataSource = 'สด' | 'API' | 'ราคาปิด' | 'DEMO';

export interface QuoteRow {
  symbol: string;
  underlying: string;
  expiry: string; // ISO date
  option_type: OptType;
  strike: number;
  price: number;
  bid: number;
  ask: number;
  volume: number;
  oi: number;
  change: number;
  change_ratio: number;
  src: string;
}

export interface EnrichedRow {
  q: QuoteRow;
  t: number;
  dte: number;
  iv: number | null;
  delta: number | null;
  gamma: number | null;
  vega: number | null;
  theta: number | null;
  pop: number | null;
  credit: number | null;
  collateral: number | null;
  yld: number | null;
  ann: number | null;
  be: number | null;
  cushion: number | null;
  p_itm: number | null;
  p_touch: number | null;
  score: number | null;
  spread_pct: number | null;
  prem: number | null;
  net_cash: number | null;
  max_profit: number | null;
  grade: Grade | null;
  why: string | null;
}

export interface StockUpdate {
  price: number;
  open: number;
  high: number;
  low: number;
  pre_close: number;
  volume: number;
  change: number;
  change_ratio: number;
  src: string;
}

export interface ChainStats {
  atm_iv: number | null;
  em: number | null;
  skew: number | null;
}

export interface EngineSnapshot {
  rows: EnrichedRow[];
  candidates: EnrichedRow[];
  spot: number | null;
  stock: StockUpdate | null;
  stats: ChainStats | null;
}

export type StrategyMode = 'CSP+CC' | 'CSP' | 'CC';
export type RiskLevel = 'SAFE' | 'BALANCED' | 'AGGR' | 'ALL';

export interface Filters {
  strategy: StrategyMode;
  risk: RiskLevel;
  expiry: string | null; // ISO date or null = all
  budget: number; // 0 = unlimited
}
