use crate::{
    ChannelConfig, Conf, DCOffsetConfig, EventWrapper, FELibReturn, ITLConnect, SamplesOverThr,
    TriggerEdge, TriggerThr, TriggerThrMode,
};
use std::{collections::VecDeque, time::Instant};

/// Structure representing an event coming from a board.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BoardEvent {
    pub board_id: usize,
    pub event: EventWrapper,
    pub zero_suppressed: bool,
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
}

impl Default for Counter {
    fn default() -> Self {
        Counter {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
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
        }
    }

    /// Long-term average rate since t_begin, in MB/s
    pub fn average_rate(&self) -> f64 {
        let secs = self.t_begin.elapsed().as_secs_f64();
        (self.total_size as f64 / secs) / (1024.0 * 1024.0)
    }

    /// Record an event of `size` bytes.
    /// Updates both the all-time totals and the sliding window.
    pub fn increment(&mut self, size: usize) {
        self.total_size += size;
        self.n_events += 1;
    }

    /// Reset both all-time counters and the sliding window.
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.total_size = 0;
        self.n_events = 0;
        self.t_begin = now;
    }
}

pub fn configure_board(board_id: usize, handle: u64, config: &Conf) -> Result<(), FELibReturn> {
    match config.board_settings.boards[board_id].en_chans {
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
    match config.board_settings.boards[board_id].dc_offset {
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
        &config.board_settings.common.record_len.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/PreTriggerS",
        &config.board_settings.common.pre_trig_len.to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/AcqTriggerSource",
        &config.board_settings.boards[board_id].trig_source,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/IOlevel",
        &config.board_settings.boards[board_id].io_level,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulsePeriod",
        &config.board_settings.boards[board_id]
            .test_pulse_period
            .to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseWidth",
        &config.board_settings.boards[board_id]
            .test_pulse_width
            .to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseLowLevel",
        &config.board_settings.boards[board_id]
            .test_pulse_low
            .to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TestPulseHighLevel",
        &config.board_settings.boards[board_id]
            .test_pulse_high
            .to_string(),
    )?;
    match config.board_settings.boards[board_id].trig_thr {
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
    match config.board_settings.boards[board_id].trig_thr_mode {
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
    match config.board_settings.boards[board_id].trig_edge {
        TriggerEdge::Fall => {
            crate::felib_setvalue(handle, "/ch/0..63/par/SelfTriggerEdge", "Fall")?;
        }
        TriggerEdge::Rise => {
            crate::felib_setvalue(handle, "/ch/0..63/par/SelfTriggerEdge", "Rise")?;
        }
    }
    match config.board_settings.boards[board_id].samples_over_thr {
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
        &config.board_settings.boards[board_id].itl_logic,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAMajorityLev",
        &config.board_settings.boards[board_id]
            .itl_majority_level
            .to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAPairLogic",
        &config.board_settings.boards[board_id].itl_pair_logic,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAPolarity",
        &config.board_settings.boards[board_id].itl_polarity,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAGateWidth",
        &config.board_settings.boards[board_id]
            .itl_gatewidth
            .to_string(),
    )?;
    crate::felib_setvalue(
        handle,
        "/par/ITLAEnRetrigger",
        &config.board_settings.boards[board_id].itl_retrig,
    )?;
    match config.board_settings.boards[board_id].itl_connect {
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
    board_id: usize,
    num_boards: usize,
    config: &Conf,
) -> Result<(), FELibReturn> {
    crate::felib_setvalue(
        handle,
        "/par/ClockSource",
        &config.sync_settings.boards[board_id].clock_src,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/SyncOutMode",
        &config.sync_settings.boards[board_id].sync_out,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/StartSource",
        &config.sync_settings.boards[board_id].start_source,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/EnClockOutFP",
        &config.sync_settings.boards[board_id].clock_out_fp,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/EnAutoDisarmAcq",
        &config.sync_settings.boards[board_id].auto_disarm,
    )?;
    crate::felib_setvalue(
        handle,
        "/par/TrgOutMode",
        &config.sync_settings.boards[board_id].trig_out,
    )?;

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

fn get_clock_out_delay(board_id: usize, num_boards: usize) -> isize {
    let first_board = board_id == 0;
    let last_board = board_id == num_boards - 1;

    if last_board {
        0
    } else if first_board {
        -2148
    } else {
        -3111
    }
}

fn get_run_delay(board_id: usize, num_boards: usize) -> usize {
    let first_board = board_id == 0;
    let board_id_from_last = num_boards - board_id - 1;

    let mut run_delay_clk = 2 * board_id_from_last;

    if first_board {
        run_delay_clk += 4;
    }

    run_delay_clk * 8
}

/// Repeatedly drops “stale” events from each queue until all
/// non‑empty queue fronts share the same trigger ID (or until
/// one queue becomes empty), counting each drop in `misaligned_count`.
pub fn align_queues(queues: &mut [VecDeque<BoardEvent>], misaligned_count: &mut usize) {
    loop {
        // If any queue is empty, we can’t fully align
        if queues.iter().any(|q| q.front().is_none()) {
            break;
        }

        // Gather all front trigger IDs
        let ids = queues
            .iter()
            .map(|q| q.front().unwrap().event.c_event.trigger_id)
            .collect::<Vec<_>>();

        // If they’re already all equal, we’re done
        if ids.windows(2).all(|w| w[0] == w[1]) {
            break;
        }

        // Otherwise drop any event whose ID is less than the current maximum
        let max_id = *ids.iter().max().unwrap();
        for q in queues.iter_mut() {
            while let Some(e) = q.front() {
                let tid = e.event.c_event.trigger_id;
                if tid < max_id {
                    q.pop_front();
                    *misaligned_count += 1;
                } else {
                    break;
                }
            }
        }
    }
}
