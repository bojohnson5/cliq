mod config;
mod event;
mod felib;
mod utils;
mod writer;

pub use config::{ChannelConfig, Conf, DCOffsetConfig};
pub use event::*;
pub use felib::*;
pub use utils::*;
pub use writer::*;

pub struct AcqControl {
    pub dev_handle: u64,
    pub ep_configured: bool,
    pub acq_started: bool,
    pub num_ch: usize,
}
