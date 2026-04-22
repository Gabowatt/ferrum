import { useEffect, useRef } from "react";
import type { LogLevel } from "../types";
import type { IndexedLogEvent } from "../hooks/useDashboard";

interface LogStreamProps {
  logs: IndexedLogEvent[];
  newLogIds: Set<number>;
}

function formatTs(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString("en-US", {
      hour12: false,
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  } catch {
    return "—";
  }
}

function msgClass(level: LogLevel): string {
  switch (level) {
    case "error": return "log-msg log-msg--error";
    case "warn":  return "log-msg log-msg--warn";
    case "signal": return "log-msg log-msg--signal";
    case "order": return "log-msg log-msg--order";
    default: return "log-msg";
  }
}

export function LogStream({ logs, newLogIds }: LogStreamProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const atBottomRef = useRef(true);

  // Track scroll position to know if user scrolled up
  function handleScroll() {
    const el = containerRef.current;
    if (!el) return;
    const threshold = 40;
    atBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight < threshold;
  }

  // Auto-scroll to bottom when new logs arrive, if already at bottom
  useEffect(() => {
    if (!atBottomRef.current) return;
    const el = containerRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [logs.length]);

  return (
    <div className="panel">
      <div className="panel-header">
        <span className="panel-title">Log Stream</span>
        <span className="panel-meta">{logs.length} events</span>
      </div>

      <div
        className="log-stream"
        ref={containerRef}
        onScroll={handleScroll}
      >
        {logs.length === 0 && (
          <div className="empty-state">Waiting for log events…</div>
        )}
        {logs.map((log) => (
          <div
            key={log._idx}
            className={`log-row ${newLogIds.has(log._idx) ? "log-row--new" : ""}`}
          >
            <span className="log-ts">{formatTs(log.timestamp)}</span>
            <span className={`log-level log-level--${log.level}`}>
              {log.level}
            </span>
            <span className={msgClass(log.level)}>{log.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
