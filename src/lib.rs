#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

mod config;
pub use config::{ChannelConfig, Conf, DCOffsetConfig};

use std::ffi::CString;

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum FELibReturn {
    Success = 0,
    Generic = -1,
    InvalidParam = -2,
    DevAlreadyOpen = -3,
    DevNotFound = -4,
    MaxDev = -5,
    Command = -6,
    Internal = -7,
    NotImplemented = -8,
    InvalidHandle = -9,
    DevLibNotAvailable = -10,
    Timeout = -11,
    Stop = -12,
    Disabled = -13,
    BadLibVer = -14,
    Comm = -15,
    Unknown = 1,
}

impl From<i32> for FELibReturn {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Success,
            -1 => Self::Generic,
            -2 => Self::InvalidParam,
            -3 => Self::DevAlreadyOpen,
            -4 => Self::DevNotFound,
            -5 => Self::MaxDev,
            -6 => Self::Command,
            -7 => Self::Internal,
            -8 => Self::NotImplemented,
            -9 => Self::InvalidHandle,
            -10 => Self::DevLibNotAvailable,
            -11 => Self::Timeout,
            -12 => Self::Stop,
            -13 => Self::Disabled,
            -14 => Self::BadLibVer,
            -15 => Self::Comm,
            _ => Self::Unknown,
        }
    }
}

pub struct AcqControl {
    pub dev_handle: u64,
    pub ep_configured: bool,
    pub acq_started: bool,
    pub num_ch: usize,
}

#[repr(C)]
#[derive(Debug)]
pub struct CEvent {
    pub timestamp: u64,
    pub timestamp_us: f64,
    pub trigger_id: u32,
    pub event_size: usize,
    // waveform is an array of pointers (one per channel)
    pub waveform: *mut *mut u16,
    // Arrays (one element per channel) filled in by the C function
    pub n_samples: *mut usize,
    pub n_allocated_samples: *mut usize,
    pub n_channels: usize,
}

/// A safe wrapper that owns the allocated memory for a CEvent.
///
/// The inner `c_event` field can be passed to the C function, while the owned
/// buffers are automatically dropped when the wrapper goes out of scope.
#[allow(dead_code)]
#[derive(Debug)]
pub struct EventWrapper {
    pub c_event: CEvent,

    // Owned memory: the actual waveform buffers.
    waveform_buffers: Vec<Box<[u16]>>,
    // Owned slice of waveform pointers. We need to keep this alive so that
    // `c_event.waveform` (a raw pointer into it) remains valid.
    waveform_ptrs: Box<[*mut u16]>,
    // Owned memory for the per-channel arrays.
    n_samples: Box<[usize]>,
    n_allocated_samples: Box<[usize]>,
}

unsafe impl Send for EventWrapper {}

impl EventWrapper {
    /// Create a new EventWrapper.
    ///
    /// # Arguments
    ///
    /// * `n_channels` - Number of waveforms/channels.
    /// * `waveform_len` - Number of samples per waveform.
    pub fn new(n_channels: usize, waveform_len: usize) -> Self {
        // Allocate the individual waveform buffers.
        let mut waveform_buffers = Vec::with_capacity(n_channels);
        let mut waveform_ptrs_vec = Vec::with_capacity(n_channels);
        for _ in 0..n_channels {
            // Create a waveform buffer with the desired length.
            let mut buffer = vec![0u16; waveform_len].into_boxed_slice();
            // Get a mutable pointer to the buffer’s data.
            let ptr = buffer.as_mut_ptr();
            waveform_ptrs_vec.push(ptr);
            waveform_buffers.push(buffer);
        }
        // Box the slice of waveform pointers. This memory is owned by our wrapper.
        let mut waveform_ptrs = waveform_ptrs_vec.into_boxed_slice();

        // Allocate the arrays for n_samples and n_allocated_samples.
        let mut n_samples = vec![0usize; n_channels].into_boxed_slice();
        let mut n_allocated_samples = vec![0usize; n_channels].into_boxed_slice();

        // IMPORTANT: Use as_mut_ptr() here so that the returned pointer
        // is actually mutable.
        let waveform_ptr = waveform_ptrs.as_mut_ptr();
        let n_samples_ptr = n_samples.as_mut_ptr();
        let n_allocated_samples_ptr = n_allocated_samples.as_mut_ptr();

        // Build the C-compatible event. We obtain raw pointers from the boxes.
        let c_event = CEvent {
            timestamp: 0,
            timestamp_us: 0.0,
            trigger_id: 0,
            event_size: 0,
            waveform: waveform_ptr,
            n_samples: n_samples_ptr,
            n_allocated_samples: n_allocated_samples_ptr,
            n_channels,
        };

        Self {
            c_event,
            waveform_buffers,
            n_samples,
            n_allocated_samples,
            waveform_ptrs,
        }
    }
}

