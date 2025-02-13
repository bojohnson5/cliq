#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use std::ffi::CString;

use crate::felib::{gethandle, getparenthandle, setreaddataformat, setvalue};
use crate::EventWrapper;
use crate::FELibReturn;
use confique::Config;

#[derive(Config, Clone)]
pub struct Dig2 {
    pub url: String,
    pub endpoint: String,
    #[config(default = 0)]
    pub handle: u64,
    #[config(default = 0)]
    pub ep_handle: u64,
    #[config(default = 0)]
    pub ep_folder_handle: u64,
    #[config(default = false)]
    pub is_connected: bool,
    #[config(default = false)]
    pub is_ep_configured: bool,
}

impl Dig2 {
    pub fn open(&mut self) -> Result<(), FELibReturn> {
        let mut handle = 0;
        let url = CString::new(&*self.url).unwrap();
        let res = unsafe { CAEN_FELib_Open(url.as_ptr(), &mut handle) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => {
                self.handle = handle;
                self.is_connected = true;
                Ok(())
            }
            _ => Err(res),
        }
    }

    pub fn close(&self) -> Result<(), FELibReturn> {
        let res = unsafe { CAEN_FELib_Close(self.handle) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn getimpllibversion(&self) -> Result<String, FELibReturn> {
        let mut libv = vec![0u8; 16];
        let res =
            unsafe { CAEN_FELib_GetImplLibVersion(self.handle, libv.as_mut_ptr() as *mut i8) };
        let res = FELibReturn::from(res);
        libv.retain(|&b| b != 0);
        match res {
            FELibReturn::Success => Ok(String::from_utf8(libv).unwrap()),
            _ => Err(res),
        }
    }

    pub fn getdevicetree(&self) -> Result<String, FELibReturn> {
        let buffer_size = 1024;
        let mut dev_tree = vec![0u8; buffer_size];
        let res = unsafe {
            CAEN_FELib_GetDeviceTree(self.handle, dev_tree.as_mut_ptr() as *mut i8, buffer_size)
        };
        let res = FELibReturn::from(res);
        dev_tree.retain(|&b| b != 0);
        match res {
            FELibReturn::Success => Ok(String::from_utf8(dev_tree).unwrap()),
            _ => Err(res),
        }
    }

    pub fn getvalue(&self, path: &str) -> Result<String, FELibReturn> {
        let mut value = vec![0u8; 256];
        let path = CString::new(path).unwrap();
        let res = unsafe {
            CAEN_FELib_GetValue(self.handle, path.as_ptr(), value.as_mut_ptr() as *mut i8)
        };
        let res = FELibReturn::from(res);
        value.retain(|&b| b != 0);
        match res {
            FELibReturn::Success => Ok(String::from_utf8(value).unwrap()),
            _ => Err(res),
        }
    }

    pub fn setvalue(&self, path: &str, value: &str) -> Result<(), FELibReturn> {
        let path = CString::new(path).unwrap();
        let value = CString::new(value).unwrap();
        let res = unsafe { CAEN_FELib_SetValue(self.handle, path.as_ptr(), value.as_ptr()) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn sendcommand(&self, path: &str) -> Result<(), FELibReturn> {
        let path = CString::new(path).unwrap();
        let res = unsafe { CAEN_FELib_SendCommand(self.handle, path.as_ptr()) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn setreaddataformat(&self, format: &str) -> Result<(), FELibReturn> {
        let format = CString::new(format).unwrap();
        let res = unsafe { CAEN_FELib_SetReadDataFormat(self.ep_handle, format.as_ptr()) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn readdata(&self, data: &mut EventWrapper) -> FELibReturn {
        let res = unsafe {
            CAEN_FELib_ReadData(
                self.handle,
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

    pub fn hasdata(&self) -> Result<(), FELibReturn> {
        let res = unsafe { CAEN_FELib_HasData(self.handle, 5) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn gethandle(&self, path: &str, path_handle: &mut u64) -> Result<(), FELibReturn> {
        let path = CString::new(path).unwrap();
        let res = unsafe { CAEN_FELib_GetHandle(self.handle, path.as_ptr(), path_handle) };
        let res = FELibReturn::from(res);
        match res {
            FELibReturn::Success => Ok(()),
            _ => Err(res),
        }
    }

    pub fn getparenthandle(
        &self,
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

    pub fn configure_endpoint(&mut self) -> Result<(), FELibReturn> {
        let mut ep_handle = 0;
        let mut ep_folder_handle = 0;
        gethandle(self.handle, "/endpoint/scope", &mut ep_handle)?;
        getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
        setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
        match setreaddataformat(ep_handle, &self.endpoint) {
            Ok(_) => {
                self.ep_handle = ep_handle;
                self.ep_folder_handle = ep_folder_handle;
                self.is_ep_configured = true;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
