#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::CString;

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
pub enum FELibError {
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

impl From<i32> for FELibError {
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

pub fn felib_getlibinfo() -> Result<String, FELibError> {
    let buffer_size = 1024;
    let mut buffer = vec![0u8; buffer_size];
    let res = unsafe { CAEN_FELib_GetLibInfo(buffer.as_mut_ptr() as *mut i8, buffer_size) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(buffer).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getlibversion() -> Result<String, FELibError> {
    let mut libv = vec![0u8; 16];
    let res = unsafe { CAEN_FELib_GetLibVersion(libv.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(libv).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_geterrorname(error: CAEN_FELib_ErrorCode) -> Result<String, FELibError> {
    let mut err_name = vec![0u8; 32];
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_name.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(err_name).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_geterrordesc(error: CAEN_FELib_ErrorCode) -> Result<String, FELibError> {
    let mut err_desc = vec![0u8; 256];
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_desc.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(err_desc).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getlasterror() -> Result<String, FELibError> {
    let mut last_err = vec![0u8; 1024];
    let res = unsafe { CAEN_FELib_GetLibVersion(last_err.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(last_err).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_devicesdiscovery() -> Result<String, FELibError> {
    let buffer_size = 1024;
    let mut devices = vec![0u8; buffer_size];
    let res =
        unsafe { CAEN_FELib_DevicesDiscovery(devices.as_mut_ptr() as *mut i8, buffer_size, 5) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(devices).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_open(url: &str) -> Result<u64, FELibError> {
    let mut handle = 0;
    let url = CString::new(url).unwrap();
    let res = unsafe { CAEN_FELib_Open(url.as_ptr(), &mut handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(handle),
        _ => Err(res),
    }
}

pub fn felib_close(handle: u64) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_Close(handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_getimpllibversion(handle: u64) -> Result<String, FELibError> {
    let mut libv = vec![0u8; 16];
    let res = unsafe { CAEN_FELib_GetImplLibVersion(handle, libv.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(libv).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getdevicetree(handle: u64) -> Result<String, FELibError> {
    let buffer_size = 1024;
    let mut dev_tree = vec![0u8; buffer_size];
    let res =
        unsafe { CAEN_FELib_GetDeviceTree(handle, dev_tree.as_mut_ptr() as *mut i8, buffer_size) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(dev_tree).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_getvalue(handle: u64, path: &str) -> Result<String, FELibError> {
    let mut value = vec![0u8; 256];
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetValue(handle, path.as_ptr(), value.as_mut_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(String::from_utf8(value).unwrap()),
        _ => Err(res),
    }
}

pub fn felib_setvalue(handle: u64, path: &str, value: &str) -> Result<(), FELibError> {
    let path = CString::new(path).unwrap();
    let value = CString::new(value).unwrap();
    let res = unsafe { CAEN_FELib_SetValue(handle, path.as_ptr(), value.as_ptr()) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_sendcommand(handle: u64, path: &str) -> Result<(), FELibError> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_SendCommand(handle, path.as_ptr()) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_setreaddataformat(handle: u64, format: &str) -> Result<(), FELibError> {
    let format = CString::new(format).unwrap();
    let res = unsafe { CAEN_FELib_SetReadDataFormat(handle, format.as_ptr()) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_readdata(handle: u64, data: &mut Data) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_ReadData(handle, 5, data.timestamp) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_hasdata(handle: u64) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_HasData(handle, 5) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_gethandle(handle: u64, path: &str, path_handle: &mut u64) -> Result<(), FELibError> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetHandle(handle, path.as_ptr(), path_handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

pub fn felib_getparenthandle(
    handle: u64,
    path: &str,
    path_handle: &mut u64,
) -> Result<(), FELibError> {
    let path = CString::new(path).unwrap();
    let res = unsafe { CAEN_FELib_GetParentHandle(handle, path.as_ptr(), path_handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}
