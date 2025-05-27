use crate::{
    BoardEvent, Conf, Counter, EventWrapper, FELibReturn, HDF5Writer, ZeroSuppressionEdge,
};
use anyhow::{anyhow, Result};
use crossbeam_channel::{tick, unbounded, Receiver, RecvError, Sender};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use log::info;
use ndarray::Axis;
use ndarray::{parallel::prelude::*, s};
use rand::Rng;
use ratatui::{
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, Clear, Paragraph},
    DefaultTerminal, Frame,
};
use std::{
    collections::VecDeque,
    fs::DirEntry,
    path::PathBuf,
    time::{Duration, Instant},
};
use std::{sync::atomic::Ordering, thread::JoinHandle};
use std::{
    sync::{atomic::AtomicBool, Arc, Condvar, Mutex},
    thread,
};

#[derive(Debug)]
#[allow(dead_code)]
enum DaqError {
    MisalignedEvents,
    DroppedEvents,
    DataTakingTransit,
    EventProcessingTransit,
    FELib(FELibReturn),
}

impl From<FELibReturn> for DaqError {
    fn from(value: FELibReturn) -> Self {
        Self::FELib(value)
    }
}

#[derive(Default, Clone)]
struct RunInfo {
    pub event_sizes: Vec<usize>,
    pub event_channel_buf: usize,
    pub misaligned_events: usize,
    pub dropped_events: usize,
}

impl RunInfo {
    fn event_size(&self) -> usize {
        self.event_sizes.iter().sum()
    }
}

#[derive(Debug)]
pub struct Tui {
    pub counter: Counter,
    pub t_begin: Instant,
    pub run_duration: Duration,
    pub run_num: usize,
    pub camp_num: usize,
    pub curr_run: usize,
    pub buffer_len: usize,
    pub misaligned_events: usize,
    pub dropped_events: usize,
    pub config: Conf,
    pub boards: Vec<(usize, u64)>,
    pub max_runs: Option<usize>,
    pub show_popup: Option<String>,
    pub exit: Option<StatusExit>,
}

#[derive(Debug, Clone, Copy)]
pub enum StatusExit {
    Quit,
    Timeout,
}

impl Tui {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let ticker = tick(Duration::from_secs(1));
        let max_runs = self.max_runs.unwrap_or(0);

