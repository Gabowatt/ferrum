import type { Position } from "../types";

interface PositionsPanelProps {
  positions: Position[];
}

function formatPrice(n: number): string {
  return n.toLocaleString("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function PlBar({ pct }: { pct: number }) {
  // pct is a fraction e.g. 0.18 = 18%, cap at ±50%
  const clamped = Math.max(-0.5, Math.min(0.5, pct));
  const pctDisplay = (pct * 100).toFixed(2);
  const isPositive = pct >= 0;

  // Bar fills from center — use absolute % of 50% max
  const barWidth = (Math.abs(clamped) / 0.5) * 100;

  return (
    <div className="pl-cell">
      <div className="pl-bar-track">
        <div
          className="pl-bar-fill"
          style={{
            width: `${barWidth}%`,
            background: isPositive
              ? "linear-gradient(90deg, rgba(158,206,106,0.3), rgba(158,206,106,0.8))"
              : "linear-gradient(90deg, rgba(247,118,142,0.8), rgba(247,118,142,0.3))",
            marginLeft: isPositive ? "50%" : `${50 - barWidth / 2}%`,
          }}
        />
      </div>
      <span
        className={`pl-pct ${isPositive ? "pl-positive" : "pl-negative"}`}
      >
        {isPositive ? "+" : ""}
        {pctDisplay}%
      </span>
    </div>
  );
}

export function PositionsPanel({ positions }: PositionsPanelProps) {
  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">Positions</span>
        <span className="panel-meta">{positions.length} open</span>
      </div>

      {positions.length === 0 ? (
        <div className="empty-state">No open positions</div>
      ) : (
        <div className="positions-table-wrap">
          <table className="positions-table">
            <thead>
              <tr>
                <th>Contract</th>
                <th>Strategy</th>
                <th>Dir</th>
                <th>Qty</th>
                <th>Entry</th>
                <th>Current</th>
                <th>Unr. P&L</th>
                <th>P&L %</th>
              </tr>
            </thead>
            <tbody>
              {positions.map((pos) => {
                const plSign = pos.unrealized_pl >= 0 ? "+" : "";
                const strategyId = pos.strategy_id ?? "manual";
                return (
                  <tr key={pos.contract}>
                    <td>
                      <div className="contract-symbol">{pos.contract}</div>
                      <div className="underlying-tag">{pos.underlying}</div>
                    </td>
                    <td>
                      <span className={`strategy-badge strategy-badge--${strategyId}`}>
                        {strategyId}
                      </span>
                    </td>
                    <td>
                      <span
                        className={`direction-badge direction-badge--${
                          pos.direction.toLowerCase() === "call"
                            ? "call"
                            : "put"
                        }`}
                      >
                        {pos.direction.toUpperCase()}
                      </span>
                    </td>
                    <td className="price-mono">{pos.qty}</td>
                    <td className="price-mono">
                      {formatPrice(pos.entry_price)}
                    </td>
                    <td className="price-mono">
                      {formatPrice(pos.current_price)}
                    </td>
                    <td>
                      <span
                        className={`price-mono ${
                          pos.unrealized_pl >= 0 ? "pl-positive" : "pl-negative"
                        }`}
                      >
                        {plSign}
                        {formatPrice(pos.unrealized_pl)}
                      </span>
                    </td>
                    <td>
                      <PlBar pct={pos.unrealized_plpc} />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
