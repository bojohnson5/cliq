mod dig2;
mod event;
mod felib;

pub use crate::dig2::Dig2;
pub use crate::event::EventWrapper;
pub use crate::felib::FELibReturn;

pub struct AcqControl {
    pub dig: Dig2,
    pub ep_configured: bool,
    pub acq_started: bool,
    pub num_ch: usize,
}
