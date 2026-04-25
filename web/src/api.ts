import type {
  BotStatusResponse,
  PnlResponse,
  Position,
  Fill,
  LogEvent,
  PdtResponse,
  ClockResponse,
  EquityResponse,
  ApiOkResponse,
  ModeResponse,
  TradingMode,
  StrategyInfo,
  TickerEntry,
} from "./types";

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, init);
  if (!res.ok) {
    throw new Error(`HTTP ${res.status}: ${res.statusText}`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  getStatus: () => fetchJson<BotStatusResponse>("/api/status"),

  getPnl: () => fetchJson<PnlResponse>("/api/pnl"),

  getPositions: () => fetchJson<Position[]>("/api/positions"),

  getFills: () => fetchJson<Fill[]>("/api/fills"),

  getPdt: () => fetchJson<PdtResponse>("/api/pdt"),

  getClock: () => fetchJson<ClockResponse>("/api/clock"),

  getLogs: (limit = 200) =>
    fetchJson<LogEvent[]>(`/api/logs?limit=${limit}`),

  getEquity: (period = "1M") =>
    fetchJson<EquityResponse>(`/api/equity?period=${period}`),

  start: () =>
    fetchJson<ApiOkResponse>("/api/start", { method: "POST" }),

  stop: () =>
    fetchJson<ApiOkResponse>("/api/stop", { method: "POST" }),

  setMode: (mode: TradingMode) =>
    fetchJson<ModeResponse>("/api/mode", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mode }),
    }),

  getStrategies: () => fetchJson<StrategyInfo[]>("/api/strategies"),

  setStrategyEnabled: (id: string, enabled: boolean) =>
    fetchJson<ApiOkResponse>(`/api/strategies/${encodeURIComponent(id)}/enabled`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ enabled }),
    }),

  getTicker: () => fetchJson<TickerEntry[]>("/api/ticker"),
};

export function createSSEConnection(
  onEvent: (event: LogEvent) => void,
  onError?: (err: Event) => void
): EventSource {
  const es = new EventSource("/api/stream");
  es.onmessage = (e: MessageEvent) => {
    try {
      const data = JSON.parse(e.data as string) as LogEvent;
      onEvent(data);
    } catch {
      // ignore malformed events
    }
  };
  if (onError) {
    es.onerror = onError;
  }
  return es;
}
