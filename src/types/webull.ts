/** Payloads of the typed Webull clients in radar-core
 *  (`webull::{stocks, options, fundamentals, screeners, trading}`).
 *  Every number is nullable on purpose: a value the broker did not send is
 *  "unknown", never 0. */

export interface ExtendedQuote {
  last_price: number | null;
  /** Change against the regular-session close. */
  change: number | null;
  change_ratio: number | null;
  high: number | null;
  low: number | null;
  volume: number | null;
  last_trade_time_ms: number | null;
}

export interface StockSnapshot {
  symbol: string;
  price: number | null;
  open: number | null;
  high: number | null;
  low: number | null;
  pre_close: number | null;
  close: number | null;
  volume: number | null;
  change: number | null;
  change_ratio: number | null;
  bid: number | null;
  ask: number | null;
  bid_size: number | null;
  ask_size: number | null;
  /** Broker session tag, e.g. "PRE" during pre-market. */
  trade_status: string | null;
  quote_time_ms: number | null;
  last_trade_time_ms: number | null;
  market_value: number | null;
  pe_ratio: number | null;
  eps: number | null;
  eps_ttm: number | null;
  pb_ratio: number | null;
  /** Fraction: 0.007966 = 0.80 %. */
  dividend_yield: number | null;
  fifty_two_wk_high: number | null;
  fifty_two_wk_low: number | null;
  /** Null until an extended-hours trade exists. */
  extended: ExtendedQuote | null;
}

export interface OptionQuote {
  symbol: string;
  price: number | null;
  open: number | null;
  high: number | null;
  low: number | null;
  pre_close: number | null;
  close: number | null;
  volume: number | null;
  change: number | null;
  change_ratio: number | null;
  bid: number | null;
  ask: number | null;
  bid_size: number | null;
  ask_size: number | null;
  open_interest: number | null;
  strike_price: number | null;
  /** Fraction: 0.8511 = 85.11 %. */
  imp_vol: number | null;
  delta: number | null;
  gamma: number | null;
  theta: number | null;
  vega: number | null;
  rho: number | null;
  deal_amount: number | null;
  quote_time_ms: number | null;
  last_trade_time_ms: number | null;
  /** 0 = real-time; above 0 the feed is delayed by that many minutes. */
  delay_minutes: number | null;
}

export interface OptionTick {
  time_ms: number | null;
  price: number | null;
  volume: number | null;
  /** Broker aggressor code as returned (observed "NB", "NS", "N"). */
  side: string | null;
}

export interface MoverRow {
  symbol: string;
  name: string | null;
  price: number | null;
  /** Fraction: 1.449709 = +144.97 %. */
  change_ratio: number | null;
  volume: number | null;
  market_value: number | null;
  pe_ttm: number | null;
  relative_volume_10d: number | null;
  /** Percent as the screener reports it: 9.539 = 9.54 %. */
  dividend_yield: number | null;
  exchange_code: string | null;
  last_trade_time: string | null;
}

export interface SectorRow {
  id: string;
  name: string | null;
  change_ratio: number | null;
  advanced: number | null;
  declined: number | null;
  flat: number | null;
  volume: number | null;
  market_value: number | null;
  leader: MoverRow | null;
}

export interface CompanyProfile {
  company_name: string;
  industries: string[];
  profile: string | null;
  exhibition_code: string | null;
  ceo: string | null;
  employees: number | null;
  establish_date: string | null;
  address: string | null;
}

export interface AnalystTarget {
  mean: number | null;
  low: number | null;
  high: number | null;
  median: number | null;
  currency: string | null;
}

export interface AnalystRating {
  number: number | null;
  strong_buy: number | null;
  buy: number | null;
  hold: number | null;
  sell: number | null;
  under_perform: number | null;
}

/** The broker's next reporting window (ISO dates). */
export interface EarningsWindow {
  start_date: string;
  end_date: string;
  fiscal_year: number | null;
  fiscal_period: number | null;
  eps_est: number | null;
  rev_est: number | null;
}

export interface DividendEvent {
  amount: number | null;
  currency: string | null;
  div_type: string | null;
  ex_div_date: string | null;
  record_date: string | null;
  pay_date: string | null;
}

export interface DividendCalendar {
  upcoming: DividendEvent[];
  history: DividendEvent[];
}

export interface Unavailable {
  section: string;
  reason: string;
}

export interface StockOverview {
  profile: CompanyProfile | null;
  analyst_target: AnalystTarget | null;
  analyst_rating: AnalystRating | null;
  next_earnings: EarningsWindow | null;
  dividends: DividendCalendar | null;
  unavailable: Unavailable[];
}

export interface OpenOrder {
  client_order_id: string;
  combo_type: string | null;
  status: string | null;
  side: string | null;
  symbol: string | null;
  instrument_type: string | null;
  order_type: string | null;
  time_in_force: string | null;
  total_quantity: number | null;
  filled_quantity: number | null;
  limit_price: number | null;
  place_time: string | null;
  option_strategy: string | null;
}

export type MoverList =
  | "gainers"
  | "losers"
  | "most_active"
  | "week52_high"
  | "week52_low"
  | "high_dividend";
