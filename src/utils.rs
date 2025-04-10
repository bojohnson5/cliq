use crate::EventWrapper;
use std::time::Instant;

/// Structure representing an event coming from a board.
#[derive(Debug)]
#[allow(dead_code)]
pub struct BoardEvent {
    pub board_id: usize,
    pub event: EventWrapper,
}

/// A helper structure to track statistics.
#[derive(Clone, Copy, Debug)]
pub struct Counter {
    pub total_size: usize,
    pub n_events: usize,
    pub t_begin: Instant,
}

impl std::default::Default for Counter {
    fn default() -> Self {
        Self {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
        }
    }
}

#[allow(dead_code)]
impl Counter {
    pub fn new() -> Self {
        Self {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
        }
    }

    pub fn from(counter: &Self) -> Self {
        Self {
            total_size: counter.total_size,
            n_events: counter.n_events,
            t_begin: counter.t_begin,
        }
    }

    pub fn rate(&self) -> f64 {
        (self.total_size as f64) / self.t_begin.elapsed().as_secs_f64() / (1024.0 * 1024.0)
    }

    pub fn increment(&mut self, size: usize) {
        self.total_size += size;
        self.n_events += 1;
    }

    pub fn reset(&mut self) {
        self.total_size = 0;
        self.n_events = 0;
        self.t_begin = Instant::now();
    }
}
