mod config;
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
