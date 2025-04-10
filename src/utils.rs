use crate::EventWrapper;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Structure representing an event coming from a board.
#[derive(Debug)]
#[allow(dead_code)]
pub struct BoardEvent {
    pub board_id: usize,
    pub event: EventWrapper,
}

/// A helper structure to track statistics, with both
/// *all-time* counters and a *sliding 1 s window* rate.
#[derive(Debug)]
pub struct Counter {
    /// All-time total bytes
    pub total_size: usize,
    /// All-time number of events
    pub n_events: usize,
    /// Time when this counter was created or last reset
    pub t_begin: Instant,

    // --- sliding window fields ---
    window: Duration,
    events: VecDeque<(Instant, usize)>,
    bytes_in_window: usize,
}

impl Default for Counter {
    fn default() -> Self {
        let now = Instant::now();
        Counter {
            total_size: 0,
            n_events: 0,
            t_begin: now,
            window: Duration::from_secs(1),
            events: VecDeque::new(),
            bytes_in_window: 0,
        }
    }
}

impl Counter {
    /// Create a new Counter with a 1 s sliding window.
    pub fn new() -> Self {
        Default::default()
    }

    /// Copy constructor
    pub fn from(other: &Self) -> Self {
        Counter {
            total_size: other.total_size,
            n_events: other.n_events,
            t_begin: other.t_begin,
            window: other.window,
            events: other.events.clone(),
            bytes_in_window: other.bytes_in_window,
        }
    }

    /// Long-term average rate since t_begin, in MB/s
    pub fn average_rate(&self) -> f64 {
        let secs = self.t_begin.elapsed().as_secs_f64().max(1e-6);
        (self.total_size as f64 / secs) / (1024.0 * 1024.0)
    }

    /// Sliding-window rate over the last `window` duration (default 1 s), in MB/s
    pub fn rate(&self) -> f64 {
        let secs = self.window.as_secs_f64().max(1e-6);
        (self.bytes_in_window as f64 / secs) / (1024.0 * 1024.0)
    }

    /// Record an event of `size` bytes.
    /// Updates both the all-time totals and the sliding window.
    pub fn increment(&mut self, size: usize) {
        let now = Instant::now();

        // 1) Update all-time stats
        self.total_size += size;
        self.n_events += 1;

        // 2) Push into sliding window
        self.events.push_back((now, size));
        self.bytes_in_window += size;

        // 3) Evict any entries older than `window`
        while let Some(&(ts, sz)) = self.events.front() {
            if now.duration_since(ts) > self.window {
                self.events.pop_front();
                self.bytes_in_window -= sz;
            } else {
                break;
            }
        }
    }

    /// Reset both all-time counters and the sliding window.
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.total_size = 0;
        self.n_events = 0;
        self.t_begin = now;

        self.events.clear();
        self.bytes_in_window = 0;
    }
}
