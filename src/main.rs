use rust_daq::*;
use std::{thread, time::Duration};

const EVENT_FORMAT: &str = " \
	[ \
		{ \"name\" : \"TIMESTAMP\", \"type\" : \"U64\" }, \
		{ \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
		{ \"name\" : \"WAVEFORM\", \"type\" : \"U16\", \"dim\" : 2 }, \
		{ \"name\" : \"WAVEFORM_SIZE\", \"type\" : \"SIZE_T\", \"dim\" : 1 }, \
		{ \"name\" : \"EVENT_SIZE\", \"type\" : \"SIZE_T\" } \
	] \
";

struct AcqControl {
    dev_handle: u64,
    ep_configured: bool,
    acq_started: bool,
    num_ch: usize,
}

struct Event {
    timestamp: u64,
    trigger_id: u32,
    waveforms: Vec<Vec<u16>>,
    waveform_size: Vec<usize>,
    event_size: usize,
}

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
    let num_chan: usize = numch.parse().unwrap();

    // reset
    felib_sendcommand(dev_handle, "/cmd/reset")?;

    // send acq_control to a new thread where it will configure endpoints and get ready
    // to read events
    let mut acq_control = AcqControl {
        dev_handle,
        ep_configured: false,
        acq_started: false,
        num_ch: num_chan,
    };

    // configure endpoint
    let mut ep_handle = 0;
    let mut ep_folder_handle = 0;
    felib_gethandle(dev_handle, "/endpoint/scope", &mut ep_handle)?;
    felib_getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
    felib_setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
    felib_setreaddataformat(ep_handle, EVENT_FORMAT)?;

    // for _ in 0..100 {
    //     felib_readdata(dev_handle, &mut data)?;
    //     println!("timestamp: {}", data.timestamp);
    //     thread::sleep(Duration::from_secs(5));
    // }

    Ok(())
}
