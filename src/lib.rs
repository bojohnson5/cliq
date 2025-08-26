mod config;
mod digitizer_params;
mod event;
mod felib;
mod tui;
mod utils;
mod writer;

pub use config::*;
pub use event::*;
pub use felib::*;
pub use tui::*;
pub use utils::*;
pub use writer::*;

pub struct AcqControl {
    pub dev_handle: u64,
    pub ep_configured: bool,
    pub acq_started: bool,
    pub num_ch: usize,
}

pub const EVENT_FORMAT: &str = " \
    [ \
        { \"name\" : \"TIMESTAMP_NS\", \"type\" : \"U64\" }, \
        { \"name\" : \"TRIGGER_ID\", \"type\" : \"U32\" }, \
        { \"name\" : \"WAVEFORM\", \"type\" : \"U16\", \"dim\" : 2 }, \
        { \"name\" : \"WAVEFORM_SIZE\", \"type\" : \"SIZE_T\", \"dim\" : 1 }, \
        { \"name\" : \"FLAGS\", \"type\" : \"U16\" }, \
        { \"name\" : \"BOARD_FAIL\", \"type\" : \"BOOL\" }, \
        { \"name\" : \"EVENT_SIZE\", \"type\" : \"SIZE_T\" } \
    ] \
";
