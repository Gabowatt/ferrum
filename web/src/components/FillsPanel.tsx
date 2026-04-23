import type { Fill } from "../types";

interface FillsPanelProps {
  fills: Fill[];
}

function formatPrice(n: number): string {
  return n.toLocaleString("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function timeAgo(isoString: string): string {
  try {
    const then = new Date(isoString).getTime();
    const now = Date.now();
    const diffS = Math.floor((now - then) / 1000);
    if (diffS < 60) return `${diffS}s ago`;
    const diffM = Math.floor(diffS / 60);
    if (diffM < 60) return `${diffM}m ago`;
    const diffH = Math.floor(diffM / 60);
    if (diffH < 24) return `${diffH}h ago`;
    return new Date(isoString).toLocaleDateString();
  } catch {
    return "—";
  }
}

export function FillsPanel({ fills }: FillsPanelProps) {
  const recent = fills.slice(0, 10);

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">Recent Fills</span>
        <span className="panel-meta">last {Math.min(fills.length, 10)}</span>
      </div>

      {recent.length === 0 ? (
        <div className="empty-state">No fills today</div>
      ) : (
        <div className="fills-table-wrap">
        <table className="fills-table">
          <thead>
            <tr>
              <th>Symbol</th>
              <th>Side</th>
              <th>Qty</th>
              <th>Price</th>
              <th>Time</th>
              <th>Order ID</th>
            </tr>
          </thead>
          <tbody>
            {recent.map((fill, i) => (
              <tr key={fill.id ?? `fill-${i}`}>
                <td>
                  <span className="text-mono" style={{ fontSize: "11px" }}>
                    {fill.symbol}
                  </span>
                </td>
                <td>
                  <span className={`side-badge side-badge--${fill.side}`}>
                    {fill.side.toUpperCase()}
                  </span>
                </td>
                <td className="text-mono">{fill.qty}</td>
                <td className="price-mono">{formatPrice(fill.price)}</td>
                <td className="time-ago">{timeAgo(fill.timestamp)}</td>
                <td>
                  <span
                    className="text-mono text-dim"
                    style={{ fontSize: "10px" }}
                  >
                    {fill.order_id.slice(0, 8)}…
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        </div>
      )}
    </div>
  );
}
