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
const TEST_FORMAT: &str = " \
	[ \
		{ \"name\" : \"TIMESTAMP\", \"type\" : \"U64\" }, \
		{ \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
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
    felib_setreaddataformat(ep_handle, TEST_FORMAT)?;

    let mut event = Event {
        timestamp: 0,
        trigger_id: 0,
        waveforms: vec![vec![0u16; 10]; 10],
        waveform_size: vec![0usize; 10],
        event_size: 0,
    };

    for _ in 0..10 {
        felib_readdata(dev_handle, &mut event)?;
        println!("timestamp: {}", event.timestamp);
        thread::sleep(Duration::from_secs(5));
    }

    felib_close(dev_handle)?;

    Ok(())
}
