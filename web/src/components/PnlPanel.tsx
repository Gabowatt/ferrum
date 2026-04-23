import { useState } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import type { PnlResponse, EquityResponse } from "../types";
import { ParrotAnimation } from "./ParrotAnimation";

interface PnlPanelProps {
  pnl: PnlResponse | null;
  equity: EquityResponse | null;
}

function formatDollar(n: number): string {
  const abs = Math.abs(n);
  const sign = n >= 0 ? "+" : "-";
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(2)}M`;
  }
  if (abs >= 1_000) {
    return `${sign}$${(abs / 1_000).toFixed(1)}k`;
  }
  return `${sign}$${abs.toFixed(2)}`;
}

function plClass(n: number): string {
  if (n > 0) return "pnl-positive";
  if (n < 0) return "pnl-negative";
  return "pnl-zero";
}

function plArrow(n: number): string {
  if (n > 0) return "↑";
  if (n < 0) return "↓";
  return "→";
}

interface ChartDatum {
  date: string;
  equity: number;
}

function buildChartData(equity: EquityResponse): ChartDatum[] {
  return equity.timestamps.map((ts, i) => ({
    date: new Date(ts * 1000).toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
    }),
    equity: equity.equity[i],
  }));
}

interface CustomTooltipProps {
  active?: boolean;
  payload?: Array<{ value: number }>;
  label?: string;
}

function CustomTooltip({ active, payload, label }: CustomTooltipProps) {
  if (!active || !payload || payload.length === 0) return null;
  const val = payload[0].value;
  return (
    <div className="chart-tooltip">
      <div className="chart-tooltip__label">{label}</div>
      <div className="chart-tooltip__value">
        {val.toLocaleString("en-US", {
          style: "currency",
          currency: "USD",
          minimumFractionDigits: 0,
          maximumFractionDigits: 0,
        })}
      </div>
    </div>
  );
}

export function PnlPanel({ pnl, equity }: PnlPanelProps) {
  // Local-only — privacy/morale toggle. When true the panel swaps its body
  // for an animated parrot.live party parrot. Persisted across reloads so the
  // user's preference survives a refresh; sessionStorage is intentional (we
  // don't want it sticky forever, but a hard refresh shouldn't reveal the P&L).
  const [hidden, setHidden] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.sessionStorage.getItem("ferrum.pnlHidden") === "1";
  });

  function toggleHidden() {
    setHidden((prev) => {
      const next = !prev;
      try {
        window.sessionStorage.setItem("ferrum.pnlHidden", next ? "1" : "0");
      } catch {
        // private mode / quota — non-fatal
      }
      return next;
    });
  }

  const chartData = equity ? buildChartData(equity) : [];

  const cards: { label: string; value: number | null }[] = [
    { label: "Today", value: pnl?.today ?? null },
    { label: "Month", value: pnl?.month ?? null },
    { label: "Year", value: pnl?.year ?? null },
  ];

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">P&amp;L</span>
        <button
          type="button"
          role="switch"
          aria-checked={!hidden}
          aria-label={hidden ? "Show P&L" : "Hide P&L"}
          title={hidden ? "Show P&L" : "Hide P&L"}
          className={`toggle-switch ${hidden ? "toggle-switch--off" : "toggle-switch--on"}`}
          onClick={toggleHidden}
        >
          <span className="toggle-switch__knob" />
        </button>
      </div>

      {hidden ? (
        <ParrotAnimation />
      ) : (
        <>
      <div className="pnl-cards">
        {cards.map(({ label, value }) => (
          <div key={label} className="pnl-card">
            <div className="pnl-card__label">{label}</div>
            {value === null ? (
              <div className="pnl-card__value pnl-zero">—</div>
            ) : (
              <div className={`pnl-card__value ${plClass(value)}`}>
                {formatDollar(value)}
                <span className="pnl-card__arrow">{plArrow(value)}</span>
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="equity-chart-wrap">
        {chartData.length === 0 ? (
          <div className="empty-state" style={{ height: 160 }}>
            No equity data
          </div>
        ) : (
          <ResponsiveContainer width="100%" height={160}>
            <AreaChart
              data={chartData}
              margin={{ top: 4, right: 16, left: 0, bottom: 0 }}
            >
              <defs>
                <linearGradient id="equityGrad" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%"  stopColor="#bb9af7" stopOpacity={0.3} />
                  <stop offset="95%" stopColor="#bb9af7" stopOpacity={0.02} />
                </linearGradient>
              </defs>
              <CartesianGrid
                strokeDasharray="3 3"
                stroke="rgba(46,46,46,0.8)"
                vertical={false}
              />
              <XAxis
                dataKey="date"
                tick={{ fill: "#555555", fontSize: 9 }}
                axisLine={false}
                tickLine={false}
                interval="preserveStartEnd"
              />
              <YAxis
                tick={{ fill: "#555555", fontSize: 9 }}
                axisLine={false}
                tickLine={false}
                width={55}
                tickFormatter={(v: number) =>
                  v >= 1000
                    ? `$${(v / 1000).toFixed(0)}k`
                    : `$${v.toFixed(0)}`
                }
              />
              <Tooltip content={<CustomTooltip />} />
              <Area
                type="monotone"
                dataKey="equity"
                stroke="#bb9af7"
                strokeWidth={1.5}
                fill="url(#equityGrad)"
                dot={false}
                activeDot={{ r: 4, fill: "#bb9af7", strokeWidth: 0 }}
              />
            </AreaChart>
          </ResponsiveContainer>
        )}
      </div>
        </>
      )}
    </div>
  );
}
