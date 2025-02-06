#![allow(dead_code)]

use rust_daq::*;
use std::{
    io,
    sync::{Arc, Condvar, Mutex},
    thread,
};

const EVENT_FORMAT: &str = " \
	[ \
		{ \"name\" : \"TIMESTAMP\", \"type\" : \"U64\" }, \
		{ \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
		{ \"name\" : \"WAVEFORM\", \"type\" : \"U16\", \"dim\" : 2 }, \
		{ \"name\" : \"WAVEFORM_SIZE\", \"type\" : \"SIZE_T\", \"dim\" : 1 }, \
		{ \"name\" : \"EVENT_SIZE\", \"type\" : \"SIZE_T\" } \
	] \
";
const TEST_FORMAT: &str = " \
	[ \
		{ \"name\" : \"TIMESTAMP\", \"type\" : \"U64\" }, \
		{ \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" } \
	] \
";

fn main() -> Result<(), FELibError> {
    // connect to digitizer
    let dev_handle = felib_open("dig2://caendgtz-usb-25380")?;

    // print dev details
    let model = felib_getvalue(dev_handle, "/par/ModelName")?;
    println!("model: {model}");
    let serialnum = felib_getvalue(dev_handle, "/par/SerialNum")?;
    println!("serialnum: {serialnum}");
    let adc_nbit = felib_getvalue(dev_handle, "/par/ADC_Nbit")?;
    println!("adc_nbit: {adc_nbit}");
    let numch = felib_getvalue(dev_handle, "/par/NumCh")?;
    println!("num ch: {numch}");
    let samplerate = felib_getvalue(dev_handle, "/par/ADC_SamplRate")?;
    println!("sample rate: {samplerate}");
    let cupver = felib_getvalue(dev_handle, "/par/cupver")?;
    println!("cup ver: {cupver}");

    // get num channels
    let num_chan = numch.parse::<usize>().map_err(|_| FELibError::Unknown)?;

    // reset
    felib_sendcommand(dev_handle, "/cmd/reset")?;

    // send acq_control to a new thread where it will configure endpoints and get ready
    // to read events
    let acq_control = AcqControl {
        dev_handle,
        ep_configured: false,
        acq_started: false,
        num_ch: num_chan,
    };
    let acq_control = Arc::new((Mutex::new(acq_control), Condvar::new()));
    let shared_acq_control = Arc::clone(&acq_control);

    let handle = thread::spawn(|| data_taking(shared_acq_control));

    // configure digitizer before running
    felib_setvalue(dev_handle, "/ch/0/par/ChEnable", "true")?;
    felib_setvalue(dev_handle, "/par/RecordLengthS", "1024")?;
    felib_setvalue(dev_handle, "/par/PreTriggerS", "100")?;
    felib_setvalue(dev_handle, "/par/AcqTriggerSource", "SwTrg | TestPulse")?;
    felib_setvalue(dev_handle, "/par/TestPulsePeriod", "100000000.0")?;
    felib_setvalue(dev_handle, "/par/TestPulseWidth", "1000")?;
    felib_setvalue(dev_handle, "/ch/0/par/DCOffset", "50.0")?;

    // wait for endpoint configuration before data taking
    let (control, cond) = &*acq_control;
    {
        let started = control.lock().unwrap();
        drop(
            cond.wait_while(started, |state| !state.ep_configured)
                .unwrap(),
        );
    }
    // begin acquisition
    felib_sendcommand(dev_handle, "/cmd/armacquisition")?;
    felib_sendcommand(dev_handle, "/cmd/swstartacquisition")?;

    {
        let (control, cond) = &*acq_control;
        let mut started = control.lock().unwrap();
        started.acq_started = true;
        cond.notify_one();
    }

    // watch for commands from user
    println!("##############################");
    println!("Commands supported:");
    println!("\t[t]\tsend manual trigger");
    println!("\t[s]\tstop acquisition");
    println!("##############################");

    let mut quit = false;
    while !quit {
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .expect("error getting input");
        match input.trim() {
            "t" => felib_sendcommand(dev_handle, "/cmd/sendswtrigger")?,
            "s" => quit = true,
            _ => (),
        }
    }

    felib_sendcommand(dev_handle, "/cmd/disarmacquisition")?;

    let _ = handle.join().unwrap();

    felib_close(dev_handle)?;

    println!("TTFN!");

    Ok(())
}

fn data_taking(acq_control: Arc<(Mutex<AcqControl>, Condvar)>) -> Result<(), FELibError> {
    let (control, cond) = &*acq_control;
    // configure endpoint
    let mut ep_handle = 0;
    let mut ep_folder_handle = 0;
    felib_gethandle(
        control.lock().unwrap().dev_handle,
        "/endpoint/scope",
        &mut ep_handle,
    )?;
    felib_getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
    felib_setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
    felib_setreaddataformat(ep_handle, TEST_FORMAT)?;

    {
        let mut started = control.lock().unwrap();
        started.ep_configured = true;
        cond.notify_one();
    }

    {
        let started = control.lock().unwrap();
        drop(
            cond.wait_while(started, |state| !state.acq_started)
                .unwrap(),
        );
    }

    Ok(())
}
