#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::CString;

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
enum FELibError {
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

#[repr(i32)]
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
enum FELibNode {
    Unknown = -1,
    Parameter = 0,
    Command = 1,
    Feature = 2,
    Attribute = 3,
    Endpoint = 4,
    Channel = 5,
    Digitizer = 6,
    Folder = 7,
    LVDS = 8,
    VGA = 9,
    HVChannel = 10,
    MonOut = 11,
    VTrace = 12,
    Group = 13,
    HVRange = 14,
    Other,
}

impl From<i32> for FELibNode {
    fn from(value: i32) -> Self {
        match value {
            -1 => Self::Unknown,
            0 => Self::Parameter,
            1 => Self::Command,
            2 => Self::Feature,
            3 => Self::Attribute,
            4 => Self::Endpoint,
            5 => Self::Channel,
            6 => Self::Digitizer,
            7 => Self::Folder,
            8 => Self::LVDS,
            9 => Self::VGA,
            10 => Self::HVChannel,
            11 => Self::MonOut,
            12 => Self::VTrace,
            13 => Self::Group,
            14 => Self::HVRange,
            _ => Self::Other,
        }
    }
}

fn connect_to_digitizer(dev_handle: &mut u64, path: &str) -> Result<(), FELibError> {
    let path = CString::new(path).expect("bad dev path");
    let res = unsafe { CAEN_FELib_Open(path.as_ptr(), dev_handle as *mut u64) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

fn get_n_channels(dev_handle: u64) -> Result<usize, FELibError> {
    let value = CString::new("/par/NumCh").expect("bad value");
    let chans = 0;
    let res = unsafe { CAEN_FELib_GetValue(dev_handle, value.as_ptr(), chans as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(chans),
        _ => Err(res),
    }
}

fn felib_getlibinfo() -> Result<String, FELibError> {
    let json_string = String::new();
    let size = 0;
    let res = unsafe { CAEN_FELib_GetLibInfo(json_string.as_ptr() as *mut i8, size) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(json_string),
        _ => Err(res),
    }
}

fn felib_getlibversion() -> Result<String, FELibError> {
    let libv = String::new();
    let res = unsafe { CAEN_FELib_GetLibVersion(libv.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(libv),
        _ => Err(res),
    }
}

fn felib_geterrorname(error: CAEN_FELib_ErrorCode) -> Result<String, FELibError> {
    let err_name = String::new();
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_name.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(err_name),
        _ => Err(res),
    }
}

fn felib_geterrordesc(error: CAEN_FELib_ErrorCode) -> Result<String, FELibError> {
    let err_desc = String::new();
    let res = unsafe { CAEN_FELib_GetErrorName(error, err_desc.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(err_desc),
        _ => Err(res),
    }
}

fn felib_getlasterror() -> Result<String, FELibError> {
    let last_err = String::new();
    let res = unsafe { CAEN_FELib_GetLibVersion(last_err.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(last_err),
        _ => Err(res),
    }
}

fn felib_devicesdiscovery() -> Result<String, FELibError> {
    let devices = String::new();
    let size = 0;
    let res = unsafe { CAEN_FELib_GetLibInfo(devices.as_ptr() as *mut i8, size) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(devices),
        _ => Err(res),
    }
}

fn felib_open(url: String) -> Result<u64, FELibError> {
    let mut handle = 0;
    let url = CString::new(url).unwrap();
    let res = unsafe { CAEN_FELib_Open(url.as_ptr(), &mut handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(handle),
        _ => Err(res),
    }
}

fn felib_close(handle: u64) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_Close(handle) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

fn felib_getimpllibversion(handle: u64) -> Result<String, FELibError> {
    let libv = String::with_capacity(16);
    let res = unsafe { CAEN_FELib_GetImplLibVersion(handle, libv.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(libv),
        _ => Err(res),
    }
}

fn felib_getdevicetree(handle: u64, size: usize) -> Result<String, FELibError> {
    let dev_tree = String::with_capacity(size);
    let res = unsafe { CAEN_FELib_GetDeviceTree(handle, dev_tree.as_ptr() as *mut i8, size) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(dev_tree),
        _ => Err(res),
    }
}

fn felib_getvalue(handle: u64, path: String) -> Result<String, FELibError> {
    let value = String::with_capacity(256);
    let res = unsafe {
        CAEN_FELib_GetValue(
            handle,
            path.as_ptr() as *const i8,
            value.as_ptr() as *mut i8,
        )
    };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(value),
        _ => Err(res),
    }
}

fn felib_setvalue(handle: u64, path: String, value: String) -> Result<String, FELibError> {
    let res = unsafe {
        CAEN_FELib_SetValue(
            handle,
            path.as_ptr() as *const i8,
            value.as_ptr() as *mut i8,
        )
    };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(value),
        _ => Err(res),
    }
}

fn felib_sendcommand(handle: u64, path: String) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_SendCommand(handle, path.as_ptr() as *mut i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

fn felib_setreaddataformat(handle: u64, format: String) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_SetReadDataFormat(handle, format.as_ptr() as *const i8) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

fn felib_hasdata(handle: u64, timeout: i32) -> Result<(), FELibError> {
    let res = unsafe { CAEN_FELib_HasData(handle, timeout) };
    let res = FELibError::from(res);
    match res {
        FELibError::Success => Ok(()),
        _ => Err(res),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os;

    #[test]
    fn test_code() {
        let mut handle: u64 = 10;
        let path = "Path";
        let path = CString::new(path).expect("uh oh");
        let res = unsafe {
            CAEN_FELib_Open(
                path.as_ptr() as *const os::raw::c_char,
                &mut handle as *mut u64,
            )
        };
        if res != CAEN_FELib_ErrorCode_CAEN_FELib_Success {
            println!("error!");
        }
        println!("{res}");
    }
}
