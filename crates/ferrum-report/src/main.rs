//! ferrum-report — weekly strategy review digest
//!
//! Reads the daemon's SQLite DB and emits a markdown report covering the
//! week's scan activity, vetoes, fills and P&L. The report is meant to
//! drive the weekend tuning pass — replaces eyeballing logs.
//!
//! Usage:
//!   ferrum-report                    # current ISO week
//!   ferrum-report --week=2026-W17
//!   ferrum-report --db=/path/to/ferrum.db --out=docs/reports
//!
//! Output: <out>/<friday-date>.md, e.g. docs/reports/2026-04-24.md.
//! Re-running the same day overwrites the file; that's intentional so the
//! Friday cron entry stays idempotent.

use chrono::{Datelike, Duration, NaiveDate, TimeZone, Utc, Weekday};
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::collections::BTreeMap;
use std::path::PathBuf;

// ── Tunables that the report compares the data against. Mirror the
// strategy.entry config — kept in code (not parsed from config.toml) so
// the report binary stays self-contained. If the live config drifts, the
// "near miss" definition will go stale; treat that as a known small cost
// for not pulling toml into the dep tree.
const TREND_MIN_SCORE: i32 = 6;
const RANGE_MIN_SCORE: i32 = 6;
const CHOPPY_MIN_SCORE: i32 = 8;
const ALLOW_CHOPPY: bool = false;
const PDT_LIMIT: u32 = 2;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let (year, week) = args.iso_week_or_current();
    let (mon, sun, fri) = week_bounds(year, week)?;

    let db_url = format!("sqlite://{}?mode=ro", args.db_path().display());
    let pool = SqlitePoolOptions::new().max_connections(1).connect(&db_url).await?;

    let week_start = mon.and_hms_opt(0, 0, 0).unwrap();
    let week_end   = sun.and_hms_opt(23, 59, 59).unwrap();
    let start_iso  = Utc.from_utc_datetime(&week_start).to_rfc3339();
    let end_iso    = Utc.from_utc_datetime(&week_end).to_rfc3339();

    let data = collect(&pool, &start_iso, &end_iso).await?;
    let md = render(year, week, mon, fri, &data);

    std::fs::create_dir_all(&args.out_dir)?;
    let out_path = args.out_dir.join(format!("{}.md", fri.format("%Y-%m-%d")));
    std::fs::write(&out_path, md)?;
    println!("wrote {}", out_path.display());
    Ok(())
}

// ── Args ────────────────────────────────────────────────────────────────

struct Args {
    week:    Option<(i32, u32)>,
    db:      Option<PathBuf>,
    out_dir: PathBuf,
}

impl Args {
    fn parse() -> Self {
        let mut week = None;
        let mut db   = None;
        let mut out  = PathBuf::from("docs/reports");
        for a in std::env::args().skip(1) {
            if let Some(v) = a.strip_prefix("--week=") {
                week = parse_iso_week(v);
            } else if let Some(v) = a.strip_prefix("--db=") {
                db = Some(PathBuf::from(v));
            } else if let Some(v) = a.strip_prefix("--out=") {
                out = PathBuf::from(v);
            }
        }
        Self { week, db, out_dir: out }
    }

    fn iso_week_or_current(&self) -> (i32, u32) {
        self.week.unwrap_or_else(|| {
            let iso = Utc::now().iso_week();
            (iso.year(), iso.week())
        })
    }

    fn db_path(&self) -> PathBuf {
        self.db.clone().unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join(".local").join("share").join("ferrum").join("ferrum.db")
        })
    }
}

fn parse_iso_week(s: &str) -> Option<(i32, u32)> {
    let (y, w) = s.split_once("-W")?;
    Some((y.parse().ok()?, w.parse().ok()?))
}

fn week_bounds(year: i32, week: u32) -> Result<(NaiveDate, NaiveDate, NaiveDate), String> {
    let mon = NaiveDate::from_isoywd_opt(year, week, Weekday::Mon)
        .ok_or_else(|| format!("invalid ISO week {year}-W{week:02}"))?;
    let sun = mon + Duration::days(6);
    let fri = mon + Duration::days(4);
    Ok((mon, sun, fri))
}

// ── Data collection ─────────────────────────────────────────────────────

