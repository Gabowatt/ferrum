import { useState } from "react";
import { api } from "../api";
import type {
  BotStatus,
  TradingMode,
  PdtResponse,
  ClockResponse,
} from "../types";

interface HeaderProps {
  status: BotStatus | null;
  mode: TradingMode | null;
  pdt: PdtResponse | null;
  clock: ClockResponse | null;
  onStatusChange: () => Promise<void>;
  onModeChange: () => Promise<void>;
}


function ModeDialog({
  current,
  onClose,
  onConfirm,
}: {
  current: TradingMode;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const [restartRequired, setRestartRequired] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);

  const target: TradingMode = current === "paper" ? "live" : "paper";

  async function handleConfirm() {
    setLoading(true);
    try {
      const res = await api.setMode(target);
      if (res.restart_required) {
        setRestartRequired(true);
      } else {
        onConfirm();
        onClose();
      }
    } catch {
      // error — just close
      onClose();
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal__title">
          {target === "live"
            ? "Switch to LIVE trading?"
            : "Switch to PAPER trading?"}
        </div>

        {target === "live" && (
          <div className="modal__warning">
            <strong>Warning</strong>
            This will use real funds. All orders will be executed against your
            live brokerage account. Ensure you have reviewed all risk parameters
            before proceeding.
          </div>
        )}

        {restartRequired && (
          <div className="modal__restart-banner">
            Daemon restart required to apply mode change.
          </div>
        )}

        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          {!restartRequired ? (
            <button
              className={
                target === "live" ? "btn btn--confirm" : "btn btn--primary"
              }
              onClick={handleConfirm}
              disabled={loading}
            >
              {loading ? "Switching…" : `Confirm → ${target.toUpperCase()}`}
            </button>
          ) : (
            <button
              className="btn btn--ghost"
              onClick={() => {
                onConfirm();
                onClose();
              }}
            >
              Close
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

export function Header({
  status,
  mode,
  pdt,
  clock,
  onStatusChange,
  onModeChange,
}: HeaderProps) {
  const [modeDialogOpen, setModeDialogOpen] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  const botStatus: BotStatus = status ?? "idle";
  const isRunning = botStatus === "running";
  const isStopping = botStatus === "stopping";

  const pdtAtMax = pdt ? pdt.used >= pdt.max : false;
  const pdtNearMax = pdt ? pdt.used === pdt.max - 1 : false;

  async function handleStart() {
    setActionLoading(true);
    try {
      await api.start();
      await onStatusChange();
    } finally {
      setActionLoading(false);
    }
  }

  async function handleStop() {
    setActionLoading(true);
    try {
      await api.stop();
      await onStatusChange();
      // Poll status every 1s for up to 10s so the UI picks up the Running → Stopping → Idle
      // transition promptly instead of waiting for the 5s polling interval.
      for (let i = 0; i < 10; i++) {
        await new Promise((r) => setTimeout(r, 1000));
        await onStatusChange();
      }
    } finally {
      setActionLoading(false);
    }
  }

  return (
    <>
      <header className="header">
        <span className="header-logo">FERRUM</span>

        <div className="header-divider" />

        <div className="header-items">
          {/* Status indicator */}
          <div className="status-indicator">
            <span className={`status-dot status-dot--${botStatus}`} />
            <span className={`status-text status-text--${botStatus}`}>
              {botStatus}
            </span>
          </div>

          <div className="header-divider" />

          {/* Mode chip */}
          {mode && (
            <button
              className={`mode-chip mode-chip--${mode}`}
              onClick={() => setModeDialogOpen(true)}
              title="Click to switch trading mode"
            >
              {mode === "live" && <span>⚡</span>}
              {mode.toUpperCase()}
            </button>
          )}

          <div className="header-divider" />

          {/* Market status */}
          {clock && (
            <div className="market-status">
              <span
                className={`market-status__badge ${
                  clock.is_open
                    ? "market-status__badge--open"
                    : "market-status__badge--closed"
                }`}
              >
                {clock.is_open ? "NYSE OPEN" : "NYSE CLOSED"}
              </span>
              <span className="market-status__next">
                · {clock.next_change}
              </span>
            </div>
          )}

          <div className="header-divider" />

          {/* PDT counter */}
          {pdt && (
            <div className="pdt-counter">
              <span className="pdt-label">PDT</span>
              <span
                className={`pdt-value ${
                  pdtAtMax
                    ? "pdt-value--max"
                    : pdtNearMax
                    ? "pdt-value--warn"
                    : "pdt-value--ok"
                }`}
              >
                {pdt.used}/{pdt.max}
              </span>
            </div>
          )}
        </div>

        {/* Start / Stop */}
        <div className="header-right">
          {!isRunning && !isStopping && (
            <button
              className="btn btn--primary"
              onClick={handleStart}
              disabled={actionLoading}
            >
              ▶ Start
            </button>
          )}
          {(isRunning || isStopping) && (
            <button
              className="btn btn--danger"
              onClick={handleStop}
              disabled={actionLoading || isStopping}
            >
              {isStopping ? "Stopping…" : "■ Stop"}
            </button>
          )}
        </div>
      </header>

      {modeDialogOpen && mode && (
        <ModeDialog
          current={mode}
          onClose={() => setModeDialogOpen(false)}
          onConfirm={onModeChange}
        />
      )}
    </>
  );
}
