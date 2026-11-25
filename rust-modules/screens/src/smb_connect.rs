//! NT-SMB's first connection surface. Network work is delegated to the existing SMB worker.
use std::borrow::Cow;
use plx_machine::machine::{Canon,Cx,Edge,Effects,EntryId,FocusKey,Handled,InputKind,Key,LogicalState,Machine,NavOp,Fx};
use plx_ui::frame::Budget;
use plx_ui::screen::{At,Focusable,GroupSpec,Dir,Step,Placed,Screen,ScreenEvent,DrawFrame,Scrim,RenderStrategy,FocusSource,HitSource};
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

impl LogicalState for SmbConnectScreen {
 fn write(&self,c:&mut Canon){c.str(&self.form.host).str(&self.form.username).u32(self.form.password.len() as u32).bool(self.form.guest).bool(self.connecting).bool(self.error.is_some()).seq(self.shares.len());}
 fn probe(&self,out:&mut String){out.push_str("smb_connect");}
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Machine<H> for SmbConnectScreen {
 type Ev=ScreenEvent<H>;
 fn step(&mut self,ev:&Self::Ev,_cx:&Cx<'_,H>,fx:&mut Effects<'_,H>)->Handled {
  match ev {
   ScreenEvent::Tick(_)=>{if let Err(error)=self.poll(){self.error=Some(error);self.connecting=false;} Handled::Yes}
   ScreenEvent::Input(input)=>match input.kind {InputKind::Key{key:Key::Back,edge:Edge::Down,..}=>{fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));Handled::Yes},_=>Handled::No},
   _=>Handled::No
  }
 }
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Focusable<H> for SmbConnectScreen {
 fn groups(&self,_cx:&Cx<'_,H>,_out:&mut Vec<GroupSpec>){}
 fn group_of(&self,_elem:&u32,_cx:&Cx<'_,H>)->Option<plx_machine::machine::GroupId>{None}
 fn neighbour(&self,_key:FocusKey<u32>,_dir:Dir,_cx:&Cx<'_,H>)->Step<u32>{Step::Edge}
 fn place(&self,_elem:&u32,_cx:&Cx<'_,H>,_at:At)->Option<Placed>{None}
 fn reconcile(&self,want:FocusKey<u32>,_cx:&Cx<'_,H>)->FocusKey<u32>{want}
 fn seat(&self,_g:plx_machine::machine::GroupId,_from:Placed,_cx:&Cx<'_,H>)->FocusKey<u32>{FocusKey{entry:self.entry,elem:0}}
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Screen<H> for SmbConnectScreen {
 fn name(&self)->&'static str{"smb_connect"}
 fn state(&self)->&dyn LogicalState{self}
 fn crumb(&self,_cx:&Cx<'_,H>)->Option<Cow<'_,str>>{None}
 fn prepare(&mut self,_b:&mut Budget,_cx:&Cx<'_,H>){}
 fn scrim(&self)->Scrim{Scrim::NONE}
 fn draw(&mut self,_f:&mut DrawFrame<'_, '_,H>){}
 fn render(&self)->RenderStrategy{RenderStrategy::Page}
 fn focus_source(&self)->FocusSource{FocusSource::Engine}
 fn hit_source(&self)->HitSource{HitSource::Engine}
 fn as_any(&self)->Option<&dyn std::any::Any>{Some(self)}
 fn as_any_mut(&mut self)->Option<&mut dyn std::any::Any>{Some(self)}
}
