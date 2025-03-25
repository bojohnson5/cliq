use core::str;
use crossterm::terminal;
use rust_daq::*;
use std::{
    io::{stdin, Read},
    sync::{mpsc, Arc, Condvar, Mutex},
    thread,
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

fn getch() -> std::io::Result<[u8; 1]> {
    terminal::enable_raw_mode()?;
    let mut stdin = stdin();
    let mut buf = [0];
    stdin.read_exact(&mut buf)?;
    terminal::disable_raw_mode()?;
    Ok(buf)
}

/// Structure representing an event coming from a board.
#[derive(Debug)]
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
    num_ch: usize,
    tx: mpsc::Sender<BoardEvent>,
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
    let mut event = EventWrapper::new(num_ch, 1024);
    loop {
        let ret = felib_readdata(ep_handle, &mut event);
        match ret {
            FELibReturn::Success => {
                // Instead of allocating a new EventWrapper,
                // swap out the current one using std::mem::replace.
                let board_event = BoardEvent {
                    board_id,
                    event: std::mem::replace(&mut event, EventWrapper::new(num_ch, 1024)),
                };
                tx.send(board_event).expect("Failed to send event");
            }
            FELibReturn::Timeout => continue,
            FELibReturn::Stop => {
                println!("\nBoard {}: Stop received.", board_id);
                break;
            }
            _ => (),
        }
    }
    Ok(())
}

fn main() -> Result<(), FELibReturn> {
    let num_chan = 64;

    // List of board connection strings. Add as many as needed.
    let board_urls = vec![
        "dig2://caendgtz-usb-25380",
        "dig2://caendgtz-usb-25381",
        // e.g., "dig2://caendgtz-usb-25382",
    ];

    // Open boards and store their handles along with an assigned board ID.
    let mut boards = Vec::new();
    for (i, url) in board_urls.iter().enumerate() {
        let dev_handle = felib_open(url)?;
        println!("\nBoard {} details:", i + 1);
        print_dig_details(dev_handle)?;
        boards.push((i + 1, dev_handle));
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
        felib_setvalue(dev_handle, "/ch/0..63/par/ChEnable", "true")?;
        felib_setvalue(dev_handle, "/par/RecordLengthS", "1024")?;
        felib_setvalue(dev_handle, "/par/PreTriggerS", "100")?;
        felib_setvalue(dev_handle, "/par/AcqTriggerSource", "SwTrg | TestPulse")?;
        felib_setvalue(dev_handle, "/par/TestPulsePeriod", "100000000.0")?;
        felib_setvalue(dev_handle, "/par/TestPulseWidth", "1000")?;
        felib_setvalue(dev_handle, "/ch/0..63/par/DCOffset", "50.0")?;
    }
    println!("done.");

    // Shared signal for acquisition start.
    let acq_start = Arc::new((Mutex::new(false), Condvar::new()));
    // Shared counter for endpoint configuration.
    let endpoint_configured = Arc::new((Mutex::new(0u32), Condvar::new()));

    // Channel to receive events from board threads.
    let (tx, rx) = mpsc::channel::<BoardEvent>();

    // Spawn a data-taking thread for each board.
    let mut board_threads = Vec::new();
    for &(board_id, dev_handle) in &boards {
        let acq_start_clone = Arc::clone(&acq_start);
        let endpoint_configured_clone = Arc::clone(&endpoint_configured);
        let tx_clone = tx.clone();
        let handle = thread::spawn(move || {
            data_taking_thread(
                board_id,
                dev_handle,
                num_chan,
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

    // Begin acquisition on all boards.
    print!("Starting acquisitions...\t");
    for &(_, dev_handle) in &boards {
        felib_sendcommand(dev_handle, "/cmd/armacquisition")?;
        felib_sendcommand(dev_handle, "/cmd/swstartacquisition")?;
    }
    println!("done.");

    // Spawn a dedicated thread to process incoming events and print global stats.
    let event_processing_handle = thread::spawn(move || {
        let mut stats = Counter::new();
        let print_interval = Duration::from_secs(1);
        let mut last_print = Instant::now();
        loop {
            // Use a blocking recv with timeout to periodically print stats.
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(board_event) => {
                    stats.increment(board_event.event.c_event.event_size);
                    // You can also log which board the event came from if needed.
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // If no event is received within the timeout, check if it's time to print.
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
            if last_print.elapsed() >= print_interval {
                println!(
                    "\nGlobal stats: Elapsed time: {} s\tEvents: {}\tData rate: {:.3} MB/s",
                    stats.t_begin.elapsed().as_secs(),
                    stats.n_events,
                    (stats.total_size as f64)
                        / stats.t_begin.elapsed().as_secs_f64()
                        / (1024.0 * 1024.0)
                );
                last_print = Instant::now();
            }
        }
        // Final stats printout.
        println!(
            "\nFinal global stats: Total time: {} s\tTotal events: {}\tAverage rate: {:.3} MB/s",
            stats.t_begin.elapsed().as_secs(),
            stats.n_events,
            (stats.total_size as f64) / stats.t_begin.elapsed().as_secs_f64() / (1024.0 * 1024.0)
        );
    });

    // Clone board handles for the user input thread.
    let boards_for_input = boards.clone();

    // Spawn a dedicated thread to listen for user input.
    let input_handle = thread::spawn(move || loop {
        println!("#################################");
        println!("Commands supported:");
        println!("\t[t]\tSend manual trigger to all boards");
        println!("\t[s]\tStop acquisition");
        println!("#################################");
        if let Ok(c) = getch() {
            if c[0] == b't' {
                for &(_, dev_handle) in &boards_for_input {
                    felib_sendcommand(dev_handle, "/cmd/sendswtrigger").ok();
                }
            } else if c[0] == b's' {
                println!("User requested stop. Stopping acquisition...");
                for &(_, dev_handle) in &boards_for_input {
                    felib_sendcommand(dev_handle, "/cmd/disarmacquisition").ok();
                }
                break;
            }
        } else {
            println!("Error reading input.");
        }
    });

    // Main thread waits for a timeout duration.
    let timeout_duration = Duration::from_secs(10);
    let start_time = Instant::now();
    while start_time.elapsed() < timeout_duration {
        thread::sleep(Duration::from_millis(100));
    }
    println!("Timeout reached. Stopping acquisition...");

    // Stop acquisition on all boards.
    for &(_, dev_handle) in &boards {
        felib_sendcommand(dev_handle, "/cmd/disarmacquisition").ok();
    }

    // Close the tx channel so that the event processing thread can exit.
    drop(tx);

    // Wait for the input, event processing, and board threads to finish.
    input_handle.join().expect("User input thread panicked");
    event_processing_handle
        .join()
        .expect("Event processing thread panicked");
    for handle in board_threads {
        handle.join().expect("A board thread panicked");
    }

    // Close all boards.
    for &(_, dev_handle) in &boards {
        felib_close(dev_handle)?;
    }

    println!("TTFN!");

    Ok(())
}
