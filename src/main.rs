use anyhow::Result;
use confique::{Config, FormatOptions};
use core::str;
use crossbeam_channel::{unbounded, Receiver, RecvTimeoutError, Sender};
use crossterm::cursor::{MoveTo, MoveToColumn, MoveToNextLine};
use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{self, Clear, ClearType};
use hdf5::{Dataset, File};
use ndarray::Array2;
use rust_daq::*;
use std::fs::DirEntry;
use std::path::PathBuf;
use std::{
    io::{stdout, Write},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const EVENT_FORMAT: &str = " \
    [ \
        { \"name\" : \"TIMESTAMP_NS\", \"type\" : \"U64\" }, \
        { \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
        { \"name\" : \"WAVEFORM\", \"type\" : \"U16\", \"dim\" : 2 }, \
        { \"name\" : \"WAVEFORM_SIZE\", \"type\" : \"SIZE_T\", \"dim\" : 1 }, \
        { \"name\" : \"EVENT_SIZE\", \"type\" : \"SIZE_T\" } \
    ] \
";

struct HDF5Writer {
    file: File,
    dataset: Dataset,
    current_rows: usize,
}

impl HDF5Writer {
    fn new(filename: &str, dataset_name: &str, n_cols: usize) -> hdf5::Result<Self> {
        let file = File::create(filename)?;

        // Create an extendable dataset with an unlimited max size
        let dataset = file
            .new_dataset::<u16>()
            .shape((0, n_cols)) // Start with 0 rows
            .chunk((64, n_cols)) // Set chunk size (tune for performance)
            .create(dataset_name)?;

        Ok(Self {
            file,
            dataset,
            current_rows: 0,
        })
    }

    fn append_waveform(&mut self, waveform: &Array2<u16>) -> hdf5::Result<()> {
        let (new_rows, n_cols) = waveform.dim();

        // Resize dataset to accommodate new data
        self.dataset
            .resize((self.current_rows + new_rows, n_cols))?;

        // Write new data at the correct position
        self.dataset.write_slice(
            waveform,
            (self.current_rows, ..), // Offset at current row
        )?;

        // Update row count
        self.current_rows += new_rows;

        Ok(())
    }
}

/// Structure representing an event coming from a board.
#[derive(Debug)]
#[allow(dead_code)]
struct BoardEvent {
    board_id: usize,
    event: EventWrapper,
}

/// A helper structure to track statistics.
#[derive(Clone, Copy, Debug)]
struct Counter {
    total_size: usize,
    n_events: usize,
    t_begin: Instant,
}

#[allow(dead_code)]
impl Counter {
    fn new() -> Self {
        Self {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
        }
    }

    fn from(counter: &Self) -> Self {
        Self {
            total_size: counter.total_size,
            n_events: counter.n_events,
            t_begin: counter.t_begin,
        }
    }

    fn increment(&mut self, size: usize) {
        self.total_size += size;
        self.n_events += 1;
    }
}

/// Prints details for a given board.
fn print_dig_details(handle: u64) -> Result<(), FELibReturn> {
    let model = felib_getvalue(handle, "/par/ModelName")?;
    println!("Model name:\t{model}");
    let serialnum = felib_getvalue(handle, "/par/SerialNum")?;
    println!("Serial number:\t{serialnum}");
    let adc_nbit = felib_getvalue(handle, "/par/ADC_Nbit")?;
    println!("ADC bits:\t{adc_nbit}");
    let numch = felib_getvalue(handle, "/par/NumCh")?;
    println!("Channels:\t{numch}");
    let samplerate = felib_getvalue(handle, "/par/ADC_SamplRate")?;
    println!("ADC rate:\t{samplerate}");
    let cupver = felib_getvalue(handle, "/par/cupver")?;
    println!("CUP version:\t{cupver}");
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
) -> Result<(), FELibReturn> {
    // Set up endpoint.
    let mut ep_handle = 0;
    let mut ep_folder_handle = 0;
    felib_gethandle(dev_handle, "/endpoint/scope", &mut ep_handle)?;
    felib_getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
    felib_setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
    felib_setreaddataformat(ep_handle, EVENT_FORMAT)?;
    felib_sendcommand(dev_handle, "/cmd/armacquisition")?;

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
        let ret = felib_readdata(ep_handle, &mut event);
        match ret {
            FELibReturn::Success => {
                // Instead of allocating a new EventWrapper,
                // swap out the current one using std::mem::replace.
                let board_event = BoardEvent {
                    board_id,
                    event: std::mem::replace(&mut event, EventWrapper::new(num_ch, waveform_len)),
                };
                tx.send(board_event).expect("Failed to send event");
            }
            FELibReturn::Timeout => continue,
            FELibReturn::Stop => {
                print_status(
                    &format!("Board {}: Stop received...\n", board_id),
                    false,
                    true,
                    false,
                );
                break;
            }
            _ => (),
        }
    }
    Ok(())
}

fn main() -> Result<(), FELibReturn> {
    let config = Conf::from_file("config.toml").map_err(|_| FELibReturn::InvalidParam)?;

    // List of board connection strings. Add as many as needed.
    let board_urls = vec!["dig2://caendgtz-usb-25380", "dig2://caendgtz-usb-25379"];

    // Open boards and store their handles along with an assigned board ID.
    let mut boards = Vec::new();
    for (i, url) in board_urls.iter().enumerate() {
        let dev_handle = felib_open(url)?;
        println!("\nBoard {} details:", i);
        print_dig_details(dev_handle)?;
        boards.push((i, dev_handle));
    }

    // Reset all boards.
    print!("\nResetting boards...\t");
    for &(_, dev_handle) in &boards {
        felib_sendcommand(dev_handle, "/cmd/reset")?;
    }
    println!("done.");

    // Configure all boards.
    print!("Configuring boards...\t");
    for &(_, dev_handle) in &boards {
        configure_board(dev_handle, &config)?;
    }
    println!("done.");

    // Configure sync settings
    print!("Configuring sync...\t");
    for &(i, dev_handle) in &boards {
        configure_sync(dev_handle, i as isize, board_urls.len() as isize, &config)?;
    }
    println!("done.");

    let mut quit = false;
    let (tx_user, rx_user) = unbounded();

    // Spawn a dedicated thread to listen for user input.
    input_thread(tx_user);
    while !quit {
        let timeout_duration = Duration::from_secs(config.run_settings.run_duration);
        let (tx, event_processing_handle, board_threads) = begin_run(&config, &boards)?;

        match rx_user.recv_timeout(timeout_duration) {
            Ok(c) => match c {
                's' => {
                    print_status("Quitting DAQ...", false, true, false);
                    // Stop acquisition on all boards.
                    for &(_, dev_handle) in &boards {
                        felib_sendcommand(dev_handle, "/cmd/disarmacquisition")?;
                    }
                    // Close the tx channel so that the event processing thread can exit.
                    drop(tx);

                    // Wait for the input, event processing, and board threads to finish.
                    event_processing_handle
                        .join()
                        .expect("Event processing thread panicked");
                    for handle in board_threads {
                        handle.join().expect("A board thread panicked");
                    }
                    quit = true;
                }
                't' => {
                    for &(_, dev_handle) in &boards {
                        felib_sendcommand(dev_handle, "/cmd/sendswtrigger")?;
                    }
                }
                _ => {
                    println!("OK received: read {:?} from user", c);
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                print_status("Ending run...", false, true, false);
                // Stop acquisition on all boards.
                for &(_, dev_handle) in &boards {
                    felib_sendcommand(dev_handle, "/cmd/disarmacquisition")?;
                }
                // Close the tx channel so that the event processing thread can exit.
                drop(tx);

                // Wait for the input, event processing, and board threads to finish.
                event_processing_handle
                    .join()
                    .expect("Event processing thread panicked");
                for handle in board_threads {
                    handle.join().expect("A board thread panicked");
                }
            }
            _ => (),
        }
    }

    terminal::disable_raw_mode().expect("Failed to disable raw mode");
    // Close all boards.
    for &(_, dev_handle) in &boards {
        felib_close(dev_handle)?;
    }

    println!("\nTTFN!");

    Ok(())
}

fn get_clock_out_delay(board_id: isize, num_boards: isize) -> isize {
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

fn get_run_delay(board_id: isize, num_boards: isize) -> isize {
    let first_board = board_id == 0;
    let board_id_from_last = num_boards - board_id - 1;

    let mut run_delay_clk = 2 * board_id_from_last;

    if first_board {
        run_delay_clk += 4;
    }

    run_delay_clk * 8
}

fn configure_board(handle: u64, config: &Conf) -> Result<(), FELibReturn> {
    match config.board_settings.en_chans {
        ChannelConfig::All(_) => {
            felib_setvalue(handle, "/ch/0..63/par/ChEnable", "true")?;
        }
        ChannelConfig::List(ref channels) => {
            for channel in channels {
                let path = format!("/ch/{}/par/ChEnable", channel);
                felib_setvalue(handle, &path, "true")?;
            }
        }
    }
    match config.board_settings.dc_offset {
        DCOffsetConfig::Global(offset) => {
            felib_setvalue(handle, "/ch/0..63/par/DCOffset", &offset.to_string())?;
        }
        DCOffsetConfig::PerChannel(ref map) => {
            for (chan, offset) in map {
                let path = format!("/ch/{}/par/DCOffset", chan);

                felib_setvalue(handle, &path, &offset.to_string())?;
            }
        }
    }
    felib_setvalue(
        handle,
        "/par/RecordLengthS",
        &config.board_settings.record_len.to_string(),
    )?;
    felib_setvalue(
        handle,
        "/par/PreTriggerS",
        &config.board_settings.pre_trig_len.to_string(),
    )?;
    felib_setvalue(
        handle,
        "/par/AcqTriggerSource",
        &config.board_settings.trig_source,
    )?;
    felib_setvalue(handle, "/par/TestPulsePeriod", "1000000000")?;
    felib_setvalue(handle, "/par/TestPulseWidth", "1000")?;
    felib_setvalue(handle, "/par/TestPulseLowLevel", "0")?;
    felib_setvalue(handle, "/par/TestPulseHighLevel", "10000")?;

    Ok(())
}

fn configure_sync(
    handle: u64,
    board_id: isize,
    num_boards: isize,
    config: &Conf,
) -> Result<(), FELibReturn> {
    let first_board = board_id == 0;

    felib_setvalue(
        handle,
        "/par/ClockSource",
        if first_board {
            &config.sync_settings.primary_clock_src
        } else {
            &config.sync_settings.secondary_clock_src
        },
    )?;
    felib_setvalue(
        handle,
        "/par/SyncOutMode",
        if first_board {
            &config.sync_settings.primary_sync_out
        } else {
            &config.sync_settings.secondary_sync_out
        },
    )?;
    felib_setvalue(
        handle,
        "/par/StartSource",
        if first_board {
            &config.sync_settings.primary_start_source
        } else {
            &config.sync_settings.secondary_start_source
        },
    )?;
    felib_setvalue(
        handle,
        "/par/EnClockOutFP",
        if first_board {
            &config.sync_settings.primary_clock_out_fp
        } else {
            &config.sync_settings.secondary_clock_out_fp
        },
    )?;
    felib_setvalue(
        handle,
        "/par/EnAutoDisarmAcq",
        &config.sync_settings.auto_disarm,
    )?;
    felib_setvalue(handle, "/par/TrgOutMode", &config.sync_settings.trig_out)?;

    let run_delay = get_run_delay(board_id, num_boards);
    let clock_out_delay = get_clock_out_delay(board_id, num_boards);
    felib_setvalue(handle, "/par/RunDelay", &run_delay.to_string())?;
    felib_setvalue(
        handle,
        "/par/VolatileClockOutDelay",
        &clock_out_delay.to_string(),
    )?;

    Ok(())
}

fn event_processing(rx: Receiver<BoardEvent>, run_file: PathBuf) {
    let mut stats = Counter::new();
    let print_interval = Duration::from_secs(1);
    let mut last_print = Instant::now();

    let mut writer = HDF5Writer::new(run_file.to_str().unwrap(), "waveforms", 4125).unwrap();
    loop {
        // Use a blocking recv with timeout to periodically print stats.
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(board_event) => {
                stats.increment(board_event.event.c_event.event_size);
                // You can also log which board the event came from if needed.
                // writer
                //     .append_waveform(&board_event.event.waveform_data)
                //     .unwrap();
            }
            Err(RecvTimeoutError::Timeout) => {
                // If no event is received within the timeout, check if it's time to print.
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if last_print.elapsed() >= print_interval {
            print_status(
                &format!(
                    // "\x1b[1K\rElapsed time: {} s\tEvents: {}\tData rate: {:.3} MB/s",
                    "Elapsed time: {} s\tEvents: {}\tData rate: {:.3} MB/s\tIn queue: {}",
                    stats.t_begin.elapsed().as_secs(),
                    stats.n_events,
                    (stats.total_size as f64)
                        / stats.t_begin.elapsed().as_secs_f64()
                        / (1024.0 * 1024.0),
                    rx.len(),
                ),
                false,
                false,
                true,
            );
            last_print = Instant::now();
        }
    }
    // Final stats printout.
    print_status(
        &format!(
            "Total time: {} s\tTotal events: {}\tAverage rate: {:.3} MB/s",
            stats.t_begin.elapsed().as_secs(),
            stats.n_events,
            (stats.total_size as f64) / stats.t_begin.elapsed().as_secs_f64() / (1024.0 * 1024.0)
        ),
        false,
        false,
        true,
    );
}

fn begin_run(
    config: &Conf,
    boards: &Vec<(usize, u64)>,
) -> Result<(Sender<BoardEvent>, JoinHandle<()>, Vec<JoinHandle<()>>), FELibReturn> {
    print_status("Beginning new run", true, true, false);
    print_status("Press [s] to stop data acquisition", false, true, false);
    // Shared signal for acquisition start.
    let acq_start = Arc::new((Mutex::new(false), Condvar::new()));
    // Shared counter for endpoint configuration.
    let endpoint_configured = Arc::new((Mutex::new(0u32), Condvar::new()));

    // Channel to receive events from board threads.
    let (tx, rx) = unbounded();

    // Spawn a data-taking thread for each board.
    let mut board_threads = Vec::new();
    for &(board_id, dev_handle) in boards {
        let config_clone = config.clone();
        let acq_start_clone = Arc::clone(&acq_start);
        let endpoint_configured_clone = Arc::clone(&endpoint_configured);
        let tx_clone = tx.clone();
        let handle = thread::spawn(move || {
            data_taking_thread(
                board_id,
                dev_handle,
                config_clone,
                tx_clone,
                acq_start_clone,
                endpoint_configured_clone,
            )
            .unwrap_or_else(|e| eprintln!("Board {} error: {:?}", board_id, e));
        });
        board_threads.push(handle);
    }

    // Wait until all boards have configured their endpoints.
    {
        let (lock, cond) = &*endpoint_configured;
        let mut count = lock.lock().unwrap();
        while *count < boards.len() as u32 {
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
    print_status(
        "Starting acquisition on primary board...",
        false,
        true,
        false,
    );
    felib_sendcommand(boards[0].1, "/cmd/swstartacquisition")?;
    print_status("done.", false, false, false);

    // Create the appropriate directory for file-writing
    let run_file = create_run_file(config).unwrap();

    // Spawn a dedicated thread to process incoming events and print global stats.
    let event_processing_handle = thread::spawn(move || {
        event_processing(rx, run_file);
    });

    Ok((tx, event_processing_handle, board_threads))
}

fn input_thread(tx: Sender<char>) {
    thread::spawn(move || {
        // Enable raw mode once.
        terminal::enable_raw_mode().expect("Failed to enable raw mode");
        loop {
            // Poll with a short timeout.
            if event::poll(Duration::from_millis(100)).expect("Polling failed") {
                if let Event::Key(key_event) = event::read().expect("Read failed") {
                    // Send only one character.
                    if let KeyCode::Char(c) = key_event.code {
                        if tx.send(c).is_err() {
                            break;
                        }
                    }
                }
            }
        }
        terminal::disable_raw_mode().expect("Failed to disable raw mode");
    });
}

fn print_status(status: &str, clear_screen: bool, move_line: bool, clear_line: bool) {
    let mut stdout = stdout();
    if clear_screen {
        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0)).unwrap();
    }
    if move_line {
        execute!(stdout, MoveToNextLine(1)).unwrap();
    }
    if clear_line {
        execute!(stdout, Clear(ClearType::CurrentLine), MoveToColumn(0)).unwrap();
    }
    write!(stdout, "{}", status).unwrap();
    stdout.flush().unwrap();
}

fn create_run_file(config: &Conf) -> Result<PathBuf> {
    let mut camp_dir = create_camp_dir(&config).unwrap();
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
                        parts.first()?.parse::<u32>().ok()
                    } else {
                        None
                    }
                })
        })
        .max();

    if let Some(max) = max_run {
        let file = format!("run{}_0.h5", max);
        camp_dir.push(&file);
        Ok(camp_dir)
    } else {
        Ok(camp_dir.join("run0_0.h5"))
    }
}

fn create_camp_dir(config: &Conf) -> Result<PathBuf> {
    let camp_dir = format!(
        "{}/camp{}",
        config.run_settings.output_dir, config.run_settings.campaign_num
    );
    let path = PathBuf::from(camp_dir);
    if !std::fs::exists(&path).unwrap() {
        match std::fs::create_dir_all(&path) {
            Ok(_) => {
                print_status("Create campaign directory\n", false, true, false);
            }
            Err(e) => {
                print_status(
                    &format!("error creating dir: {:?}\n", e),
                    false,
                    true,
                    false,
                );
            }
        }
    }

    Ok(path)
}
