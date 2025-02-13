use confique::Config;
use core::str;
use crossterm::terminal;
use rust_daq::*;
use std::{
    io::{stdin, stdout, Read, Write},
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

    // Read one byte from stdin.
    let mut stdin = stdin();
    let mut buf = [0];
    stdin.read_exact(&mut buf)?;

    terminal::disable_raw_mode()?;

    Ok(buf)
}

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
        Self { ..*counter }
    }

    fn increment(&mut self, size: usize) {
        self.total_size += size;
        self.n_events += 1;
    }

    fn reset(&mut self) {
        self.total_size = 0;
        self.n_events = 0;
        self.t_begin = Instant::now();
    }
}

fn main() -> Result<(), FELibReturn> {
    // connect to digitizer
    let mut dig = Dig2::from_file("config.toml").map_err(|_| FELibReturn::DevNotFound)?;
    dig.open()?;
    dig.configure_endpoint()?;

    // print dev details
    let model = dig.getvalue("/par/ModelName")?;
    println!("Model name:\t{model}");
    let serialnum = dig.getvalue("/par/SerialNum")?;
    println!("Serial number:\t{serialnum}");
    let adc_nbit = dig.getvalue("/par/ADC_Nbit")?;
    println!("ADC bits:\t{adc_nbit}");
    let numch = dig.getvalue("/par/NumCh")?;
    println!("Channels:\t{numch}");
    let samplerate = dig.getvalue("/par/ADC_SamplRate")?;
    println!("ADC rate:\t{samplerate}");
    let cupver = dig.getvalue("/par/cupver")?;
    println!("CUP version:\t{cupver}");

    // get num channels
    let num_chan = numch.parse::<usize>().map_err(|_| FELibReturn::Unknown)?;

    // reset
    print!("Resetting...\t");
    dig.sendcommand("/cmd/reset")?;
    println!("done.");

    // send acq_control to a new thread where it will configure endpoints and get ready
    // to read events
    let acq_control = AcqControl {
        dig: dig.clone(),
        ep_configured: false,
        acq_started: false,
        num_ch: num_chan,
    };
    let acq_control = Arc::new((Mutex::new(acq_control), Condvar::new()));
    let shared_acq_control = Arc::clone(&acq_control);

    let handle = thread::spawn(|| data_taking(shared_acq_control));

    // configure digitizer before running
    print!("Configuring...\t");
    dig.setvalue("/ch/0..63/par/ChEnable", "true")?;
    dig.setvalue("/par/RecordLengthS", "1024")?;
    dig.setvalue("/par/PreTriggerS", "100")?;
    dig.setvalue("/par/AcqTriggerSource", "SwTrg | TestPulse")?;
    dig.setvalue("/par/TestPulsePeriod", "100000000.0")?;
    dig.setvalue("/par/TestPulseWidth", "1000")?;
    dig.setvalue("/ch/0..63/par/DCOffset", "50.0")?;
    println!("done.");

    // wait for endpoint configuration before data taking
    let (control, cond) = &*acq_control;
    {
        let mut started = control.lock().unwrap();
        while !started.ep_configured {
            started = cond.wait(started).unwrap();
        }
    }

    // begin acquisition
    print!("Starting...\t");
    dig.sendcommand("/cmd/armacquisition")?;
    dig.sendcommand("/cmd/swstartacquisition")?;
    println!("done.");

    {
        let mut started = control.lock().unwrap();
        started.acq_started = true;
        cond.notify_one();
    }

    let (tx, rx) = mpsc::channel();

    let _input_handle = thread::spawn(move || {
        // watch for commands from user
        println!("#################################");
        println!("Commands supported:");
        println!("\t[t]\tsend manual trigger");
        println!("\t[s]\tstop acquisition");
        println!("#################################");
        match getch() {
            Ok(c) => tx.send(c),
            Err(_) => {
                print!("error getting input");
                Ok(())
            }
        }
    });

    let mut quit = false;
    let timeout_duration = Duration::from_secs(10);
    while !quit {
        match rx.recv_timeout(timeout_duration) {
            Ok(c) => match &c {
                b"s" => quit = true,
                b"t" => dig.sendcommand("/cmd/sendswtrigger")?,
                _ => (),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {
                terminal::disable_raw_mode().map_err(|_| FELibReturn::Generic)?;
                println!("\nEnding run...");
                quit = true;
            }
            _ => (),
        }
    }

    // end acquisition
    print!("\nStopping...\t");
    dig.sendcommand("/cmd/disarmacquisition")?;
    println!("done.");

    let _ = handle.join().unwrap();

    dig.close()?;

    println!("TTFN!");

    Ok(())
}

fn data_taking(acq_control: Arc<(Mutex<AcqControl>, Condvar)>) -> Result<(), FELibReturn> {
    let (control, cond) = &*acq_control;
    // configure endpoint
    // let mut ep_handle = 0;
    // let mut ep_folder_handle = 0;
    // felib_gethandle(
    //     control.lock().unwrap().dig.handle,
    //     "/endpoint/scope",
    //     &mut ep_handle,
    // )?;
    // felib_getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
    // felib_setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
    // felib_setreaddataformat(ep_handle, EVENT_FORMAT)?;

    // signal main thread endpoint is configured
    {
        let mut started = control.lock().unwrap();
        started.ep_configured = true;
        cond.notify_one();
    }

    // wait on main thread to being acquisition
    {
        let mut started = control.lock().unwrap();
        while !started.acq_started {
            started = cond.wait(started).unwrap();
        }
    }

    let mut event = EventWrapper::new(control.lock().unwrap().num_ch, 1024);

    let mut total = Counter::new();
    let mut current = Counter::from(&total);
    let mut previous_time = Instant::now();

    loop {
        // print the run stats
        if Instant::now().duration_since(previous_time) > Duration::from_secs(1) {
            print!(
                "\x1b[1K\rTime (s): {}\tEvents: {}\tReadout rate (MB/s): {:.3}",
                total.t_begin.elapsed().as_secs(),
                total.n_events,
                current.total_size as f64
                    / current.t_begin.elapsed().as_secs_f64()
                    / (1024.0 * 1024.0)
            );
            stdout().flush().expect("couldn't flush stdout");
            current.reset();
            previous_time = Instant::now();
        }
        {
            let dig = control.lock().unwrap();
            let ret = dig.dig.readdata(&mut event);
            match ret {
                FELibReturn::Success => {
                    total.increment(event.c_event.event_size);
                    current.increment(event.c_event.event_size);
                }
                FELibReturn::Timeout => (),
                FELibReturn::Stop => {
                    println!("\nStop received.");
                    break;
                }
                _ => (),
            }
        }
        // let ret = felib_readdata(ep_handle, &mut event);
        // match ret {
        //     FELibReturn::Success => {
        //         total.increment(event.c_event.event_size);
        //         current.increment(event.c_event.event_size);
        //     }
        //     FELibReturn::Timeout => (),
        //     FELibReturn::Stop => {
        //         println!("\nStop received.");
        //         break;
        //     }
        //     _ => (),
        // }
    }

    Ok(())
}
