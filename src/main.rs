use std::{thread, time::Duration};

use rust_daq::*;

fn main() -> Result<(), FELibError> {
    let dev_handle = felib_open("dig2://caendgtz-usb-25380")?;

    let mut data = Data {
        format: String::from(
            " \
	[ \
		{ \"name\" : \"TIMESTAMP\", \"type\" : \"U64\" }, \
		{ \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
		{ \"name\" : \"WAVEFORM\", \"type\" : \"U16\", \"dim\" : 2 }, \
		{ \"name\" : \"WAVEFORM_SIZE\", \"type\" : \"SIZE_T\", \"dim\" : 1 }, \
		{ \"name\" : \"EVENT_SIZE\", \"type\" : \"SIZE_T\" } \
	] \
",
        ),
        timestamp: 0,
    };

    felib_setreaddataformat(dev_handle, &data.format)?;

    for _ in 0..100 {
        felib_readdata(dev_handle, &mut data)?;
        println!("timestamp: {}", data.timestamp);
        thread::sleep(Duration::from_secs(5));
    }

    Ok(())
}
