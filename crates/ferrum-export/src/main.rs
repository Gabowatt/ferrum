/// ferrum-export — fill export and tax tooling (Phase 4 / Milestone 4.1)
///
/// Usage:
///   ferrum-export --from 2025-01-01 --to 2025-12-31 [--format csv|json]
///
/// This binary is a stub. Full implementation lands in Milestone 4.1.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "ferrum-export", about = "Export fills for tax/accounting")]
struct Args {
    #[arg(long, help = "Start date (YYYY-MM-DD)")]
    from: String,

    #[arg(long, help = "End date (YYYY-MM-DD)")]
    to: String,

    #[arg(long, default_value = "csv", help = "Output format: csv | json")]
    format: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    eprintln!(
        "ferrum-export: from={} to={} format={} — full implementation in Milestone 4.1",
        args.from, args.to, args.format
    );
}
