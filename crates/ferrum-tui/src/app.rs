use std::collections::VecDeque;
use ferrum_core::types::{BotStatus, FillRecord, LogEvent};

pub const LOG_RING_SIZE: usize = 500;

pub struct App {
    pub daemon_online: bool,
    pub bot_status:    BotStatus,
    pub mode:          String,

    pub pnl_today: f64,
    pub pnl_month: f64,
    pub pnl_year:  f64,

    pub fills:      Vec<FillRecord>,
    pub log_events: VecDeque<LogEvent>,

    pub log_scroll:  usize,
    pub tail_follow: bool,
    pub show_help:   bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            daemon_online: true,
            bot_status:    BotStatus::Idle,
            mode:          "PAPER".to_string(),
            pnl_today:     0.0,
            pnl_month:     0.0,
            pnl_year:      0.0,
            fills:         Vec::new(),
            log_events:    VecDeque::new(),
            log_scroll:    0,
            tail_follow:   true,
            show_help:     false,
        }
    }

    pub fn push_log(&mut self, event: LogEvent) {
        if self.log_events.len() >= LOG_RING_SIZE {
            self.log_events.pop_front();
        }
        self.log_events.push_back(event);
        if self.tail_follow {
            self.log_scroll = self.log_events.len().saturating_sub(1);
        }
    }
}
