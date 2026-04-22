use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::path::PathBuf;
use ferrum_core::{error::FerrumError, types::FillRecord};
use crate::pdt::DayTradeRecord;

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub timestamp: String,
    pub symbol:    String,
    pub regime:    String,
    pub score:     i32,
    pub direction: Option<String>,
    pub outcome:   String,
}

#[derive(Debug, Clone)]
pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn open() -> Result<Self, FerrumError> {
        let data_dir = data_dir();
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("ferrum.db");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new().max_connections(5).connect(&url).await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), FerrumError> {
        // Core tables.
        //
        // V2.1 multi-strategy attribution: `strategy_id` (default 'forge')
        // tags every fill / trade / scan with the strategy that produced it.
        // Defaults exist so legacy code paths that don't yet pass an explicit
        // id keep working; later commits in V2.1 will thread the value
        // through writers.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS fills (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol      TEXT    NOT NULL,
                side        TEXT    NOT NULL,
                qty         REAL    NOT NULL,
                price       REAL    NOT NULL,
                timestamp   TEXT    NOT NULL,
                order_id    TEXT    NOT NULL UNIQUE,
                strategy_id TEXT    NOT NULL DEFAULT 'forge'
            )"
        ).execute(&self.pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS log_events (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT    NOT NULL,
                level     TEXT    NOT NULL,
                message   TEXT    NOT NULL
            )"
        ).execute(&self.pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                mode       TEXT    NOT NULL,
                started_at TEXT    NOT NULL,
                stopped_at TEXT
            )"
        ).execute(&self.pool).await?;

        // Strategy-specific tables
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS day_trades (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                contract_symbol TEXT    NOT NULL,
                underlying      TEXT    NOT NULL,
                open_time       TEXT    NOT NULL,
                close_time      TEXT    NOT NULL,
                open_price      REAL    NOT NULL,
                close_price     REAL    NOT NULL,
                pnl             REAL    NOT NULL,
                was_emergency   INTEGER DEFAULT 0
            )"
        ).execute(&self.pool).await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS iv_snapshots (
                id                     INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol                 TEXT    NOT NULL,
                timestamp              TEXT    NOT NULL,
                implied_volatility     REAL,
                historical_volatility  REAL,
                iv_rank                REAL,
                UNIQUE(symbol, timestamp)
            )"
        ).execute(&self.pool).await?;

        // `position_id` is nullable. Phase 3 uses it to group the 4 legs of an
        // iron condor under a single position id; Forge writes it as NULL
        // (single-leg positions don't need grouping).
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS trade_log (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                contract_symbol  TEXT    NOT NULL,
                underlying       TEXT    NOT NULL,
                direction        TEXT    NOT NULL,
                action           TEXT    NOT NULL,
                timestamp        TEXT    NOT NULL,
                price            REAL    NOT NULL,
                quantity         INTEGER NOT NULL,
                confluence_score INTEGER,
                regime           TEXT,
                iv_rank          REAL,
                delta            REAL,
                dte              INTEGER,
                exit_reason      TEXT,
                pnl              REAL,
                strategy_id      TEXT    NOT NULL DEFAULT 'forge',
                position_id      TEXT
            )"
        ).execute(&self.pool).await?;

        // Per-symbol scan results — every symbol scored each cycle is recorded.
        // Useful for diagnosing how close/far the bot is from generating a buy signal.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS scan_results (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp   TEXT    NOT NULL,
                symbol      TEXT    NOT NULL,
                regime      TEXT    NOT NULL,
                score       INTEGER NOT NULL,
                direction   TEXT,
                outcome     TEXT    NOT NULL,
                strategy_id TEXT    NOT NULL DEFAULT 'forge'
            )"
        ).execute(&self.pool).await?;

        // ── Upgrade path for existing DBs (V2 → V2.1) ────────────────────────
        // SQLite has no IF NOT EXISTS for ADD COLUMN, so we inspect the current
        // schema first and only ALTER when the column is missing. Idempotent
        // and safe to run on every boot.
        self.add_column_if_missing(
            "fills", "strategy_id",
            "ALTER TABLE fills ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge'",
        ).await?;
        self.add_column_if_missing(
            "trade_log", "strategy_id",
            "ALTER TABLE trade_log ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge'",
        ).await?;
        self.add_column_if_missing(
            "trade_log", "position_id",
            "ALTER TABLE trade_log ADD COLUMN position_id TEXT",
        ).await?;
        self.add_column_if_missing(
            "scan_results", "strategy_id",
            "ALTER TABLE scan_results ADD COLUMN strategy_id TEXT NOT NULL DEFAULT 'forge'",
        ).await?;

        Ok(())
    }

    /// Idempotent ADD COLUMN: no-op if `column` already exists on `table`.
    async fn add_column_if_missing(
        &self,
        table:      &str,
        column:     &str,
        alter_sql:  &str,
    ) -> Result<(), FerrumError> {
        // PRAGMA table_info returns one row per column; the `name` field is the column name.
        let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
            .fetch_all(&self.pool).await?;
        let exists = rows.iter().any(|r| {
            r.try_get::<String, _>("name")
                .map(|n| n == column)
                .unwrap_or(false)
        });
        if !exists {
            sqlx::query(alter_sql).execute(&self.pool).await?;
        }
        Ok(())
    }

    // ── Fills ─────────────────────────────────────────────────────────────────

    pub async fn upsert_fill(&self, fill: &FillRecord) -> Result<(), FerrumError> {
        sqlx::query(
            "INSERT INTO fills (symbol, side, qty, price, timestamp, order_id)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(order_id) DO UPDATE SET qty = excluded.qty, price = excluded.price",
        )
        .bind(&fill.symbol).bind(&fill.side).bind(fill.qty).bind(fill.price)
        .bind(fill.timestamp.to_rfc3339()).bind(&fill.order_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn recent_fills(&self, limit: i64) -> Result<Vec<FillRecord>, FerrumError> {
        let rows = sqlx::query(
            "SELECT id, symbol, side, qty, price, timestamp, order_id
             FROM fills ORDER BY timestamp DESC LIMIT ?",
        ).bind(limit).fetch_all(&self.pool).await?;

        rows.into_iter().map(|r| {
            let ts: String = r.get("timestamp");
            Ok(FillRecord {
                id:        Some(r.get("id")),
                symbol:    r.get("symbol"),
                side:      r.get("side"),
                qty:       r.get("qty"),
                price:     r.get("price"),
                timestamp: ts.parse::<DateTime<Utc>>()
                    .map_err(|e| FerrumError::Config(e.to_string()))?,
                order_id:  r.get("order_id"),
            })
        }).collect()
    }

    // ── Log events ────────────────────────────────────────────────────────────

    pub async fn insert_log(&self, ts: &str, level: &str, msg: &str) -> Result<(), FerrumError> {
        sqlx::query("INSERT INTO log_events (timestamp, level, message) VALUES (?, ?, ?)")
            .bind(ts).bind(level).bind(msg)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn recent_logs(&self, limit: i64) -> Result<Vec<ferrum_core::types::LogEvent>, FerrumError> {
        use ferrum_core::types::{LogEvent, LogLevel};
        let rows = sqlx::query(
            "SELECT timestamp, level, message FROM log_events ORDER BY id DESC LIMIT ?"
        ).bind(limit).fetch_all(&self.pool).await?;

        let mut events: Vec<LogEvent> = rows.iter().map(|r| {
            let ts: String  = r.try_get("timestamp").unwrap_or_default();
            let lv: String  = r.try_get("level").unwrap_or_default();
            let msg: String = r.try_get("message").unwrap_or_default();
            let timestamp = DateTime::parse_from_rfc3339(&ts)
                .map(|t| t.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let level = match lv.as_str() {
                "SIGNAL" => LogLevel::Signal,
                "ORDER"  => LogLevel::Order,
                "RISK"   => LogLevel::Risk,
                "ERROR"  => LogLevel::Error,
                "WARN"   => LogLevel::Warn,
                _        => LogLevel::Info,
            };
            LogEvent { timestamp, level, message: msg }
        }).collect();
        events.reverse(); // oldest first
        Ok(events)
    }

    // ── Sessions ──────────────────────────────────────────────────────────────

    pub async fn start_session(&self, mode: &str) -> Result<i64, FerrumError> {
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query(
            "INSERT INTO sessions (mode, started_at) VALUES (?, ?) RETURNING id"
        ).bind(mode).bind(&now).fetch_one(&self.pool).await?;
        Ok(row.get("id"))
    }

    pub async fn stop_session(&self, id: i64) -> Result<(), FerrumError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE sessions SET stopped_at = ? WHERE id = ?")
            .bind(&now).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    // ── Day trades (PDT tracker) ──────────────────────────────────────────────

    pub async fn insert_day_trade(&self, trade: &DayTradeRecord) -> Result<(), FerrumError> {
        sqlx::query(
            "INSERT INTO day_trades
             (contract_symbol, underlying, open_time, close_time, open_price, close_price, pnl, was_emergency)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&trade.contract_symbol)
        .bind(&trade.underlying)
        .bind(trade.open_time.to_rfc3339())
        .bind(trade.close_time.to_rfc3339())
        .bind(trade.open_price)
        .bind(trade.close_price)
        .bind(trade.pnl)
        .bind(trade.was_emergency as i32)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn recent_day_trades(&self, lookback_days: i64) -> Result<Vec<DayTradeRecord>, FerrumError> {
        let cutoff = (Utc::now() - chrono::Duration::days(lookback_days)).to_rfc3339();
        let rows = sqlx::query(
            "SELECT contract_symbol, underlying, open_time, close_time,
                    open_price, close_price, pnl, was_emergency
             FROM day_trades WHERE close_time >= ? ORDER BY close_time DESC",
        ).bind(&cutoff).fetch_all(&self.pool).await?;

        rows.into_iter().map(|r| {
            let ot: String = r.get("open_time");
            let ct: String = r.get("close_time");
            Ok(DayTradeRecord {
                contract_symbol: r.get("contract_symbol"),
                underlying:      r.get("underlying"),
                open_time:       ot.parse().map_err(|e| FerrumError::Config(format!("{e}")))?,
                close_time:      ct.parse().map_err(|e| FerrumError::Config(format!("{e}")))?,
                open_price:      r.get("open_price"),
                close_price:     r.get("close_price"),
                pnl:             r.get("pnl"),
                was_emergency:   r.get::<i32, _>("was_emergency") != 0,
            })
        }).collect()
    }

    /// Day trades in the current rolling 5-day window.
    pub async fn day_trade_count_in_window(&self, window_days: i64) -> Result<u32, FerrumError> {
        let cutoff = (Utc::now() - chrono::Duration::days(window_days)).to_rfc3339();
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM day_trades WHERE close_time >= ?")
            .bind(&cutoff).fetch_one(&self.pool).await?;
        Ok(row.get::<i64, _>("cnt") as u32)
    }

    // ── IV snapshots ──────────────────────────────────────────────────────────

    pub async fn upsert_iv_snapshot(
        &self,
        symbol: &str,
        iv: f64,
        hv: f64,
        iv_rank: f64,
    ) -> Result<(), FerrumError> {
        let date = Utc::now().format("%Y-%m-%d").to_string();
        sqlx::query(
            "INSERT INTO iv_snapshots (symbol, timestamp, implied_volatility, historical_volatility, iv_rank)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(symbol, timestamp) DO UPDATE SET
                 implied_volatility    = excluded.implied_volatility,
                 historical_volatility = excluded.historical_volatility,
                 iv_rank               = excluded.iv_rank",
        )
        .bind(symbol).bind(&date).bind(iv).bind(hv).bind(iv_rank)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn count_iv_snapshots(&self, symbol: &str) -> Result<i64, FerrumError> {
        let row = sqlx::query("SELECT COUNT(*) as cnt FROM iv_snapshots WHERE symbol = ?")
            .bind(symbol).fetch_one(&self.pool).await?;
        Ok(row.get("cnt"))
    }

    pub async fn iv_range_52w(&self, symbol: &str) -> Result<(f64, f64), FerrumError> {
        let cutoff = (Utc::now() - chrono::Duration::days(365)).to_rfc3339();
        let row = sqlx::query(
            "SELECT MIN(implied_volatility) as low, MAX(implied_volatility) as high
             FROM iv_snapshots WHERE symbol = ? AND timestamp >= ?",
        ).bind(symbol).bind(&cutoff).fetch_one(&self.pool).await?;
        Ok((row.get::<f64, _>("low"), row.get::<f64, _>("high")))
    }

    // ── Scan results ──────────────────────────────────────────────────────────

    /// Record a per-symbol scan result so we can track scoring trends over time.
    /// outcome: "entered" | "below_threshold" | "no_contracts" | "risk_blocked" | "choppy"
    pub async fn insert_scan_result(
        &self,
        symbol:    &str,
        regime:    &str,
        score:     i32,
        direction: Option<&str>,
        outcome:   &str,
    ) -> Result<(), FerrumError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO scan_results (timestamp, symbol, regime, score, direction, outcome)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&now).bind(symbol).bind(regime).bind(score).bind(direction).bind(outcome)
        .execute(&self.pool).await?;
        Ok(())
    }

    /// Most-recent scan results per symbol (latest N rows total, newest first).
    pub async fn recent_scan_results(&self, limit: i64) -> Result<Vec<ScanResult>, FerrumError> {
        let rows = sqlx::query(
            "SELECT timestamp, symbol, regime, score, direction, outcome
             FROM scan_results ORDER BY id DESC LIMIT ?",
        ).bind(limit).fetch_all(&self.pool).await?;

        Ok(rows.iter().map(|r| ScanResult {
            timestamp: r.try_get::<String, _>("timestamp").unwrap_or_default(),
            symbol:    r.try_get("symbol").unwrap_or_default(),
            regime:    r.try_get("regime").unwrap_or_default(),
            score:     r.try_get("score").unwrap_or(0),
            direction: r.try_get("direction").ok(),
            outcome:   r.try_get("outcome").unwrap_or_default(),
        }).collect())
    }

    // ── Trade log ─────────────────────────────────────────────────────────────

    pub async fn insert_trade_log(
        &self,
        contract_symbol: &str,
        underlying:      &str,
        direction:       &str,
        action:          &str,
        price:           f64,
        quantity:        i64,
        confluence_score: Option<i64>,
        regime:          Option<&str>,
        iv_rank:         Option<f64>,
        delta:           Option<f64>,
        dte:             Option<i64>,
        exit_reason:     Option<&str>,
        pnl:             Option<f64>,
    ) -> Result<(), FerrumError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO trade_log
             (contract_symbol, underlying, direction, action, timestamp, price, quantity,
              confluence_score, regime, iv_rank, delta, dte, exit_reason, pnl)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(contract_symbol).bind(underlying).bind(direction).bind(action)
        .bind(&now).bind(price).bind(quantity)
        .bind(confluence_score).bind(regime).bind(iv_rank).bind(delta)
        .bind(dte).bind(exit_reason).bind(pnl)
        .execute(&self.pool).await?;
        Ok(())
    }
}

fn data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local").join("share").join("ferrum")
}
