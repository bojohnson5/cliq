#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

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
