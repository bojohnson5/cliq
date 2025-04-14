use crate::{BoardEvent, Conf, Counter, EventWrapper, FELibReturn, HDF5Writer};
use anyhow::{anyhow, Result};
use crossbeam_channel::{tick, unbounded, Receiver, RecvError, Sender};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
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
    FELib(FELibReturn),
}

impl From<FELibReturn> for DaqError {
    fn from(value: FELibReturn) -> Self {
        Self::FELib(value)
    }
}

#[derive(Default, Clone, Copy)]
struct RunInfo {
    board0_event_size: usize,
    board1_event_size: usize,
    event_channel_buf: usize,
}

impl RunInfo {
    fn event_size(&self) -> usize {
        self.board0_event_size + self.board1_event_size
    }
}

#[derive(Debug)]
pub struct Status {
    pub counter: Counter,
    pub t_begin: Instant,
    pub run_duration: Duration,
    pub run_num: usize,
    pub camp_num: usize,
    pub curr_run: usize,
    pub buffer_len: usize,
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

impl Status {
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let ticker = tick(Duration::from_secs(1));
        let max_runs = self.max_runs.unwrap_or(0);

        loop {
            self.t_begin = Instant::now();
            self.exit = None;
            self.counter.reset();
            self.buffer_len = 0;

            let shutdown = Arc::new(AtomicBool::new(false));
            let (tx_stats, rx_stats) = unbounded();
            let (tx_events, ev_handle, board_handles) =
                self.begin_run(Arc::clone(&shutdown), tx_stats)?;

            while self.exit.is_none() {
                let _ = ticker.recv();

                // Drain stats channel
                while let Ok(run_info) = rx_stats.try_recv() {
                    self.counter.increment(run_info.event_size());
                    self.buffer_len = run_info.event_channel_buf;
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
                                        Some(String::from("Misaligned events. Quitting DAQ."));
                                }
                                DaqError::DroppedEvents => {
                                    self.show_popup =
                                        Some(String::from("Events dropped. Quitting DAQ."))
                                }
                                DaqError::FELib(val) => self.show_popup = Some(val.to_string()),
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
                                self.show_popup =
                                    Some(String::from("Misaligned events. Quitting DAQ."));
                            }
                            DaqError::DroppedEvents => {
                                self.show_popup =
                                    Some(String::from("Events dropped. Quitting DAQ."));
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
            // else (Timeout) start next run automatically
        }
    }

    pub fn new(config: Conf, boards: Vec<(usize, u64)>, max_runs: Option<usize>) -> Self {
        let run_duration = Duration::from_secs(config.run_settings.run_duration);
        let camp_num = config.run_settings.campaign_num;
        Self {
            counter: Counter::default(),
            t_begin: Instant::now(),
            run_num: 0,
            camp_num,
            curr_run: 0,
            show_popup: None,
            exit: None,
            buffer_len: 0,
            config,
            boards,
            max_runs,
            run_duration,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        let outer_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(frame.area());

        let inner_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer_layout[1]);

        let run_stats = self.run_stats_paragraph();
        frame.render_widget(run_stats, outer_layout[0]);

        let board0_status = self.board_status_paragraph(0);
        frame.render_widget(board0_status, inner_layout[0]);

        let board1_status = self.board_status_paragraph(1);
        frame.render_widget(board1_status, inner_layout[1]);

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
            KeyCode::Char('q') => self.exit(),
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

        let status_text = Text::from(vec![Line::from(vec![
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
            format!("{:.2}", self.counter.rate()).yellow(),
            " MB/s ".into(),
            " Buffer length: ".into(),
            self.buffer_len.to_string().yellow(),
        ])]);

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
    let mut writer = HDF5Writer::new(
        run_file,
        64,
        config.board_settings.record_len,
        config.run_settings.boards.len(),
        7500,
        50,
    )
    .unwrap();
    let mut queue0 = VecDeque::new();
    let mut queue1 = VecDeque::new();
    let mut curr_trig_id = 0;
    loop {
        match rx.recv() {
            Ok(mut board_event) => {
                zero_suppress(&mut board_event);
                match board_event.board_id {
                    0 => {
                        queue0.push_back(board_event);
                    }
                    1 => {
                        queue1.push_back(board_event);
                    }
                    _ => unreachable!(),
                }
            }
            Err(RecvError) => {
                writer.flush_all().unwrap();
                break;
            }
        }
        if queue0.front().is_some() && queue1.front().is_some() {
            let event0 = queue0.pop_front().unwrap();
            let event1 = queue1.pop_front().unwrap();
            let curr_trig0 = event0.event.c_event.trigger_id;
            let curr_trig1 = event1.event.c_event.trigger_id;
            if curr_trig0 != curr_trig1 {
                return Err(DaqError::MisalignedEvents);
            }
            if curr_trig0 != curr_trig_id {
                return Err(DaqError::DroppedEvents);
            }
            curr_trig_id += 1;
            let run_info = RunInfo {
                board0_event_size: event0.event.c_event.event_size,
                board1_event_size: event1.event.c_event.event_size,
                event_channel_buf: rx.len(),
            };
            if tx_stats.send(run_info).is_err() {
                break;
            }
            writer
                .append_event(
                    event0.board_id,
                    event0.event.c_event.timestamp,
                    &event0.event.waveform_data,
                )
                .unwrap();
            writer
                .append_event(
                    event1.board_id,
                    event1.event.c_event.timestamp,
                    &event1.event.waveform_data,
                )
                .unwrap();
        }
        if shutdown.load(Ordering::SeqCst) {
            writer.flush_all().unwrap();
            break;
        }
    }

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
    let waveform_len = config.board_settings.record_len;
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
                    break;
                }
            }
            FELibReturn::Timeout => continue,
            FELibReturn::Stop => {
                break;
            }
            _ => (),
        }
    }

    drop(tx);
    Ok(())
}

fn zero_suppress(board_data: &mut BoardEvent) {
    board_data
        .event
        .waveform_data
        .par_map_inplace(|adc| *adc = 0);
}
