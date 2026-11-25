//! NT-SMB connection and share-selection screen.
//!
//! Credentials remain in this live screen/form and are sent only to the SMB worker.
//! The logical digest records lengths only; password text is never serialized.
use std::{borrow::Cow, ffi::{CStr, CString}};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Handled, InputKind, Key,
    LogicalState, Machine, NavOp, Fx, TextEdit,
};
use plx_ui::frame::Budget;
use plx_ui::screen::{
    At, AxisMask, DrawFrame, EdgeRule, ElemKind, FocusSource, Focusable, GroupKind,
    GroupSpec, HitSource, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Scrim, Step, Dir,
    TextEntry,
};
use plx_ui::route_screen::RouteLayout;
use plx_ui::text_buffer::TextBuffer;
use plx_ui::text_field::{TextField, FIELD_H};
use plx_ui::widgets::Button;
use plx_ui::{theme, Env, Painter, Rect};
use plx_smb::{connect_form::ConnectForm, worker::{Event, SmbWorker}};

const HOST: u32 = 10;
const PORT: u32 = 11;
const USER: u32 = 12;
const PASSWORD: u32 = 13;
const GUEST: u32 = 14;
const CONNECT: u32 = 15;
const SHARE_BASE: u32 = 100;
const FORM_GROUP: plx_machine::machine::GroupId = plx_machine::machine::GroupId(0);
const GAP: f32 = theme::space::SM;
const FIELD_W: f32 = 720.0;

pub const SHAPE: &str = "SmbConnectScreen{host_len:u32,port_len:u32,user_len:u32,password_len:u32,guest:bool,editing:u32,connecting:bool,error:bool,shares:usize}";

