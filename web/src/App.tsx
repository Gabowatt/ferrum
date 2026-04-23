import { useDashboard } from "./hooks/useDashboard";
import { Header } from "./components/Header";
import { PositionsPanel } from "./components/PositionsPanel";
import { FillsPanel } from "./components/FillsPanel";
import { PnlPanel } from "./components/PnlPanel";
import { LogStream } from "./components/LogStream";
import { StrategiesPanel } from "./components/StrategiesPanel";

export default function App() {
  const {
    botStatus,
    pnl,
    positions,
    fills,
    pdt,
    clock,
    equity,
    strategies,
    logs,
    newLogIds,
    refresh,
  } = useDashboard();

  return (
    <div className="app">
      <Header
        status={botStatus?.status ?? null}
        mode={botStatus?.mode ?? null}
        pdt={pdt}
        clock={clock}
        onStatusChange={refresh.status}
        onModeChange={refresh.status}
      />

      <main className="main-content">
        <div className="main-grid">
          {/* Left 60% */}
          <div className="left-column">
            <PositionsPanel positions={positions} />
            <FillsPanel fills={fills} />
          </div>

          {/* Right 40% */}
          <div className="right-column">
            <StrategiesPanel
              strategies={strategies}
              onChange={refresh.strategies}
            />
            <PnlPanel pnl={pnl} equity={equity} />
          </div>
        </div>

        {/* Full-width log stream */}
        <LogStream logs={logs} newLogIds={newLogIds} />
      </main>
    </div>
  );
}
