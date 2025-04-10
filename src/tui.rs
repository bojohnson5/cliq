use crate::{BoardEvent, Conf, Counter, EventWrapper, FELibReturn, HDF5Writer};
use anyhow::Result;
use crossbeam_channel::{tick, unbounded, Receiver, RecvTimeoutError, Sender};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    symbols::border,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
    DefaultTerminal, Frame,
};
use std::{
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

        loop {
            self.curr_run += 1;
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
                while let Ok((sz, buf)) = rx_stats.try_recv() {
                    self.counter.increment(sz);
                    self.buffer_len = buf;
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
                h.join().expect("board thread panic");
            }
            // drop tx_events so event thread will exit
            drop(tx_events);
            // wait for event‐processing to finish
            ev_handle.join().expect("event thread panic")?;

            // if user quit, break out of the outer loop
            if let Some(StatusExit::Quit) = self.exit {
                // Close all boards
                for &(_, dev_handle) in &self.boards {
                    crate::felib_close(dev_handle)?;
                }
                return Ok(());
            }
            // else (Timeout) start next run automatically
        }
    }

    pub fn new(config: Conf, boards: Vec<(usize, u64)>) -> Self {
        let run_duration = Duration::from_secs(config.run_settings.run_duration);
        let camp_num = config.run_settings.campaign_num;
        Self {
            counter: Counter::default(),
            t_begin: Instant::now(),
            run_duration,
            run_num: 0,
            camp_num,
            curr_run: 0,
            config,
            boards,
            exit: None,
            buffer_len: 0,
        }
    }

    fn draw(&self, frame: &mut Frame) {
        frame.render_widget(self, frame.area());
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

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Char('q') => self.exit(),
            _ => {}
        }
    }

    fn exit(&mut self) {
        self.exit = Some(StatusExit::Quit);
    }

    fn begin_run(
        &mut self,
        shutdown: Arc<AtomicBool>,
        tx_stats: Sender<(usize, usize)>,
    ) -> Result<(
        Sender<BoardEvent>,
        JoinHandle<Result<()>>,
        Vec<JoinHandle<()>>,
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
                .unwrap_or_else(|e| eprintln!("Board {} error: {:?}", board_id, e));
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
        let boards_clone = self.boards.clone();
        let shutdown_clone = Arc::clone(&shutdown);
        let event_processing_handle = thread::spawn(move || -> Result<()> {
            event_processing(
                rx_events,
                tx_stats,
                run_file,
                config_clone,
                boards_clone,
                shutdown_clone,
            )
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

impl Widget for &Status {
    fn render(self, area: Rect, buf: &mut Buffer) {
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

        Paragraph::new(status_text)
            .centered()
            .block(block)
            .render(area, buf);
    }
}

fn event_processing(
    rx: Receiver<BoardEvent>,
    tx_stats: Sender<(usize, usize)>,
    run_file: PathBuf,
    config: Conf,
    boards: Vec<(usize, u64)>,
    shutdown: Arc<AtomicBool>,
) -> Result<()> {
    let board_handles: Vec<u64> = boards.iter().map(|(_, h)| *h).collect();
    let mut prev_len = 0;
    let mut writer =
        HDF5Writer::new(run_file, 64, config.board_settings.record_len, 7500, 50).unwrap();
    loop {
        // Use a blocking recv with timeout to periodically print stats.
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(board_event) => {
                // stats.increment(board_event.event.c_event.event_size);
                if tx_stats
                    .send((board_event.event.c_event.event_size, rx.len()))
                    .is_err()
                {
                    break;
                }
                // You can also log which board the event came from if needed.
                writer
                    .append_event(
                        board_event.board_id,
                        board_event.event.c_event.timestamp,
                        &board_event.event.waveform_data,
                    )
                    .unwrap();
            }
            Err(RecvTimeoutError::Timeout) => {
                // If no event is received within the timeout, check if it's time to print.
            }
            Err(RecvTimeoutError::Disconnected) => {
                writer.flush_all().unwrap();
                break;
            }
        }
        if shutdown.load(Ordering::SeqCst) {
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
) -> Result<(), FELibReturn> {
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
        let ret = crate::felib_readdata(ep_handle, &mut event);
        match ret {
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
                // println!("Board {}: Stop received...", board_id);
                break;
            }
            _ => (),
        }
    }
    Ok(())
}
