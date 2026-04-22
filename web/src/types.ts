export type BotStatus = "idle" | "running" | "stopping";
export type TradingMode = "paper" | "live";
export type LogLevel = "info" | "signal" | "order" | "risk" | "error" | "warn";

export interface BotStatusResponse {
  status: BotStatus;
  mode: TradingMode;
}

export interface PnlResponse {
  today: number;
  month: number;
  year: number;
}

export interface Position {
  contract: string;
  underlying: string;
  direction: string;
  qty: number;
  entry_price: number;
  current_price: number;
  market_value: number;
  unrealized_pl: number;
  unrealized_plpc: number;
  opened_at: string;
}

export interface Fill {
  id: number | null;
  symbol: string;
  side: "buy" | "sell";
  qty: number;
  price: number;
  timestamp: string;
  order_id: string;
}

export interface LogEvent {
  timestamp: string;
  level: LogLevel;
  message: string;
}

export interface PdtResponse {
  used: number;
  max: number;
}

export interface ClockResponse {
  is_open: boolean;
  next_change: string;
}

export interface EquityResponse {
  timestamps: number[];
  equity: number[];
}

export interface ApiOkResponse {
  ok: boolean;
}

export interface ModeResponse {
  ok: boolean;
  restart_required: boolean;
}
