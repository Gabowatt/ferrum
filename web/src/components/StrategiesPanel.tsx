import { useState } from "react";
import { api } from "../api";
import type { StrategyInfo } from "../types";

interface StrategiesPanelProps {
  strategies: StrategyInfo[];
  onChange: () => Promise<void>;
}

/// Toggle switch — keeps the optimistic UI state for the duration of the
/// in-flight request so the user gets immediate feedback.
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

  async function handleToggle(s: StrategyInfo) {
    const next = !s.enabled;
    setPending((p) => ({ ...p, [s.id]: true }));
    try {
      await api.setStrategyEnabled(s.id, next);
      await onChange();  // pull fresh stats from daemon
    } catch (e) {
      console.error("toggle failed", e);
    } finally {
      setPending((p) => ({ ...p, [s.id]: false }));
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">Strategies</span>
        <span className="panel-meta">
          {strategies.filter((s) => s.enabled).length}/{strategies.length} enabled
        </span>
      </div>

      {strategies.length === 0 ? (
        <div className="empty-state">No strategies registered</div>
      ) : (
        <div className="strategies-list">
          {strategies.map((s) => (
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
              </div>
              <ToggleSwitch
                checked={s.enabled}
                disabled={!!pending[s.id]}
                onToggle={() => handleToggle(s)}
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
