import { useEffect, useRef, useCallback, useReducer } from "react";
import { api, createSSEConnection } from "../api";
import type {
  BotStatusResponse,
  PnlResponse,
  Position,
  Fill,
  LogEvent,
  PdtResponse,
  ClockResponse,
  EquityResponse,
  StrategyInfo,
} from "../types";

const MAX_LOG_ENTRIES = 500;

interface DashboardState {
  botStatus: BotStatusResponse | null;
  pnl: PnlResponse | null;
  positions: Position[];
  fills: Fill[];
  pdt: PdtResponse | null;
  clock: ClockResponse | null;
  equity: EquityResponse | null;
  strategies: StrategyInfo[];
  logs: LogEvent[];
  newLogIds: Set<number>;
  error: string | null;
}

type DashboardAction =
  | { type: "SET_STATUS"; payload: BotStatusResponse }
  | { type: "SET_PNL"; payload: PnlResponse }
  | { type: "SET_POSITIONS"; payload: Position[] }
  | { type: "SET_FILLS"; payload: Fill[] }
  | { type: "SET_PDT"; payload: PdtResponse }
  | { type: "SET_CLOCK"; payload: ClockResponse }
  | { type: "SET_EQUITY"; payload: EquityResponse }
  | { type: "SET_STRATEGIES"; payload: StrategyInfo[] }
  | { type: "SET_LOGS"; payload: LogEvent[] }
  | { type: "APPEND_LOG"; payload: LogEvent }
  | { type: "CLEAR_NEW_LOG"; payload: number }
  | { type: "SET_ERROR"; payload: string };

let logCounter = 0;

interface IndexedLogEvent extends LogEvent {
  _idx: number;
}

interface IndexedDashboardState extends Omit<DashboardState, "logs"> {
  logs: IndexedLogEvent[];
}

function reducer(
  state: IndexedDashboardState,
  action: DashboardAction
): IndexedDashboardState {
  switch (action.type) {
    case "SET_STATUS":
      return { ...state, botStatus: action.payload };
    case "SET_PNL":
      return { ...state, pnl: action.payload };
    case "SET_POSITIONS":
      return { ...state, positions: action.payload };
    case "SET_FILLS":
      return { ...state, fills: action.payload };
    case "SET_PDT":
      return { ...state, pdt: action.payload };
    case "SET_CLOCK":
      return { ...state, clock: action.payload };
    case "SET_EQUITY":
      return { ...state, equity: action.payload };
    case "SET_STRATEGIES":
      return { ...state, strategies: action.payload };
    case "SET_LOGS": {
      const indexed = action.payload.map((l) => ({
        ...l,
        _idx: logCounter++,
      }));
      return { ...state, logs: indexed, newLogIds: new Set() };
    }
    case "APPEND_LOG": {
      const idx = logCounter++;
      const newLog: IndexedLogEvent = { ...action.payload, _idx: idx };
      const logs = [...state.logs, newLog].slice(-MAX_LOG_ENTRIES);
      const newLogIds = new Set(state.newLogIds);
      newLogIds.add(idx);
      return { ...state, logs, newLogIds };
    }
    case "CLEAR_NEW_LOG": {
      const newLogIds = new Set(state.newLogIds);
      newLogIds.delete(action.payload);
      return { ...state, newLogIds };
    }
    case "SET_ERROR":
      return { ...state, error: action.payload };
    default:
      return state;
  }
}

const initialState: IndexedDashboardState = {
  botStatus: null,
  pnl: null,
  positions: [],
  fills: [],
  pdt: null,
  clock: null,
  equity: null,
  strategies: [],
  logs: [],
  newLogIds: new Set(),
  error: null,
};

export interface DashboardData extends Omit<IndexedDashboardState, "logs"> {
  logs: IndexedLogEvent[];
  refresh: {
    status: () => Promise<void>;
    positions: () => Promise<void>;
    fills: () => Promise<void>;
    strategies: () => Promise<void>;
  };
}

export type { IndexedLogEvent };

export function useDashboard(): DashboardData {
  const [state, dispatch] = useReducer(reducer, initialState);
  const mountedRef = useRef(true);

  const safeDispatch = useCallback(
    (action: DashboardAction) => {
      if (mountedRef.current) dispatch(action);
    },
    []
  );

  const fetchStatus = useCallback(async () => {
    try {
      const data = await api.getStatus();
      safeDispatch({ type: "SET_STATUS", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchPositions = useCallback(async () => {
    try {
      const data = await api.getPositions();
      safeDispatch({ type: "SET_POSITIONS", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchFills = useCallback(async () => {
    try {
      const data = await api.getFills();
      safeDispatch({ type: "SET_FILLS", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchPnl = useCallback(async () => {
    try {
      const data = await api.getPnl();
      safeDispatch({ type: "SET_PNL", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchPdt = useCallback(async () => {
    try {
      const data = await api.getPdt();
      safeDispatch({ type: "SET_PDT", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchClock = useCallback(async () => {
    try {
      const data = await api.getClock();
      safeDispatch({ type: "SET_CLOCK", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  const fetchStrategies = useCallback(async () => {
    try {
      const data = await api.getStrategies();
      safeDispatch({ type: "SET_STRATEGIES", payload: data });
    } catch (e) {
      safeDispatch({ type: "SET_ERROR", payload: String(e) });
    }
  }, [safeDispatch]);

  useEffect(() => {
    mountedRef.current = true;

    // Initial fetches
    fetchStatus();
    fetchPositions();
    fetchFills();
    fetchPnl();
    fetchPdt();
    fetchClock();
    fetchStrategies();

    // Fetch equity once on mount
    api
      .getEquity("1M")
      .then((data) => safeDispatch({ type: "SET_EQUITY", payload: data }))
      .catch(() => {});

    // Initial log fetch
    api
      .getLogs(200)
      .then((data) => safeDispatch({ type: "SET_LOGS", payload: data }))
      .catch(() => {});

    // Polling intervals
    const intervals: ReturnType<typeof setInterval>[] = [
      setInterval(fetchStatus, 5_000),
      setInterval(fetchPositions, 10_000),
      setInterval(fetchPdt, 10_000),
      setInterval(fetchFills, 15_000),
      setInterval(fetchStrategies, 15_000),
      setInterval(fetchPnl, 30_000),
      setInterval(fetchClock, 60_000),
    ];

    // SSE for live logs
    const es = createSSEConnection((event) => {
      safeDispatch({ type: "APPEND_LOG", payload: event });
    });

    return () => {
      mountedRef.current = false;
      intervals.forEach(clearInterval);
      es.close();
    };
  }, [fetchStatus, fetchPositions, fetchFills, fetchPnl, fetchPdt, fetchClock, fetchStrategies, safeDispatch]);

  return {
    ...state,
    refresh: {
      status: fetchStatus,
      positions: fetchPositions,
      fills: fetchFills,
      strategies: fetchStrategies,
    },
  };
}