pub fn felib_getlibinfo() -> Result<String, FELibReturn> {
    let buffer_size = 1024;
    let mut buffer = vec![0u8; buffer_size];
    let res = unsafe { CAEN_FELib_GetLibInfo(buffer.as_mut_ptr() as *mut i8, buffer_size) };
    let res = FELibReturn::from(res);
    buffer.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(buffer).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getlibversion() -> Result<String, FELibReturn> {
    let mut libv = vec![0u8; 16];
    let res = unsafe { CAEN_FELib_GetLibVersion(libv.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    libv.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(libv).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_geterrorname(error: CAEN_FELib_ErrorCode) -> Result<String, FELibReturn> {
    let mut err_name = vec![0u8; 32];
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_name.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    err_name.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(err_name).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_geterrordesc(error: CAEN_FELib_ErrorCode) -> Result<String, FELibReturn> {
    let mut err_desc = vec![0u8; 256];
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_desc.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    err_desc.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(err_desc).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getlasterror() -> Result<String, FELibReturn> {
    let mut last_err = vec![0u8; 1024];
    let res = unsafe { CAEN_FELib_GetLibVersion(last_err.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    last_err.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(last_err).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_devicesdiscovery() -> Result<String, FELibReturn> {
    let buffer_size = 1024;
    let mut devices = vec![0u8; buffer_size];
    let res =
        unsafe { CAEN_FELib_DevicesDiscovery(devices.as_mut_ptr() as *mut i8, buffer_size, 5) };
    let res = FELibReturn::from(res);
    devices.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(devices).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_open(url: &str) -> Result<u64, FELibReturn> {
    let mut handle = 0;
    let url = CString::new(url).unwrap();
    let res = unsafe { CAEN_FELib_Open(url.as_ptr(), &mut handle) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(handle),
        _ => Err(res),
    }
}

pub fn felib_close(handle: u64) -> Result<(), FELibReturn> {
    let res = unsafe { CAEN_FELib_Close(handle) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_getimpllibversion(handle: u64) -> Result<String, FELibReturn> {
    let mut libv = vec![0u8; 16];
    let res = unsafe { CAEN_FELib_GetImplLibVersion(handle, libv.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    libv.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(libv).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getdevicetree(handle: u64) -> Result<String, FELibReturn> {
    let buffer_size = 1024;
    let mut dev_tree = vec![0u8; buffer_size];
    let res =
        unsafe { CAEN_FELib_GetDeviceTree(handle, dev_tree.as_mut_ptr() as *mut i8, buffer_size) };
    let res = FELibReturn::from(res);
    dev_tree.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(dev_tree).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getvalue(handle: u64, path: &str) -> Result<String, FELibReturn> {
    let mut value = vec![0u8; 256];
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetValue(handle, path.as_ptr(), value.as_mut_ptr() as *mut i8) };
    let res = FELibReturn::from(res);
    value.retain(|&b| b != 0);
    match res {
        FELibReturn::Success => Ok(String::from_utf8(value).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_setvalue(handle: u64, path: &str, value: &str) -> Result<(), FELibReturn> {
    let path = CString::new(path).unwrap();
    let value = CString::new(value).unwrap();
    let res = unsafe { CAEN_FELib_SetValue(handle, path.as_ptr(), value.as_ptr()) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_sendcommand(handle: u64, path: &str) -> Result<(), FELibReturn> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_SendCommand(handle, path.as_ptr()) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_setreaddataformat(handle: u64, format: &str) -> Result<(), FELibReturn> {
    let format = CString::new(format).unwrap();
    let res = unsafe { CAEN_FELib_SetReadDataFormat(handle, format.as_ptr()) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_readdata(handle: u64, data: &mut EventWrapper) -> FELibReturn {
    let res = unsafe {
        CAEN_FELib_ReadData(
            handle,
            100,
            &mut data.c_event.timestamp,
            &mut data.c_event.trigger_id,
            data.c_event.waveform,
            data.c_event.n_samples,
            &mut data.c_event.event_size,
        )
    };
    FELibReturn::from(res)
}

pub fn felib_hasdata(handle: u64) -> Result<(), FELibReturn> {
    let res = unsafe { CAEN_FELib_HasData(handle, 5) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_gethandle(handle: u64, path: &str, path_handle: &mut u64) -> Result<(), FELibReturn> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetHandle(handle, path.as_ptr(), path_handle) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_getparenthandle(
    handle: u64,
    path: &str,
    path_handle: &mut u64,
) -> Result<(), FELibReturn> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetParentHandle(handle, path.as_ptr(), path_handle) };
    let res = FELibReturn::from(res);
    match res {
        FELibReturn::Success => Ok(()),
        _ => Err(res),
    }
}
