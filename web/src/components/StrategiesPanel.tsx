import { useState } from "react";
import { api } from "../api";
import type { StrategyInfo } from "../types";

interface StrategiesPanelProps {
  strategies: StrategyInfo[];
  onChange: () => Promise<void>;
}

/// Toggle switch — purely presentational. Visual state is driven by `checked`
/// (which the parent flips optimistically the moment the click fires) so the
/// UI feels instant even if the daemon round-trip is slow or fails.
function ToggleSwitch({
  checked,
  disabled,
  onToggle,
}: {
  checked: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      className={`toggle-switch ${checked ? "toggle-switch--on" : "toggle-switch--off"}`}
      onClick={onToggle}
    >
      <span className="toggle-switch__knob" />
    </button>
  );
}

export function StrategiesPanel({ strategies, onChange }: StrategiesPanelProps) {
  // Per-strategy in-flight flag so each toggle disables only its own row.
  const [pending, setPending] = useState<Record<string, boolean>>({});
  // Optimistic enabled state — wins over the prop while a toggle is in flight
  // (or until the next successful refetch overrides it). Cleared on success
  // so the prop becomes the source of truth again.
  const [optimistic, setOptimistic] = useState<Record<string, boolean>>({});
  // Per-strategy error string surfaced inline below the row.
  const [errors, setErrors] = useState<Record<string, string>>({});

  async function handleToggle(s: StrategyInfo) {
    const next = !(optimistic[s.id] ?? s.enabled);
    setOptimistic((o) => ({ ...o, [s.id]: next }));
    setPending((p) => ({ ...p, [s.id]: true }));
    setErrors((e) => {
      // clear any previous error on this row
      if (!(s.id in e)) return e;
      const { [s.id]: _, ...rest } = e;
      return rest;
    });
    try {
      await api.setStrategyEnabled(s.id, next);
      await onChange(); // pull fresh stats from daemon
      // Success — drop the optimistic value, the refetched prop is now correct.
      setOptimistic((o) => {
        const { [s.id]: _, ...rest } = o;
        return rest;
      });
    } catch (e) {
      // Revert the optimistic flip and surface the failure visibly.
      console.error("toggle failed", e);
      setOptimistic((o) => {
        const { [s.id]: _, ...rest } = o;
        return rest;
      });
      const msg = e instanceof Error ? e.message : String(e);
      setErrors((errs) => ({ ...errs, [s.id]: msg }));
    } finally {
      setPending((p) => ({ ...p, [s.id]: false }));
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">Strategies</span>
        <span className="panel-meta">
          {strategies.filter((s) => (optimistic[s.id] ?? s.enabled)).length}/
          {strategies.length} enabled
        </span>
      </div>

      {strategies.length === 0 ? (
        <div className="empty-state">No strategies registered</div>
      ) : (
        <div className="strategies-list">
          {strategies.map((s) => {
            const checked = optimistic[s.id] ?? s.enabled;
            const err = errors[s.id];
            return (
              <div key={s.id} className="strategy-row">
                <div className="strategy-row__main">
                  <div className="strategy-row__name">
                    <span
                      className={`strategy-badge strategy-badge--${s.id}`}
                    >
                      {s.id}
                    </span>
                    <span className="strategy-row__interval">
                      scan every {s.scan_interval_secs}s
                    </span>
                  </div>
                  <div className="strategy-row__stats">
                    <span className="stat">
                      <span className="stat__label">positions</span>
                      <span className="stat__value">{s.open_positions}</span>
                    </span>
                    <span className="stat">
                      <span className="stat__label">signals today</span>
                      <span className="stat__value">{s.signals_today}</span>
                    </span>
                    <span className="stat">
                      <span className="stat__label">scans today</span>
                      <span className="stat__value">{s.scans_today}</span>
                    </span>
                  </div>
                  {err && (
                    <div className="strategy-row__error" title={err}>
                      toggle failed: {err}
                    </div>
                  )}
                </div>
                <ToggleSwitch
                  checked={checked}
                  disabled={!!pending[s.id]}
                  onToggle={() => handleToggle(s)}
                />
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
