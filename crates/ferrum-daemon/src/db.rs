use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::path::PathBuf;
use ferrum_core::{error::FerrumError, types::FillRecord};

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

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<(), FerrumError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS fills (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol    TEXT    NOT NULL,
                side      TEXT    NOT NULL,
                qty       REAL    NOT NULL,
                price     REAL    NOT NULL,
                timestamp TEXT    NOT NULL,
                order_id  TEXT    NOT NULL UNIQUE
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS log_events (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT    NOT NULL,
                level     TEXT    NOT NULL,
                message   TEXT    NOT NULL
            )",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                mode       TEXT    NOT NULL,
                started_at TEXT    NOT NULL,
                stopped_at TEXT
            )",
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Upsert a fill from Alpaca activity feed.
    pub async fn upsert_fill(&self, fill: &FillRecord) -> Result<(), FerrumError> {
        sqlx::query(
            "INSERT INTO fills (symbol, side, qty, price, timestamp, order_id)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(order_id) DO UPDATE SET
                 qty = excluded.qty,
                 price = excluded.price",
        )
        .bind(&fill.symbol)
        .bind(&fill.side)
        .bind(fill.qty)
        .bind(fill.price)
        .bind(fill.timestamp.to_rfc3339())
        .bind(&fill.order_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Fetch the N most recent fills.
    pub async fn recent_fills(&self, limit: i64) -> Result<Vec<FillRecord>, FerrumError> {
        let rows = sqlx::query(
            "SELECT id, symbol, side, qty, price, timestamp, order_id
             FROM fills
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
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
            })
            .collect()
    }

    /// Persist a log event.
    pub async fn insert_log(&self, ts: &str, level: &str, msg: &str) -> Result<(), FerrumError> {
        sqlx::query(
            "INSERT INTO log_events (timestamp, level, message) VALUES (?, ?, ?)",
        )
        .bind(ts)
        .bind(level)
        .bind(msg)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Open a new session record, return its ID.
    pub async fn start_session(&self, mode: &str) -> Result<i64, FerrumError> {
        let now = Utc::now().to_rfc3339();
        let row = sqlx::query("INSERT INTO sessions (mode, started_at) VALUES (?, ?) RETURNING id")
            .bind(mode)
            .bind(&now)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get("id"))
    }

    /// Close a session.
    pub async fn stop_session(&self, id: i64) -> Result<(), FerrumError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE sessions SET stopped_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local").join("share").join("ferrum")
}