pub struct SmbConnectScreen {
    pub entry: EntryId,
    pub form: ConnectForm,
    host: TextBuffer,
    port: TextBuffer,
    user: TextBuffer,
    password: TextBuffer,
    editing: Option<u32>,
    pub worker: SmbWorker,
    pub shares: Vec<plx_smb::ShareInfo>,
    pub error: Option<String>,
    pub connecting: bool,
}
impl SmbConnectScreen {
    pub fn new(entry: EntryId) -> Self {
        let form = ConnectForm::new();
        Self {
            entry, host: TextBuffer::new(String::new(), 0),
            port: TextBuffer::new(form.port.clone(), form.port.len()),
            user: TextBuffer::new(String::new(), 0),
            password: TextBuffer::new(String::new(), 0),
            form, worker: SmbWorker::start().expect("SMB worker must start"),
            shares: Vec::new(), error: None, connecting: false, editing: None,
        }
    }
    fn buffer(&self, field: u32) -> &TextBuffer {
        match field { HOST => &self.host, PORT => &self.port, USER => &self.user, _ => &self.password }
    }
    fn buffer_mut(&mut self, field: u32) -> &mut TextBuffer {
        match field { HOST => &mut self.host, PORT => &mut self.port, USER => &mut self.user, _ => &mut self.password }
    }
    fn field(field: u32) -> bool { matches!(field, HOST | PORT | USER | PASSWORD) }
    fn text_entry(&self) -> TextEntry {
        match self.editing { Some(PASSWORD) => TextEntry::Secret, Some(_) => TextEntry::Plain, None => TextEntry::None }
    }
    fn set_editing<H>(&mut self, field: Option<u32>, fx: &mut Effects<'_, H>) {
        let field = field.filter(|f| Self::field(*f));
        if self.editing == field { return; }
        if let Some(f) = field {
            let value = self.buffer(f).text().to_owned();
            *self.buffer_mut(f) = TextBuffer::new(value.clone(), value.len());
        }
        let was_up = self.editing.is_some();
        self.editing = field;
        if was_up != field.is_some() {
            let me = fx.from();
            fx.push(Fx::Deliver(me, Delivery::Keyboard { up: field.is_some() }));
        }
        fx.invalidate(plx_machine::present::Provenance::Input);
    }
    fn sync_form(&mut self) {
        self.form.host = self.host.text().to_owned();
        self.form.port = self.port.text().to_owned();
        self.form.username = self.user.text().to_owned();
        self.form.password = self.password.text().to_owned();
    }
    pub fn connect(&mut self) -> Result<(), String> {
        self.sync_form();
        let request = self.form.validate()?;
        self.worker.connect(request)?;
        self.connecting = true;
        self.error = None;
        self.shares.clear();
        Ok(())
    }
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
    fn rect(field: u32, measure: &dyn plx_machine::machine::Measure) -> Option<Rect> {
        let layout = RouteLayout::screen();
        let width = layout.content.w.min(FIELD_W);
        let total = 4.0 * FIELD_H + 4.0 * GAP + layout.action.h;
        let x = layout.content.x + (layout.content.w - width) * 0.5;
        let top = (layout.content.cy() - total * 0.5).max(layout.content.y);
        let field_rect = |i: f32| Rect::new(x, top + i * (FIELD_H + GAP), width, FIELD_H);
        let button_y = top + 4.0 * (FIELD_H + GAP);
        let button_w = Button::pill_w_measured(plx_os::i18n::msg::browse_action_sign_in_c(), theme::size::BODY, false, None, measure);
        match field {
            HOST => Some(field_rect(0.0)), PORT => Some(field_rect(1.0)),
            USER => Some(field_rect(2.0)), PASSWORD => Some(field_rect(3.0)),
            CONNECT => Some(Rect::new(x, button_y, button_w, layout.action.h)),
            GUEST => Some(Rect::new(x + button_w + theme::space::MD, button_y, button_w, layout.action.h)),
            _ => None,
        }
    }
    fn stop_rect(field: u32, measure: &dyn plx_machine::machine::Measure) -> Option<Rect> {
        let r = Self::rect(field, measure)?;
        Some(if Self::field(field) { plx_ui::text_field::plate(r) } else { r })
    }
    fn ids(&self) -> Vec<u32> {
        if !self.shares.is_empty() {
            (0..self.shares.len()).map(|i| SHARE_BASE + i as u32).collect()
        } else { vec![HOST, PORT, USER, PASSWORD, CONNECT, GUEST] }
    }
}
impl LogicalState for SmbConnectScreen {
 fn write(&self,c:&mut Canon){
  c.u32(self.host.text().len() as u32).u32(self.port.text().len() as u32)
   .u32(self.user.text().len() as u32).u32(self.password.text().len() as u32)
   .bool(self.form.guest).u32(self.editing.unwrap_or(0)).bool(self.connecting)
   .bool(self.error.is_some()).seq(self.shares.len());
 }
 fn probe(&self,out:&mut String){out.push_str("smb_connect");}
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Machine<H> for SmbConnectScreen {
 type Ev=ScreenEvent<H>;
 fn step(&mut self,ev:&Self::Ev,_cx:&Cx<'_,H>,fx:&mut Effects<'_,H>)->Handled {
  match ev {
   ScreenEvent::Tick(_)=>{if let Err(error)=self.poll(){self.error=Some(error);self.connecting=false;} Handled::Yes}
   ScreenEvent::Activate(elem) => {
    match *elem {
     HOST|PORT|USER|PASSWORD => self.set_editing(if self.editing==Some(*elem){None}else{Some(*elem)},fx),
     GUEST => { self.form.guest=!self.form.guest; if self.form.guest { self.set_editing(None,fx); } fx.invalidate(plx_machine::present::Provenance::Input); }
     CONNECT => { self.set_editing(None,fx); if let Err(e)=self.connect(){self.error=Some(e);} }
     e if e>=SHARE_BASE => { let i=(e-SHARE_BASE) as usize; if let Some(share)=self.shares.get(i){ if let Err(err)=self.worker.select_share(share.name.clone()){self.error=Some(err);} else {self.connecting=true; self.error=None;} } }
     _=>{}
    }
    Handled::Yes
   }
   ScreenEvent::Input(input)=>match &input.kind {
    InputKind::SystemKeyboard(up)=>{if !*up {self.editing=None;} Handled::Yes}
    InputKind::Text(edit)=>{if let Some(field)=self.editing {self.buffer_mut(field).edit(edit);self.sync_form();fx.invalidate(plx_machine::present::Provenance::Input);} Handled::Yes}
    InputKind::Key{key,sym,edge,..} if *edge!=Edge::Up=>{
      if let Some(field)=self.editing {
       let edit=match (*key,*sym) {(Key::Left,_)=>Some(TextEdit::Left),(Key::Right,_)=>Some(TextEdit::Right),(_,plx_ui::consts::SDLK_BACKSPACE)=>Some(TextEdit::Backspace),(_,plx_ui::consts::SDLK_CLEAR)=>Some(TextEdit::Clear),_=>None};
       if let Some(edit)=edit {self.buffer_mut(field).edit(&edit);self.sync_form();fx.invalidate(plx_machine::present::Provenance::Input);return Handled::Yes;}
       if matches!(key,Key::Back|Key::Ok){self.set_editing(None,fx);return Handled::Yes;}
       if matches!(key,Key::Up|Key::Down){self.set_editing(None,fx);return Handled::No;}
       return Handled::Yes;
      }
      if *key==Key::Back {fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));Handled::Yes} else {Handled::No}
    }
    _=>Handled::No
   },
   ScreenEvent::FocusMoved{to,..}=>{if self.editing.is_some_and(|e|e!=to.elem){self.set_editing(None,fx);} Handled::Yes}
   ScreenEvent::WillLeave(_)|ScreenEvent::Cover|ScreenEvent::Suspend|ScreenEvent::Unmount=>{self.set_editing(None,fx);Handled::No}
   _=>Handled::No
  }
 }
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Focusable<H> for SmbConnectScreen {
 fn groups(&self,cx:&Cx<'_,H>,out:&mut Vec<GroupSpec>){
  let ids=self.ids(); if ids.is_empty(){return;}
  let extent=ids.iter().filter_map(|e| if *e>=SHARE_BASE {None}else{Self::stop_rect(*e,cx.measure)}).reduce(|a,b|a.union(b)).unwrap_or(Rect::FULL);
  out.push(GroupSpec{id:FORM_GROUP,kind:GroupKind::Free,seat:Seat::First,reachable:AxisMask::BOTH,edge:[EdgeRule::Stop;4],extent,len:ids.len(),elem:ElemKind::Bare});
 }
 fn group_of(&self,elem:&u32,_cx:&Cx<'_,H>)->Option<plx_machine::machine::GroupId>{self.ids().contains(elem).then_some(FORM_GROUP)}
 fn neighbour(&self,key:FocusKey<u32>,dir:Dir,_cx:&Cx<'_,H>)->Step<u32>{
  let ids=self.ids();let Some(i)=ids.iter().position(|e|*e==key.elem) else{return Step::Edge};
  let next=match dir{Dir::Up|Dir::Left=>i.checked_sub(1),Dir::Down|Dir::Right=>Some(i+1).filter(|n|*n<ids.len())};
  next.map_or(Step::Edge,|n|Step::Move(FocusKey{entry:self.entry,elem:ids[n]}))
 }
 fn place(&self,elem:&u32,cx:&Cx<'_,H>,_at:At)->Option<Placed>{
  let r=if *elem>=SHARE_BASE {let i=(*elem-SHARE_BASE) as usize; let layout=RouteLayout::screen();Some(Rect::new(layout.content.x,layout.content.y+i as f32*(FIELD_H+GAP),layout.content.w.min(FIELD_W),FIELD_H))}else{Self::stop_rect(*elem,cx.measure)}?;
  Some(Placed{rect:r,rest_rect:r,clip:Rect::FULL,index:Some(self.ids().iter().position(|e|e==elem).unwrap_or(0) as u32)})
 }
 fn reconcile(&self,want:FocusKey<u32>,_cx:&Cx<'_,H>)->FocusKey<u32>{if self.ids().contains(&want.elem){want}else{FocusKey{entry:self.entry,elem:self.ids().first().copied().unwrap_or(HOST)}}}
 fn seat(&self,_g:plx_machine::machine::GroupId,_from:Placed,_cx:&Cx<'_,H>)->FocusKey<u32>{FocusKey{entry:self.entry,elem:self.ids().first().copied().unwrap_or(HOST)}}
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Screen<H> for SmbConnectScreen {
 fn name(&self)->&'static str{"smb_connect"}
 fn state(&self)->&dyn LogicalState{self}
 fn crumb(&self,_cx:&Cx<'_,H>)->Option<Cow<'_,str>>{None}
 fn prepare(&mut self,_b:&mut Budget,_cx:&Cx<'_,H>){}
 fn scrim(&self)->Scrim{Scrim::NONE}
 fn draw(&mut self,f:&mut DrawFrame<'_, '_,H>){
  let p=f.painter; let layout=RouteLayout::screen(); let measure=f.measure;
  let title=CString::new("Connect to SMB").expect("static title");
  let body=if self.connecting{"Connecting to the SMB server and loading shares…".to_owned()}
   else if let Some(error)=self.error.as_deref(){format!("Connection failed: {error}")}
   else if !self.shares.is_empty(){format!("Choose a share on {}.",self.form.host)}
   else{"Enter your server details to browse and play videos.".to_owned()};
  let body=CString::new(body.replace('\0',"")).expect("SMB status contains no nul");
  layout.draw_narrative(p,None,title.as_c_str(),body.as_c_str(),measure);
  let focus=f.focus.current.map(|k|k.elem);
  if !self.shares.is_empty(){
   for (i,share) in self.shares.iter().enumerate(){
    let id=SHARE_BASE+i as u32; let Some(r)=Self::rect(HOST,measure).map(|r|Rect::new(r.x,r.y+i as f32*(FIELD_H+GAP),r.w,FIELD_H)) else{continue};
    TextField::new(plx_os::i18n::msg::browse_jf_server_c(),&share.name).hot(if focus==Some(id){1.0}else{0.0}).draw(p,r,measure);
   }
   return;
  }
  let fields=[(HOST,"Server / IP",&self.host,false),(PORT,"Port",&self.port,false),(USER,"Username",&self.user,false),(PASSWORD,"Password",&self.password,true)];
  for (id,label,buffer,secret) in fields{
   if let Some(r)=Self::rect(id,measure){
    let label=CString::new(label).expect("static field label");
    TextField::new(label.as_c_str(),buffer.text()).caret(buffer.caret()).secret(secret).hot(if focus==Some(id){1.0}else{0.0}).editing(self.editing==Some(id),true).draw(p,r,measure);
   }
  }
  for (id,label) in [(CONNECT,"Connect"),(GUEST,if self.form.guest{"Guest: On"}else{"Guest: Off"})]{
   if let Some(r)=Self::rect(id,measure){
    let label=CString::new(label).expect("static button label");
    Button::new(label.as_ptr(),theme::size::BODY,r).focused(focus==Some(id)).draw(&Env::inert(),p);
   }
  }
 }
 fn render(&self)->RenderStrategy{RenderStrategy::Page}
 fn focus_source(&self)->FocusSource{FocusSource::Engine}
 fn hit_source(&self)->HitSource{HitSource::Engine}
 fn text_entry(&self)->TextEntry{self.text_entry()}
 fn as_any(&self)->Option<&dyn std::any::Any>{Some(self)}
 fn as_any_mut(&mut self)->Option<&mut dyn std::any::Any>{Some(self)}
}
