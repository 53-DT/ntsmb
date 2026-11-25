//! NT-SMB's first connection surface. Network work is delegated to the existing SMB worker.
use plx_machine::machine::EntryId;
use plx_smb::{connect_form::ConnectForm, worker::{Event, SmbWorker}};
pub const SHAPE: &str = "SmbConnectScreen{host:str,user:str,password_len:u32,guest:bool,connecting:bool,error:bool,shares:usize}";
/// Initial implementation of the SMB connection surface; drawing/focus integration is WIP.
pub struct SmbConnectScreen {
    pub entry: EntryId,
    pub form: ConnectForm,
    pub worker: SmbWorker,
    pub shares: Vec<plx_smb::ShareInfo>,
    pub error: Option<String>,
    pub connecting: bool,
}
impl SmbConnectScreen {
    pub fn new(entry: EntryId) -> Self {
        Self { entry, form: ConnectForm::new(), worker: SmbWorker::start().expect("SMB worker must start"),
            shares: Vec::new(), error: None, connecting: false }
    }
    /// Submit credentials without blocking the UI thread.
    pub fn connect(&mut self) -> Result<(), String> {
        let request = self.form.validate()?;
        self.worker.connect(request)?;
        self.connecting = true;
        self.error = None;
        Ok(())
    }
    /// Consume worker results on the UI thread; this list is the temporary share picker.
    pub fn poll(&mut self) -> Result<(), String> {
        while let Some(event) = self.worker.try_event()? {
            match event {
                Event::Shares(Ok(shares)) => { self.shares = shares; self.connecting = false; }
                Event::Shares(Err(error)) => { self.error = Some(error); self.connecting = false; }
                Event::Directory(_) | Event::Disconnected => {}
            }
        }
        Ok(())
    }
}
