import type { TickerEntry } from "../types";

interface TickerStripProps {
  entries: TickerEntry[];
}

function formatPrice(p: number): string {
  if (p >= 100) return p.toFixed(2);
  if (p >= 10)  return p.toFixed(2);
  return p.toFixed(2);
}

function formatPct(pct: number): string {
  const v = pct * 100;
  const sign = v > 0 ? "+" : v < 0 ? "" : "";
  return `${sign}${v.toFixed(2)}%`;
}

/// Nasdaq-style scrolling ticker. The marquee animation is a pure CSS
/// translateX loop — to keep it seamless we render the tape twice back-to-back
/// and translate by 50% so the second copy slides in just as the first leaves.
export function TickerStrip({ entries }: TickerStripProps) {
  if (entries.length === 0) {
    return (
      <div className="ticker-strip ticker-strip--empty">
        <span className="ticker-empty">— ticker —</span>
      </div>
    );
  }

  return (
    <div className="ticker-strip" aria-label="Live ticker">
      <div className="ticker-track">
        {[0, 1].map((copy) => (
          <div className="ticker-tape" key={copy} aria-hidden={copy === 1}>
            {entries.map((e) => {
              const cls =
                e.change_pct > 0
                  ? "ticker-item--up"
                  : e.change_pct < 0
                    ? "ticker-item--down"
                    : "ticker-item--flat";
              const arrow = e.change_pct > 0 ? "▲" : e.change_pct < 0 ? "▼" : "·";
              return (
                <span
                  key={`${copy}-${e.symbol}`}
                  className={`ticker-item ${cls}`}
                >
                  <span className="ticker-symbol">{e.symbol}</span>
                  <span className="ticker-price">{formatPrice(e.price)}</span>
                  <span className="ticker-arrow">{arrow}</span>
                  <span className="ticker-pct">{formatPct(e.change_pct)}</span>
                </span>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
