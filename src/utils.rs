use crate::{
    ChannelConfig, Conf, DCOffsetConfig, EventWrapper, FELibReturn, ITLConnect, SamplesOverThr,
    TriggerEdge, TriggerThr, TriggerThrMode,
};
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

pub fn configure_board(handle: u64, config: &Conf) -> Result<(), FELibReturn> {
    match config.board_settings.en_chans {
        ChannelConfig::All(_) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/ChEnable", "true")?;
        }
        ChannelConfig::List(ref channels) => {
            for channel in channels {
                let path = format!("/ch/{}/par/ChEnable", channel);
                crate::felib_setvalue(handle, &path, "true")?;
            }
        }
    }
    match config.board_settings.dc_offset {
        DCOffsetConfig::Global(offset) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/DCOffset", &offset.to_string())?;
        }
        DCOffsetConfig::PerChannel(ref map) => {
            for (chan, offset) in map {
                let path = format!("/ch/{}/par/DCOffset", chan);

                crate::felib_setvalue(handle, &path, &offset.to_string())?;
            }
        }
    }
    crate::felib_setvalue(
        handle,
        "/par/RecordLengthS",
        &config.board_settings.record_len.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/PreTriggerS",
        &config.board_settings.pre_trig_len.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/AcqTriggerSource",
        &config.board_settings.trig_source,
    )?;
    crate::felib_setvalue(handle, "/par/IOlevel", &config.board_settings.io_level)?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulsePeriod",
        &config.board_settings.test_pulse_period.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseWidth",
        &config.board_settings.test_pulse_width.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseLowLevel",
        &config.board_settings.test_pulse_low.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseHighLevel",
        &config.board_settings.test_pulse_high.to_string(),
    )?;
    match config.board_settings.trig_thr {
        TriggerThr::Global(thr) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/TriggerThr", &thr.to_string())?;
        }
        TriggerThr::PerChannel(ref map) => {
            for (chan, thr) in map {
                let path = format!("/ch/{}/par/TriggerThr", chan);

                crate::felib_setvalue(handle, &path, &thr.to_string())?;
            }
        }
    }
    match config.board_settings.trig_thr_mode {
        TriggerThrMode::Global(ref mode) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/TriggerThrMode", mode)?;
        }
        TriggerThrMode::PerChannel(ref map) => {
            for (chan, mode) in map {
                let path = format!("/ch/{}/par/TriggerThrMode", chan);

                crate::felib_setvalue(handle, &path, mode)?;
            }
        }
    }
    match config.board_settings.trig_edge {
        TriggerEdge::Global(ref edge) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/SelfTriggerEdge", edge)?;
        }
        TriggerEdge::PerChannel(ref map) => {
            for (chan, edge) in map {
                let path = format!("/ch/{}/par/SelfTriggerEdge", chan);

                crate::felib_setvalue(handle, &path, edge)?;
            }
        }
    }
    match config.board_settings.samples_over_thr {
        SamplesOverThr::Global(samples) => {
            crate::felib_setvalue(
                handle,
                "/ch/0..63/par/SamplesOverThreshold",
                &samples.to_string(),
            )?;
        }
        SamplesOverThr::PerChannel(ref map) => {
            for (chan, samples) in map {
                let path = format!("/ch/{}/par/SamplesOverThreshold", chan);

                crate::felib_setvalue(handle, &path, &samples.to_string())?;
            }
        }
    }
    crate::felib_setvalue(
        handle,
        "/par/ITLAMainLogic",
        &config.board_settings.itl_logic,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAMajorityLev",
        &config.board_settings.itl_majority_level.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAPairLogic",
        &config.board_settings.itl_pair_logic,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAPolarity",
        &config.board_settings.itl_polarity,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAGateWidth",
        &config.board_settings.itl_gatewidth.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAEnRetrigger",
        &config.board_settings.itl_retrig,
    )?;
    match config.board_settings.itl_connect {
        ITLConnect::Global(ref connect) => {
            crate::felib_setvalue(handle, "/ch/0..63/par/ITLConnect", connect)?;
        }
        ITLConnect::PerChannel(ref map) => {
            for (chan, connect) in map {
                let path = format!("/ch/{}/par/ITLConnect", chan);

                crate::felib_setvalue(handle, &path, connect)?;
            }
        }
    }

    Ok(())
}

pub fn configure_sync(
    handle: u64,
    board_id: isize,
    num_boards: isize,
    config: &Conf,
) -> Result<(), FELibReturn> {
    let first_board = board_id == 0;

    crate::felib_setvalue(
        handle,
        "/par/ClockSource",
        if first_board {
            &config.sync_settings.primary_clock_src
        } else {
            &config.sync_settings.secondary_clock_src
        },
    )?;
    crate::felib_setvalue(
        handle,
        "/par/SyncOutMode",
        if first_board {
            &config.sync_settings.primary_sync_out
        } else {
            &config.sync_settings.secondary_sync_out
        },
    )?;
    crate::felib_setvalue(
        handle,
        "/par/StartSource",
        if first_board {
            &config.sync_settings.primary_start_source
        } else {
            &config.sync_settings.secondary_start_source
        },
    )?;
    crate::felib_setvalue(
        handle,
        "/par/EnClockOutFP",
        if first_board {
            &config.sync_settings.primary_clock_out_fp
        } else {
            &config.sync_settings.secondary_clock_out_fp
        },
    )?;
    crate::felib_setvalue(
        handle,
        "/par/EnAutoDisarmAcq",
        &config.sync_settings.auto_disarm,
    )?;
    crate::felib_setvalue(handle, "/par/TrgOutMode", &config.sync_settings.trig_out)?;

    let run_delay = get_run_delay(board_id, num_boards);
    let clock_out_delay = get_clock_out_delay(board_id, num_boards);
    crate::felib_setvalue(handle, "/par/RunDelay", &run_delay.to_string())?;
    crate::felib_setvalue(
        handle,
        "/par/VolatileClockOutDelay",
        &clock_out_delay.to_string(),
    )?;

    Ok(())
}

fn get_clock_out_delay(board_id: isize, num_boards: isize) -> isize {
    let first_board = board_id == 0;
    let last_board = board_id == num_boards - 1;

    if last_board {
        0
    } else if first_board {
        // -2148
        -2188
    } else {
        -3111
    }
}

fn get_run_delay(board_id: isize, num_boards: isize) -> isize {
    let first_board = board_id == 0;
    let board_id_from_last = num_boards - board_id - 1;

    let mut run_delay_clk = 2 * board_id_from_last;

    if first_board {
        run_delay_clk += 4;
    }

    run_delay_clk * 8
}