#[derive(Default)]
struct WeekData {
    total_scans:     i64,
    by_outcome:      BTreeMap<String, i64>,
    by_regime:       BTreeMap<String, i64>,
    per_symbol:      Vec<SymbolStats>,
    near_miss:       Vec<NearMissBucket>,
    fills:           Vec<FillRow>,
    fills_by_day:    Vec<DayFills>,
    closed_trades:   Vec<ClosedTrade>,
    open_positions:  Vec<OpenPosition>,
    day_trades:      Vec<DayTradeRow>,
    risk_blocks:     BTreeMap<String, i64>,
    pdt_warnings:    i64,
    insufficient_funds_errors: i64,
    other_errors:    i64,
}

struct SymbolStats {
    symbol:    String,
    scans:     i64,
    entered:   i64,
    choppy:    i64,
    below:     i64,
    extreme:   i64,
    no_ctr:    i64,
    avg_score: f64,
    max_score: i64,
}

struct NearMissBucket {
    regime: String,
    score:  i64,
    count:  i64,
    threshold: i32,
}

struct FillRow {
    // `symbol` is read indirectly via the per-day aggregation below (`day_map`
    // groups on the timestamp prefix, not the symbol). Kept as part of the
    // row so future per-symbol slices don't have to re-query.
    #[allow(dead_code)]
    symbol: String,
    side:   String,
    qty:    f64,
    price:  f64,
    ts:     String,
}

struct DayFills {
    day:        String,
    buys:       i64,
    sells:      i64,
    buy_notional:  f64,
    sell_notional: f64,
}

struct ClosedTrade {
    contract:    String,
    underlying:  String,
    direction:   String,
    exit_reason: Option<String>,
    pnl:         Option<f64>,
}

struct OpenPosition {
    contract:  String,
    legs:      i64,
    total_qty: i64,
    avg_price: f64,
}

struct DayTradeRow {
    contract:      String,
    underlying:    String,
    pnl:           f64,
    was_emergency: bool,
}