        loop {
            // draw terminal here before resetting everything
            terminal.draw(|f| self.draw(f))?;

            // Reset the boards and reconfigure everything for next run
            for &(_, dev_handle) in &self.boards {
                crate::felib_sendcommand(dev_handle, "/cmd/reset")?;
            }
            for &(i, dev_handle) in &self.boards {
                crate::configure_board(i, dev_handle, &self.config)?;
            }
            for &(i, dev_handle) in &self.boards {
                crate::configure_sync(dev_handle, i, self.boards.len(), &self.config)?;
            }
            info!("Reset and configured digitizer(s)");

            let shutdown = Arc::new(AtomicBool::new(false));
            let (tx_stats, rx_stats) = unbounded();
            let (tx_events, ev_handle, board_handles) =
                self.begin_run(Arc::clone(&shutdown), tx_stats)?;
            info!("Beginning run {}", self.run_num);

            self.t_begin = Instant::now();
            self.exit = None;
            self.counter.reset();
            self.buffer_len = 0;
            while self.exit.is_none() && !shutdown.load(Ordering::SeqCst) {
                let _ = ticker.recv();

                // Drain stats channel
                while let Ok(run_info) = rx_stats.try_recv() {
                    self.counter.increment(run_info.event_size());
                    self.buffer_len = run_info.event_channel_buf;
                    self.misaligned_events = run_info.misaligned_events;
                    self.dropped_events = run_info.dropped_events;
                }

                self.handle_events()?;

                if self.t_begin.elapsed() >= self.run_duration {
                    self.exit = Some(StatusExit::Timeout);
                }

                terminal.draw(|f| self.draw(f))?;
            }

            // If user quit, record that so outer loop can break
            if let Some(StatusExit::Quit) = self.exit {
                shutdown.store(true, Ordering::SeqCst);
            }

            // disarm boards
            for &(_, dev) in &self.boards {
                crate::felib_sendcommand(dev, "/cmd/disarmacquisition")?;
            }
            // join board threads
            for h in board_handles {
                match h.join() {
                    Err(_) => return Err(anyhow!("Data taking panic")),
                    Ok(inner) => {
                        if let Err(daq_err) = inner {
                            match daq_err {
                                DaqError::MisalignedEvents => {
                                    self.show_popup =
                                        Some(String::from("Misaligned events. Quitting DAQ.\n<q> to exit."));
                                }
                                DaqError::DroppedEvents => {
                                    self.show_popup =
                                        Some(String::from("Events dropped. Quitting DAQ.\n<q> to exit."))
                                }
                                DaqError::FELib(val) => self.show_popup = Some(val.to_string()),
                                DaqError::DataTakingTransit => {
                                    self.show_popup = Some(String::from(
                                        "Data taking pipeline error. Quitting DAQ.\n<q> to exit.",
                                    ))
                                }
                                DaqError::EventProcessingTransit => {
                                    self.show_popup = Some(String::from(
                                        "Event processing stats pipeline error. Quitting DAQ.\n<q> to exit.",
                                    ))
                                }
                            }
                            terminal.draw(|f| self.draw(f))?;
                            self.handle_error_event()?;
                        }
                    }
                }
            }
            // drop tx_events so event thread will exit
            drop(tx_events);
            // wait for event‐processing to finish
            match ev_handle.join() {
                Err(_) => return Err(anyhow!("Event processing panic")),
                Ok(inner) => {
                    if let Err(daq_err) = inner {
                        match daq_err {
                            DaqError::MisalignedEvents => {
                                self.show_popup = Some(String::from(
                                    "Misaligned events. Quitting DAQ.\n<q> to exit.",
                                ));
                            }
                            DaqError::DroppedEvents => {
                                self.show_popup = Some(String::from(
                                    "Events dropped. Quitting DAQ.\n<q> to exit.",
                                ));
                            }
                            _ => {}
                        }
                        terminal.draw(|f| self.draw(f))?;
                        self.handle_error_event()?;
                    }
                }
            }

            // if user quit, break out of the outer loop
            if let Some(StatusExit::Quit) = self.exit {
                // Close all boards
                for &(_, dev_handle) in &self.boards {
                    crate::felib_close(dev_handle)?;
                }
                return Ok(());
            }
            self.curr_run += 1;
            if self.curr_run == max_runs && max_runs != 0 {
                // Close all boards
                for &(_, dev_handle) in &self.boards {
                    crate::felib_close(dev_handle)?;
                }
                return Ok(());
            }
        }
    }

    pub fn new(config: Conf, boards: Vec<(usize, u64)>, max_runs: Option<usize>) -> Self {
        let run_duration = Duration::from_secs(config.run_settings.run_duration);
        let camp_num = config.run_settings.campaign_num;
        Self {
            counter: Counter::default(),
            t_begin: Instant::now(),
            run_num: 0,
            curr_run: 0,
            show_popup: None,
            exit: None,
            buffer_len: 0,
            camp_num,
            config,
            boards,
            max_runs,
            run_duration,
            misaligned_events: 0,
            dropped_events: 0,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(frame.area());

        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Fill(1); self.boards.len()])
            .split(outer_layout[1]);

        let run_stats = self.run_stats_paragraph();
        frame.render_widget(run_stats, outer_layout[0]);

        for &(i, _) in &self.boards {
            let board_status = self.board_status_paragraph(i);
            frame.render_widget(board_status, inner_layout[i]);
        }

        if let Some(err) = &self.show_popup {
            let block = Block::bordered().title("DAQ Error").bold();
            let daq_error = Paragraph::new(Text::from(err.as_str()))
                .centered()
                .block(block);
            let vertical = Layout::vertical([Constraint::Percentage(20)]).flex(Flex::Center);
            let horizontal = Layout::horizontal([Constraint::Percentage(60)]).flex(Flex::Center);
            let [area] = vertical.areas(frame.area());
            let [area] = horizontal.areas(area);
            frame.render_widget(Clear, area); //this clears out the background
            frame.render_widget(daq_error, area);
        }
    }

    fn handle_events(&mut self) -> Result<()> {
        if event::poll(Duration::ZERO)? {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                _ => {}
            };
        }
        Ok(())
    }

    fn handle_error_event(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_event(key_event)
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => {
                info!("User exited DAQ");
                self.exit()
            }
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = Some(StatusExit::Quit);
    }

    fn run_stats_paragraph(&self) -> Paragraph {
        let title =
            Line::from(format!(" Campaign {} Run {} Status ", self.camp_num, self.run_num).bold());
        let instructrions = Line::from(vec![" Quit ".into(), "<Q> ".blue().bold()]);
        let block = Block::bordered()
            .title(title.centered())
            .title_bottom(instructrions.centered())
            .border_set(border::THICK);

        let status_text = Text::from(vec![
            Line::from(vec![
                "Elapsed time: ".into(),
                self.counter
                    .t_begin
                    .elapsed()
                    .as_secs()
                    .to_string()
                    .yellow(),
                " s".into(),
                " Events: ".into(),
                self.counter.n_events.to_string().yellow(),
                " Data rate: ".into(),
                format!("{:.2}", self.counter.average_rate()).yellow(),
                " MB/s ".into(),
                " Buffer length: ".into(),
                self.buffer_len.to_string().yellow(),
            ]),
            Line::from(vec![
                "Misaligned events: ".into(),
                self.misaligned_events.to_string().yellow(),
                " Dropped events: ".into(),
                self.dropped_events.to_string().yellow(),
            ]),
        ]);

        Paragraph::new(status_text).centered().block(block)
    }

    fn board_status_paragraph(&self, board: usize) -> Paragraph {
        let title = Line::from(format!(" Board {} Status ", self.boards[board].0).bold());
        let block = Block::bordered()
            .title(title.centered())
            .border_set(border::THICK);
        let handle = self.boards[board].1;
        let mut status_text = vec![];
        match crate::felib_getvalue(handle, "/par/RealtimeMonitor") {
            Ok(s) => status_text.push(Line::from(format!("Realtime Monitor: {}", s).yellow())),
            Err(_) => status_text.push(Line::from("Realtime monitor: err in read".yellow())),
        };
        match crate::felib_getvalue(handle, "/par/DeadtimeMonitor") {
            Ok(s) => status_text.push(Line::from(format!("Deadtime Monitor: {}", s).yellow())),
            Err(_) => status_text.push(Line::from("Deadtime monitor: err in read".yellow())),
        };
        match crate::felib_getvalue(handle, "/par/TriggerCnt") {
            Ok(s) => status_text.push(Line::from(format!("Trigger count: {}", s).yellow())),
            Err(_) => status_text.push(Line::from("Trigger counts: err in read".yellow())),
        };
        match crate::felib_getvalue(handle, "/par/LostTriggerCnt") {
            Ok(s) => status_text.push(Line::from(format!("Lost trigger count: {}", s).yellow())),
            Err(_) => status_text.push(Line::from("Lost trigger count: err in read".yellow())),
        };
        match crate::felib_getvalue(handle, "/par/AcquisitionStatus") {
            Ok(s) => {
                // parse the status code as a number, then format as binary string
                let bin = format!("{:b}", s.parse::<u32>().unwrap());

                // build a Spans line: first the label, then one Span per bit
                let mut spans = Vec::with_capacity(1 + bin.len());
                spans.push(Span::raw("Acquisition status: ").yellow());
                spans.extend(bin.chars().map(|c| {
                    let (color, label) = match c {
                        '1' => (Color::Red, "●"),
                        '0' => (Color::White, "●"),
                        _ => (Color::White, "?"),
                    };
                    Span::styled(label, Style::default().fg(color))
                }));

                status_text.push(Line::from(spans));
            }
            Err(_) => status_text.push(Line::from("Acquisition status: err in read".yellow())),
        };

        Paragraph::new(status_text).centered().block(block)
    }

    fn begin_run(
        &mut self,
        shutdown: Arc<AtomicBool>,
        tx_stats: Sender<RunInfo>,
    ) -> Result<(
        Sender<BoardEvent>,
        JoinHandle<Result<(), DaqError>>,
        Vec<JoinHandle<Result<(), DaqError>>>,
    )> {
        // Shared signal for acquisition start.
        let acq_start = Arc::new((Mutex::new(false), Condvar::new()));
        // Shared counter for endpoint configuration.
        let endpoint_configured = Arc::new((Mutex::new(0u32), Condvar::new()));

        // Channel to receive events from board threads.
        let (tx_events, rx_events) = unbounded();

        // Spawn a data-taking thread for each board.
        let mut board_thread_handles = Vec::new();
        for &(board_id, dev_handle) in &self.boards {
            let config_clone = self.config.clone();
            let acq_start_clone = Arc::clone(&acq_start);
            let endpoint_configured_clone = Arc::clone(&endpoint_configured);
            let tx_clone = tx_events.clone();
            let shutdown_clone = Arc::clone(&shutdown);
            let handle = thread::spawn(move || {
                data_taking_thread(
                    board_id,
                    dev_handle,
                    config_clone,
                    tx_clone,
                    acq_start_clone,
                    endpoint_configured_clone,
                    shutdown_clone,
                )
            });
            board_thread_handles.push(handle);
        }

        // Wait until all boards have configured their endpoints.
        {
            let (lock, cond) = &*endpoint_configured;
            let mut count = lock.lock().unwrap();
            while *count < self.boards.len() as u32 {
                count = cond.wait(count).unwrap();
            }
        }

        // Signal acquisition start.
        {
            let (lock, cvar) = &*acq_start;
            let mut started = lock.lock().unwrap();
            *started = true;
            cvar.notify_all();
        }

        // Begin run acquisition.
        crate::felib_sendcommand(self.boards[0].1, "/cmd/swstartacquisition")?;

        // Create the appropriate directory for file-writing
        let run_file = self.create_run_file().unwrap();

        // Spawn a dedicated thread to process incoming events and print global stats.
        let config_clone = self.config.clone();
        let shutdown_clone = Arc::clone(&shutdown);
        let event_processing_handle = thread::spawn(move || -> Result<(), DaqError> {
            event_processing(rx_events, tx_stats, run_file, config_clone, shutdown_clone)
        });

        Ok((tx_events, event_processing_handle, board_thread_handles))
    }

    fn create_run_file(&mut self) -> Result<PathBuf> {
        let mut camp_dir = self.create_camp_dir().unwrap();
        let runs: Vec<DirEntry> = std::fs::read_dir(&camp_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        let max_run = runs
            .iter()
            .filter_map(|path| {
                path.file_name()
                    .to_str() // Get file name (OsStr)
                    .and_then(|filename| {
                        // Ensure the filename starts with "run"
                        if let Some(stripped) = filename.strip_prefix("run") {
                            // Split at '_' and take the first part
                            let parts: Vec<&str> = stripped.split('_').collect();
                            parts.first()?.parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
            })
            .max();

        if let Some(max) = max_run {
            let file = format!("run{}_0.h5", max + 1);
            camp_dir.push(&file);
            self.run_num = max + 1;
            Ok(camp_dir)
        } else {
            Ok(camp_dir.join("run0_0.h5"))
        }
    }

    fn create_camp_dir(&self) -> Result<PathBuf> {
        let camp_dir = format!(
            "{}/camp{}",
            self.config.run_settings.output_dir, self.config.run_settings.campaign_num
        );
        let path = PathBuf::from(camp_dir);
        if !std::fs::exists(&path).unwrap() {
            match std::fs::create_dir_all(&path) {
                Ok(_) => {
                    println!("Create campaign directory");
                }
                Err(e) => {
                    eprintln!("Error creating dir: {:?}", e)
                }
            }
        }

        Ok(path)
    }
}

fn event_processing(
    rx: Receiver<BoardEvent>,
    tx_stats: Sender<RunInfo>,
    run_file: PathBuf,
    config: Conf,
    shutdown: Arc<AtomicBool>,
) -> Result<(), DaqError> {
    info!("Started event processing thread");
    // new counters
    let mut misaligned_count = 0;
    let mut dropped_count = 0;
    let mut curr_trig_id = 0;

    let num_boards = config.run_settings.boards.len();
    let mut events = Vec::with_capacity(num_boards);

    let mut writer = HDF5Writer::new(
        run_file,
        64,
        config.board_settings.common.record_len,
        config.run_settings.boards.len(),
        7500,
        50,
        config.run_settings.blosc_threads,
        config.run_settings.compression_level,
    )
    .unwrap();

    let mut queues = Vec::with_capacity(num_boards);
    for _ in 0..num_boards {
        queues.push(VecDeque::new());
    }
    let mut rng = rand::rng();
    let zs_level = config.run_settings.zs_level;
    let zs_threshold = config.run_settings.zs_threshold;
    let zs_edge = config.run_settings.zs_edge;
    let zs_samples = config.run_settings.zs_samples;

    loop {
        match rx.recv() {
            Ok(mut board_event) => {
                let r: f64 = rng.random();
                if r > zs_level {
                    zero_suppress(&mut board_event, zs_threshold, zs_edge, zs_samples);
                }
                queues[board_event.board_id].push_back(board_event);
            }
            Err(RecvError) => {
                writer.flush_all().unwrap();
                break;
            }
        }

        if queues.iter().all(|q| q.front().is_some()) {
            // if queue0.front().is_some() && queue1.front().is_some() {
            crate::align_queues(&mut queues, &mut misaligned_count);

            if queues.iter().all(|q| q.front().is_some()) {
                // if let (Some(e0), Some(e1)) = (queue0.front(), queue1.front()) {
                let trgid = queues[0].front().unwrap().event.c_event.trigger_id;
                // let _trgid1 = e1.event.c_event.trigger_id;

                if trgid != curr_trig_id {
                    dropped_count += (trgid as isize - curr_trig_id as isize).abs() as usize;
                }

                curr_trig_id = trgid + 1;

                for queue in queues.iter_mut() {
                    events.push(queue.pop_front().unwrap());
                }

                let run_info = RunInfo {
                    event_sizes: events.iter().map(|e| e.event.c_event.event_size).collect(),
                    event_channel_buf: rx.len(),
                    misaligned_events: misaligned_count,
                    dropped_events: dropped_count,
                };

                if tx_stats.send(run_info).is_err() {
                    shutdown.store(true, Ordering::SeqCst);
                    return Err(DaqError::EventProcessingTransit);
                }

                for event in &events {
                    writer
                        .append_event(
                            event.board_id,
                            event.event.c_event.timestamp,
                            &event.event.waveform_data,
                            event.event.c_event.trigger_id,
                            event.event.c_event.flags,
                            event.event.c_event.board_fail,
                        )
                        .unwrap();
                }
                events.clear();
            }
        }

        if shutdown.load(Ordering::SeqCst) {
            writer.flush_all().unwrap();
            break;
        }
    }

    info!("Ending event processing thread");
    drop(tx_stats);
    Ok(())
}

/// Data-taking thread function for one board.
/// It configures the endpoint, signals that configuration is complete,
/// waits for the shared acquisition start signal, then continuously reads events and sends them.
fn data_taking_thread(
    board_id: usize,
    dev_handle: u64,
    config: Conf,
    tx: Sender<BoardEvent>,
    acq_start: Arc<(Mutex<bool>, Condvar)>,
    endpoint_configured: Arc<(Mutex<u32>, Condvar)>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), DaqError> {
    info!("Started data taking thread for board {board_id}");
    // Set up endpoint.
    let mut ep_handle = 0;
    let mut ep_folder_handle = 0;
    crate::felib_gethandle(dev_handle, "/endpoint/scope", &mut ep_handle)?;
    crate::felib_getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
    crate::felib_setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
    crate::felib_setreaddataformat(ep_handle, crate::EVENT_FORMAT)?;
    crate::felib_sendcommand(dev_handle, "/cmd/armacquisition")?;

    // Signal that this board's endpoint is configured.
    {
        let (lock, cond) = &*endpoint_configured;
        let mut count = lock.lock().unwrap();
        *count += 1;
        cond.notify_all();
    }

    // Wait for the acquisition start signal.
    {
        let (lock, cvar) = &*acq_start;
        let mut started = lock.lock().unwrap();
        while !*started {
            started = cvar.wait(started).unwrap();
        }
    }

    // Data-taking loop.
    // num_ch has to be 64 due to the way CAEN reads data from the board
    let num_ch = 64;
    let waveform_len = config.board_settings.common.record_len;
    let mut event = EventWrapper::new(num_ch, waveform_len);
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        match crate::felib_readdata(ep_handle, &mut event) {
            FELibReturn::Success => {
                // Instead of allocating a new EventWrapper,
                // swap out the current one using std::mem::replace.
                let board_event = BoardEvent {
                    board_id,
                    event: std::mem::replace(&mut event, EventWrapper::new(num_ch, waveform_len)),
                };
                if tx.send(board_event).is_err() {
                    shutdown.store(true, Ordering::SeqCst);
                    return Err(DaqError::DataTakingTransit);
                }
            }
            FELibReturn::Timeout => continue,
            FELibReturn::Stop => {
                break;
            }
            _ => (),
        }
    }

    info!("Ending data taking thread for board {board_id}");
    drop(tx);
    Ok(())
}

/// suppress adc samples from digitizer based on user-defined threshold
/// relative to baseline and whether or not the pulses are rising or
/// falling
fn zero_suppress(
    board_data: &mut BoardEvent,
    threshold: f64,
    edge: ZeroSuppressionEdge,
    bl_samples: isize,
) {
    board_data
        .event
        .waveform_data
        .axis_iter_mut(Axis(0))
        .into_par_iter()
        .for_each(|mut channel| match edge {
            ZeroSuppressionEdge::Rise => {
                let mut sum = 0.0;
                for val in channel.slice(s![0..bl_samples]) {
                    sum += *val as f64;
                }
                let baseline = sum / bl_samples as f64;
                channel.map_inplace(|adc| {
                    let x = *adc as f64;
                    if x - baseline < threshold {
                        *adc = 0
                    }
                })
            }
            ZeroSuppressionEdge::Fall => {
                let mut sum = 0.0;
                for val in channel.slice(s![0..bl_samples]) {
                    sum += *val as f64;
                }
                let baseline = sum / bl_samples as f64;
                channel.map_inplace(|adc| {
                    let x = *adc as f64;
                    if x - baseline > threshold {
                        *adc = 0
                    }
                })
            }
        });
}
