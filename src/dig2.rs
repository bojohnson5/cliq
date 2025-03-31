use crate::felib;
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
        match felib::open(&self.url) {
            Ok(handle) => {
                self.handle = handle;
                self.is_connected = true;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn close(&self) -> Result<(), FELibReturn> {
        felib::close(self.handle)
    }

    pub fn getimpllibversion(&self) -> Result<String, FELibReturn> {
        felib::getimpllibversion(self.handle)
    }

    pub fn getdevicetree(&self) -> Result<String, FELibReturn> {
        felib::getdevicetree(self.handle)
    }

    pub fn getvalue(&self, path: &str) -> Result<String, FELibReturn> {
        felib::getvalue(self.handle, path)
    }

    pub fn setvalue(&self, path: &str, value: &str) -> Result<(), FELibReturn> {
        felib::setvalue(self.handle, path, value)
    }

    pub fn sendcommand(&self, path: &str) -> Result<(), FELibReturn> {
        felib::sendcommand(self.handle, path)
    }

    pub fn setreaddataformat(&self, format: &str) -> Result<(), FELibReturn> {
        felib::setreaddataformat(self.ep_handle, format)
    }

    pub fn readdata(&self, data: &mut EventWrapper) -> FELibReturn {
        felib::readdata(self.ep_handle, data)
    }

    pub fn hasdata(&self) -> Result<(), FELibReturn> {
        felib::hasdata(self.handle)
    }

    pub fn gethandle(&self, path: &str, path_handle: &mut u64) -> Result<(), FELibReturn> {
        felib::gethandle(self.handle, path, path_handle)
    }

    pub fn getparenthandle(
        &self,
        handle: u64,
        path: &str,
        path_handle: &mut u64,
    ) -> Result<(), FELibReturn> {
        felib::getparenthandle(handle, path, path_handle)
    }

    pub fn configure_endpoint(&mut self) -> Result<(), FELibReturn> {
        let mut ep_handle = 0;
        let mut ep_folder_handle = 0;
        felib::gethandle(self.handle, "/endpoint/scope", &mut ep_handle)?;
        felib::getparenthandle(ep_handle, "", &mut ep_folder_handle)?;
        felib::setvalue(ep_folder_handle, "/par/activeendpoint", "scope")?;
        match felib::setreaddataformat(ep_handle, &self.endpoint) {
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