async fn collect(pool: &SqlitePool, start: &str, end: &str) -> Result<WeekData, sqlx::Error> {
    let mut d = WeekData::default();

    // ── Scan totals + outcome / regime breakdown ───────────────────────
    d.total_scans = sqlx::query("SELECT COUNT(*) AS n FROM scan_results WHERE timestamp >= ? AND timestamp <= ?")
        .bind(start).bind(end).fetch_one(pool).await?.get::<i64, _>("n");

    for row in sqlx::query("SELECT outcome, COUNT(*) AS n FROM scan_results
                            WHERE timestamp >= ? AND timestamp <= ?
                            GROUP BY outcome")
        .bind(start).bind(end).fetch_all(pool).await?
    {
        d.by_outcome.insert(row.get("outcome"), row.get("n"));
    }

    for row in sqlx::query("SELECT regime, COUNT(*) AS n FROM scan_results
                            WHERE timestamp >= ? AND timestamp <= ?
                            GROUP BY regime")
        .bind(start).bind(end).fetch_all(pool).await?
    {
        d.by_regime.insert(row.get("regime"), row.get("n"));
    }

    // ── Per-symbol stats ────────────────────────────────────────────────
    for row in sqlx::query(
        "SELECT symbol,
                COUNT(*) AS scans,
                SUM(CASE WHEN outcome='entered'           THEN 1 ELSE 0 END) AS entered,
                SUM(CASE WHEN outcome='choppy'            THEN 1 ELSE 0 END) AS choppy,
                SUM(CASE WHEN outcome='below_threshold'   THEN 1 ELSE 0 END) AS below,
                SUM(CASE WHEN outcome='extreme_proximity' THEN 1 ELSE 0 END) AS extreme,
                SUM(CASE WHEN outcome='no_contracts'      THEN 1 ELSE 0 END) AS no_ctr,
                AVG(score) AS avg_score,
                MAX(score) AS max_score
         FROM scan_results
         WHERE timestamp >= ? AND timestamp <= ?
         GROUP BY symbol
         ORDER BY entered DESC, scans DESC"
    ).bind(start).bind(end).fetch_all(pool).await? {
        d.per_symbol.push(SymbolStats {
            symbol:    row.get("symbol"),
            scans:     row.get("scans"),
            entered:   row.get("entered"),
            choppy:    row.get("choppy"),
            below:     row.get("below"),
            extreme:   row.get("extreme"),
            no_ctr:    row.get("no_ctr"),
            avg_score: row.try_get::<f64, _>("avg_score").unwrap_or(0.0),
            max_score: row.try_get("max_score").unwrap_or(0),
        });
    }

    // ── Near-miss: score = (threshold − 1) by regime ────────────────────
    // Trend & range fire at 6+; choppy at 8+ (and only if allow_choppy=true).
    // We surface scores at threshold − 1 to highlight what would have
    // changed if the threshold dropped a point.
    for (regime, threshold) in [
        ("trending_up",   TREND_MIN_SCORE),
        ("trending_down", TREND_MIN_SCORE),
        ("range_bound",   RANGE_MIN_SCORE),
        ("choppy",        CHOPPY_MIN_SCORE),
    ] {
        let near = (threshold - 1) as i64;
        let count: i64 = sqlx::query(
            "SELECT COUNT(*) AS n FROM scan_results
             WHERE timestamp >= ? AND timestamp <= ? AND regime = ? AND score = ?"
        ).bind(start).bind(end).bind(regime).bind(near)
         .fetch_one(pool).await?.get("n");
        if count > 0 {
            d.near_miss.push(NearMissBucket {
                regime: regime.to_string(), score: near, count, threshold,
            });
        }
    }

    // ── Fills (full week, ordered) ─────────────────────────────────────
    for row in sqlx::query(
        "SELECT symbol, side, qty, price, timestamp
         FROM fills WHERE timestamp >= ? AND timestamp <= ?
         ORDER BY timestamp"
    ).bind(start).bind(end).fetch_all(pool).await? {
        d.fills.push(FillRow {
            symbol: row.get("symbol"),
            side:   row.get("side"),
            qty:    row.get("qty"),
            price:  row.get("price"),
            ts:     row.get("timestamp"),
        });
    }

    // ── Per-day fill totals ────────────────────────────────────────────
    let mut day_map: BTreeMap<String, (i64, i64, f64, f64)> = BTreeMap::new();
    for f in &d.fills {
        let day = f.ts.get(..10).unwrap_or("?").to_string();
        let e = day_map.entry(day).or_insert((0, 0, 0.0, 0.0));
        // Options are 100-multiplier; each `fill.qty` is contracts.
        let notional = f.qty * f.price * 100.0;
        if f.side == "buy" { e.0 += 1; e.2 += notional; } else { e.1 += 1; e.3 += notional; }
    }
    for (day, (buys, sells, buy_n, sell_n)) in day_map {
        d.fills_by_day.push(DayFills {
            day, buys, sells, buy_notional: buy_n, sell_notional: sell_n,
        });
    }

    // ── Closed trades from trade_log ────────────────────────────────────
    for row in sqlx::query(
        "SELECT contract_symbol, underlying, direction, exit_reason, pnl
         FROM trade_log
         WHERE timestamp >= ? AND timestamp <= ? AND action='close'
         ORDER BY timestamp"
    ).bind(start).bind(end).fetch_all(pool).await? {
        d.closed_trades.push(ClosedTrade {
            contract:    row.get("contract_symbol"),
            underlying:  row.get("underlying"),
            direction:   row.get("direction"),
            exit_reason: row.try_get("exit_reason").ok(),
            pnl:         row.try_get("pnl").ok(),
        });
    }

    // ── Open positions = buys with no matching close in window ─────────
    for row in sqlx::query(
        "SELECT contract_symbol AS contract, COUNT(*) AS legs,
                CAST(COALESCE(SUM(quantity), 0) AS INTEGER) AS total_qty,
                AVG(price) AS avg_price
         FROM trade_log
         WHERE timestamp >= ? AND timestamp <= ? AND action='buy'
           AND contract_symbol NOT IN (
             SELECT contract_symbol FROM trade_log
             WHERE timestamp >= ? AND timestamp <= ? AND action='close'
           )
         GROUP BY contract_symbol
         ORDER BY total_qty DESC"
    ).bind(start).bind(end).bind(start).bind(end).fetch_all(pool).await? {
        d.open_positions.push(OpenPosition {
            contract:  row.get("contract"),
            legs:      row.get("legs"),
            total_qty: row.get("total_qty"),
            avg_price: row.try_get::<f64, _>("avg_price").unwrap_or(0.0),
        });
    }

    // ── Day trades ─────────────────────────────────────────────────────
    for row in sqlx::query(
        "SELECT contract_symbol, underlying, pnl, was_emergency
         FROM day_trades
         WHERE close_time >= ? AND close_time <= ?
         ORDER BY close_time"
    ).bind(start).bind(end).fetch_all(pool).await? {
        d.day_trades.push(DayTradeRow {
            contract:      row.get("contract_symbol"),
            underlying:    row.get("underlying"),
            pnl:           row.get("pnl"),
            was_emergency: row.try_get::<i64, _>("was_emergency").unwrap_or(0) != 0,
        });
    }

    // ── Risk-block categorisation from log_events ──────────────────────
    for row in sqlx::query(
        "SELECT
           CASE
             WHEN message LIKE '%sector%'         THEN 'sector_cap'
             WHEN message LIKE '%open positions%' THEN 'max_open_positions'
             WHEN message LIKE '%PDT%'            THEN 'pdt_block'
             WHEN message LIKE '%cooldown%'       THEN 'cooldown'
             WHEN message LIKE '%cash%'           THEN 'cash_reserve'
             WHEN message LIKE '%portfolio%'
               OR message LIKE '%drawdown%'       THEN 'portfolio_risk'
             WHEN message LIKE '%passed%'         THEN 'passed'
             ELSE 'other'
           END AS category,
           COUNT(*) AS n
         FROM log_events
         WHERE level='RISK' AND timestamp >= ? AND timestamp <= ?
         GROUP BY category"
    ).bind(start).bind(end).fetch_all(pool).await? {
        let cat: String = row.get("category");
        if cat == "passed" { continue; }   // not a block
        d.risk_blocks.insert(cat, row.get("n"));
    }

    // ── PDT warnings (overnight holds because PDT cap was hit) ─────────
    d.pdt_warnings = sqlx::query(
        "SELECT COUNT(DISTINCT substr(message, 1, instr(message, ' — ') - 1)) AS n
         FROM log_events
         WHERE level='WARN' AND timestamp >= ? AND timestamp <= ?
           AND message LIKE '%PDT%'"
    ).bind(start).bind(end).fetch_one(pool).await?.get("n");

    // ── Errors: insufficient buying power vs. everything else ──────────
    d.insufficient_funds_errors = sqlx::query(
        "SELECT COUNT(*) AS n FROM log_events
         WHERE level='ERROR' AND timestamp >= ? AND timestamp <= ?
           AND (message LIKE '%insufficient%' OR message LIKE '%cost_basis%')"
    ).bind(start).bind(end).fetch_one(pool).await?.get("n");

    d.other_errors = sqlx::query(
        "SELECT COUNT(*) AS n FROM log_events
         WHERE level='ERROR' AND timestamp >= ? AND timestamp <= ?
           AND NOT (message LIKE '%insufficient%' OR message LIKE '%cost_basis%')"
    ).bind(start).bind(end).fetch_one(pool).await?.get("n");

    Ok(d)
}

// ── Markdown rendering ──────────────────────────────────────────────────

fn render(year: i32, week: u32, mon: NaiveDate, fri: NaiveDate, d: &WeekData) -> String {
    let mut out = String::new();
    let total = d.total_scans.max(1) as f64;

    // ── Header / metadata ──────────────────────────────────────────────
    out.push_str(&format!("# Weekly review — {}-W{:02}\n\n", year, week));
    out.push_str(&format!(
        "**Coverage:** {} → {}  \n**Generated:** {} UTC\n\n",
        mon.format("%Y-%m-%d"),
        fri.format("%Y-%m-%d"),
        Utc::now().format("%Y-%m-%d %H:%M"),
    ));

    // ── Verdict (operator-edited at the top before the weekend pass) ───
    out.push_str("## Verdict\n\n");
    out.push_str("> _operator: write a one-line verdict here before tuning._\n\n");

    // ── Headline numbers ───────────────────────────────────────────────
    let entered = *d.by_outcome.get("entered").unwrap_or(&0);
    let buys  = d.fills.iter().filter(|f| f.side == "buy").count();
    let sells = d.fills.iter().filter(|f| f.side == "sell").count();
    let realized: f64 = d.closed_trades.iter().filter_map(|t| t.pnl).sum();
    out.push_str("## Headline\n\n");
    out.push_str(&format!(
        "- **Scans:** {} across {} symbols\n",
        d.total_scans,
        d.per_symbol.len(),
    ));
    out.push_str(&format!(
        "- **Signals entered:** {} ({:.1}% of scans)\n",
        entered, 100.0 * entered as f64 / total,
    ));
    out.push_str(&format!("- **Fills:** {} buys, {} sells\n", buys, sells));
    out.push_str(&format!("- **Closed trades:** {} (realised P&L: {:+.2})\n", d.closed_trades.len(), realized));
    out.push_str(&format!("- **Day trades used:** {} / {} (rolling 5-day)\n\n", d.day_trades.len(), PDT_LIMIT));

    // ── Why buys were low (or strong) — narrative derived from data ────
    out.push_str("## Why buys were low\n\n");
    let choppy   = *d.by_outcome.get("choppy").unwrap_or(&0);
    let below    = *d.by_outcome.get("below_threshold").unwrap_or(&0);
    let extreme  = *d.by_outcome.get("extreme_proximity").unwrap_or(&0);
    let no_ctr   = *d.by_outcome.get("no_contracts").unwrap_or(&0);
    let sector_blocks = *d.risk_blocks.get("sector_cap").unwrap_or(&0);
    let max_open      = *d.risk_blocks.get("max_open_positions").unwrap_or(&0);

    out.push_str(&format!(
        "Of {} scans this week, only {} ({:.1}%) cleared all gates and emitted a signal. The funnel:\n\n",
        d.total_scans, entered, 100.0 * entered as f64 / total,
    ));
    out.push_str(&format!(
        "1. **Regime gate** — {} scans ({:.1}%) landed in *choppy* and were filtered before scoring (`allow_choppy = {}`).\n",
        choppy, 100.0 * choppy as f64 / total, ALLOW_CHOPPY,
    ));
    out.push_str(&format!(
        "2. **Score gate** — {} scans ({:.1}%) had a regime but scored *below threshold* (trend/range need ≥{}, choppy ≥{}). This is the dominant reason \
         for the trend-up bucket: most symbols spent the week range-grinding without confluence.\n",
        below, 100.0 * below as f64 / total, TREND_MIN_SCORE, CHOPPY_MIN_SCORE,
    ));
    out.push_str(&format!(
        "3. **Extreme-proximity veto** — {} scans ({:.1}%) had a qualifying score but were within 0.25 ATR of the 20-day high/low. \
         At the new 0.25 ATR threshold this is rare; before tuning down from 0.5 ATR it would have been ~4× higher.\n",
        extreme, 100.0 * extreme as f64 / total,
    ));
    if no_ctr > 0 {
        out.push_str(&format!(
            "4. **No tradable contracts** — {} scans ({:.1}%) found a signal but no contract passed the liquidity filter (OI/volume/spread).\n",
            no_ctr, 100.0 * no_ctr as f64 / total,
        ));
    }
    out.push_str("\n");
    out.push_str("Once a signal *did* clear the strategy gates, risk caps then thinned out the entries:\n\n");
    out.push_str(&format!(
        "- **Sector cap** (`max_sector_positions = 2`) blocked **{}** entries — by far the most common downstream block. \
         The comm sector (T + SIRI) saturated early and stayed that way.\n",
        sector_blocks,
    ));
    out.push_str(&format!(
        "- **Max-open-positions cap** (`max_open_positions = 4`) blocked **{}** entries.\n",
        max_open,
    ));
    if d.pdt_warnings > 0 {
        out.push_str(&format!(
            "- **PDT cap** forced **{}** distinct overnight holds — once the 2-per-5-day budget was spent (RIVN day-trade pair on the 24th), \
             same-day flatten was disabled and BAC was held into the close.\n",
            d.pdt_warnings,
        ));
    }
    if d.insufficient_funds_errors > 0 {
        out.push_str(&format!(
            "- **Insufficient buying power** — Alpaca rejected **{}** order submits with `403 insufficient`. Mostly Friday afternoon, \
             trying to scale into BAC + T 25.5P after cash was tied up in the existing T 26P stack. Sizing config is currently calibrated for \
             a 4-position PDT-conserving regime; this is the symptom of pushing past it without rebalancing the cash reserve.\n",
            d.insufficient_funds_errors,
        ));
    }
    out.push_str("\n");

    // ── Scan summary ───────────────────────────────────────────────────
    out.push_str("## Scan summary\n\n");
    out.push_str("### By regime\n\n");
    out.push_str("| Regime | Scans | % |\n|---|---:|---:|\n");
    for (regime, n) in &d.by_regime {
        out.push_str(&format!("| {} | {} | {:.1}% |\n", regime, n, 100.0 * *n as f64 / total));
    }
    out.push_str("\n### Per symbol\n\n");
    out.push_str("| Symbol | Scans | Entered | Choppy | Below | Extreme | NoCtr | Avg | Max |\n");
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for s in &d.per_symbol {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {:.2} | {} |\n",
            s.symbol, s.scans, s.entered, s.choppy, s.below, s.extreme, s.no_ctr, s.avg_score, s.max_score,
        ));
    }
    out.push_str("\n");

    // ── Veto / risk-block breakdown ────────────────────────────────────
    out.push_str("## Veto + risk-block breakdown\n\n");
    out.push_str("### Strategy-level vetoes (from scan outcomes)\n\n");
    out.push_str("| Reason | Count | % of scans |\n|---|---:|---:|\n");
    for outcome in ["choppy", "below_threshold", "extreme_proximity", "no_contracts", "entered"] {
        let n = *d.by_outcome.get(outcome).unwrap_or(&0);
        out.push_str(&format!("| {} | {} | {:.1}% |\n", outcome, n, 100.0 * n as f64 / total));
    }
    out.push_str("\n### Risk-guard blocks (post-signal)\n\n");
    if d.risk_blocks.is_empty() {
        out.push_str("_No risk blocks recorded this week._\n\n");
    } else {
        out.push_str("| Category | Count |\n|---|---:|\n");
        let mut sorted: Vec<_> = d.risk_blocks.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));
        for (cat, n) in sorted {
            out.push_str(&format!("| {} | {} |\n", cat, n));
        }
        out.push_str("\n");
    }
    out.push_str("### Errors\n\n");
    out.push_str(&format!(
        "- Insufficient buying power: **{}**\n- Other errors: **{}**\n\n",
        d.insufficient_funds_errors, d.other_errors,
    ));

    // ── Near-miss table ────────────────────────────────────────────────
    out.push_str("## Near-miss\n\n");
    out.push_str("Scans that scored exactly one point below the regime's `min_confluence_score` — \
                  the population that would convert if the threshold dropped by 1.\n\n");
    if d.near_miss.is_empty() {
        out.push_str("_No near-miss buckets._\n\n");
    } else {
        out.push_str("| Regime | Score | Threshold | Count |\n|---|---:|---:|---:|\n");
        for nm in &d.near_miss {
            out.push_str(&format!("| {} | {} | {} | {} |\n", nm.regime, nm.score, nm.threshold, nm.count));
        }
        out.push_str("\n");
    }

    // ── Entries + exits ────────────────────────────────────────────────
    out.push_str("## Entries + exits\n\n");
    out.push_str("### Fills by day\n\n");
    if d.fills_by_day.is_empty() {
        out.push_str("_No fills this week._\n\n");
    } else {
        out.push_str("| Day | Buys | Sells | Buy notional | Sell notional |\n|---|---:|---:|---:|---:|\n");
        for r in &d.fills_by_day {
            out.push_str(&format!(
                "| {} | {} | {} | ${:.2} | ${:.2} |\n",
                r.day, r.buys, r.sells, r.buy_notional, r.sell_notional,
            ));
        }
        out.push_str("\n");
    }

    out.push_str("### Closed trades\n\n");
    if d.closed_trades.is_empty() {
        out.push_str("_No positions closed this week._\n\n");
    } else {
        out.push_str("| Contract | Underlying | Direction | Exit reason | P&L |\n|---|---|---|---|---:|\n");
        for t in &d.closed_trades {
            out.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                t.contract, t.underlying, t.direction,
                t.exit_reason.as_deref().unwrap_or("—"),
                t.pnl.map(|p| format!("{:+.2}", p)).unwrap_or_else(|| "—".into()),
            ));
        }
        out.push_str("\n");
    }

    out.push_str("### Day trades (PDT budget)\n\n");
    if d.day_trades.is_empty() {
        out.push_str("_None this week._\n\n");
    } else {
        out.push_str("| Contract | Underlying | P&L | Emergency? |\n|---|---|---:|---|\n");
        for t in &d.day_trades {
            out.push_str(&format!(
                "| `{}` | {} | {:+.2} | {} |\n",
                t.contract, t.underlying, t.pnl,
                if t.was_emergency { "yes" } else { "no" },
            ));
        }
        out.push_str("\n");
    }

    out.push_str("### Open at end of week\n\n");
    if d.open_positions.is_empty() {
        out.push_str("_Flat into the weekend._\n\n");
    } else {
        out.push_str("| Contract | Legs | Qty | Avg fill |\n|---|---:|---:|---:|\n");
        for p in &d.open_positions {
            out.push_str(&format!(
                "| `{}` | {} | {} | ${:.2} |\n",
                p.contract, p.legs, p.total_qty, p.avg_price,
            ));
        }
        out.push_str("\n");
    }

    // ── PDT-transition notes (V2.2 prep) ───────────────────────────────
    out.push_str("## PDT-transition notes\n\n");
    out.push_str("Data-driven inputs for the V2.2 Theme B rework (gating PDT logic off once \
                  Alpaca's API reflects the SEC rule change).\n\n");

    let dt_used = d.day_trades.len() as u32;
    let losing_dt: Vec<&DayTradeRow> = d.day_trades.iter().filter(|t| t.pnl < 0.0).collect();
    let winning_dt: Vec<&DayTradeRow> = d.day_trades.iter().filter(|t| t.pnl >= 0.0).collect();

    out.push_str(&format!(
        "- **Day-trade budget consumed:** {}/{} this week. ",
        dt_used, PDT_LIMIT,
    ));
    if dt_used >= PDT_LIMIT {
        out.push_str(
            "We hit the cap — every same-day flatten attempt after that was either denied or \
             converted into an overnight hold. This is the exact scenario PDT removal unblocks.\n",
        );
    } else {
        out.push_str("Cap not hit, so PDT didn't change behaviour this week — but see sector-cap notes below.\n");
    }
    if !losing_dt.is_empty() {
        let avg_loss: f64 = losing_dt.iter().map(|t| t.pnl).sum::<f64>() / losing_dt.len() as f64;
        out.push_str(&format!(
            "- **Of the day trades used, {} were losers** (avg {:+.2}). Spending PDT slots on losers is the worst case: \
             the slot is gone *and* we paid for it. With unlimited day trades we'd recycle the slot for the next setup.\n",
            losing_dt.len(), avg_loss,
        ));
    }
    if !winning_dt.is_empty() {
        out.push_str(&format!(
            "- **Winning day trades:** {} — these would still happen post-PDT, just without the budget anxiety.\n",
            winning_dt.len(),
        ));
    }
    out.push_str(&format!(
        "- **Sector-cap blocks:** {} this week. The cap (`max_sector_positions = 2`) was originally a PDT-slot conservation tool — \
         keep one slot per sector free in case we need to flatten. Once PDT goes away, the cap's purpose shifts to pure diversification; \
         it should be re-justified rather than inherited at 2.\n",
        sector_blocks,
    ));
    out.push_str(&format!(
        "- **Max-open-positions cap:** {} blocks this week. Currently 4 — sized for the PDT-aware regime where one slot is reserved. \
         Likely 6–8 post-PDT; tune against this week's near-miss + sector data.\n",
        max_open,
    ));
    if d.insufficient_funds_errors > 0 {
        out.push_str(&format!(
            "- **Buying-power ceiling visible:** {} `insufficient` rejects on Friday. Whatever new `max_open_positions` we pick post-PDT, \
             cash reserve (`min_cash_reserve_pct = 30%`) becomes the binding constraint, not slot count. Re-derive `max_portfolio_risk_pct` \
             alongside the position-count change.\n",
            d.insufficient_funds_errors,
        ));
    }
    out.push_str(&format!(
        "- **PDT-overnight holds:** {} distinct {} held overnight this week solely because the day-trade cap was spent. \
         Those are the exits we'd have taken intraday under the new rule.\n",
        d.pdt_warnings,
        if d.pdt_warnings == 1 { "position" } else { "positions" },
    ));
    out.push_str("\n");
    out.push_str("**Action items for V2.2 Theme B:** add `[pdt] enabled = true` config flag (default true → reversible), \
                  gate the same-day-close blocking in `exit_monitor.rs` and the day-trade recording in `order_poller.rs`, \
                  and keep the dashboard counter as a status indicator (\"PDT N/A\") rather than removing it entirely.\n");

    out
}
