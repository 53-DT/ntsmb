//! NT-SMB connection and share-selection screen.
//!
//! Credentials remain in this live screen/form and are sent only to the SMB worker.
//! The logical digest records lengths only; password text is never serialized.
use std::{borrow::Cow, ffi::CString};
use plx_machine::machine::{
    Canon, Cx, Delivery, Edge, Effects, EntryId, FocusKey, Handled, InputKind, Key,
    LogicalState, Machine, NavOp, Fx, TextEdit, Host,
};
use plx_ui::frame::Budget;
use plx_ui::cards::{CardEvent, CardSource, Grid, GridSpec, RowStyle, Tile, TileLabel};
use plx_ui::widgets::Art;
use plx_machine::machine::Measure;
use plx_ui::label::{Label, HAlign};
use plx_ui::consts::{CARD_H, CARD_W, MARGIN_X, SCR_W};
use plx_ui::screen::{
    At, AxisMask, DrawFrame, EdgeRule, ElemKind, Enter, FocusSource, Focusable, GroupKind,
    GroupSpec, HitSource, Placed, RenderStrategy, Screen, ScreenEvent, Seat, Scrim, Step, Dir,
    TextEntry,
};
use plx_ui::route_screen::RouteLayout;
use plx_ui::text_buffer::TextBuffer;
use plx_ui::text_field::{TextField, FIELD_H};
use plx_ui::widgets::{Button, CtlPop};
use plx_ui::{theme, Env, Rect, View};
use plx_smb::{connect_form::ConnectForm, controller::{BrowseController, Phase, Activation, Back}};

const HOST: u32 = 10;
const PORT: u32 = 11;
const USER: u32 = 12;
const PASSWORD: u32 = 13;
const GUEST: u32 = 14;
const CONNECT: u32 = 15;
const SAVE: u32 = 16;
const SHARE_BASE: u32 = 100;
const SAVED_BASE: u32 = 200;
const NEW_CONNECTION: u32 = 17;
const CHANGE_SERVER: u32 = 18;
const PROFILE: u32 = 19;
const MENU_SAVED: u32 = 20;
const MENU_ADD: u32 = 21;
const MENU_DISCONNECT: u32 = 22;
const MENU_LOCK: u32 = 23;
const MENU_CLOSE: u32 = 24;
const REMOVE_BASE: u32 = 300;
const FORM_GROUP: plx_machine::machine::GroupId = plx_machine::machine::GroupId(0);
const PROFILE_GROUP: plx_machine::machine::GroupId = plx_machine::machine::GroupId(1);
const GAP: f32 = theme::space::SM;
const SHARE_TOP_PAD: f32 = 112.0;
const FIELD_W: f32 = 720.0;

pub const SHAPE: &str = "SmbConnectScreen{host_len:u32,port_len:u32,user_len:u32,password_len:u32,guest:bool,save:bool,editing:u32,connecting:bool,error:bool,shares:usize}";

pub struct SmbConnectScreen {
    pub entry: EntryId,
    pub form: ConnectForm,
    host: TextBuffer,
    port: TextBuffer,
    user: TextBuffer,
    password: TextBuffer,
    editing: Option<u32>,
    pub controller: BrowseController,
    pub shares: Vec<plx_smb::ShareInfo>,
    pub rows: Vec<plx_smb::browser::BrowserEntry>,
    pub selected: usize,
    pub error: Option<String>,
    pub connecting: bool,
    playback: Option<std::sync::mpsc::Receiver<Result<plx_smb::playback::PreparedPlayback, String>>>,
    saved: plx_smb::store::SavedStore,
    save_pending: bool,
    saved_id: Option<String>,
    last_saved_path: String,
    saved_view: bool,
    menu_open: bool,
    resume_path: Option<String>,
    browser_grid: Grid,
    // Reuse FinNative's spring-based control lift for the Profile toggle and its menu items.
    profile_pop: CtlPop<1>,
    menu_pop: CtlPop<5>,
}

struct SmbCardSource<'a> { rows: &'a [plx_smb::browser::BrowserEntry] }
impl<H: Host<Elem = u32>> CardSource<H> for SmbCardSource<'_> {
 fn len(&self)->usize { self.rows.len() }
 fn elem(&self,i:usize)->u32 { SHARE_BASE+i as u32 }
 fn index_of(&self,e:&u32)->Option<usize> { e.checked_sub(SHARE_BASE).map(|i|i as usize).filter(|i|*i<self.rows.len()) }
 fn art(&self,_i:usize)->Art<'_> { Art::Poster(None) }
 fn label(&self,i:usize)->TileLabel {
  let Some(row)=self.rows.get(i) else{return TileLabel::default()};
  TileLabel { title:Some(CString::new(row.entry.name.as_str()).unwrap_or_default()), caption:Some(CString::new(if row.is_video{"Video"}else{"Folder"}).unwrap_or_default()), ..TileLabel::default() }
 }
 fn a11y_title(&self,i:usize)->Option<Cow<'_,str>> { self.rows.get(i).map(|r|Cow::Borrowed(r.entry.name.as_str())) }
 fn overlay(&self,p:plx_ui::Painter,i:usize,tile:&Tile,_measure:&dyn Measure) {
  let Some(row)=self.rows.get(i) else{return};
  let glyph=CString::new(if row.is_video{"▶"}else{"📁"}).unwrap();
  let frame=Rect::new(tile.rect.x,tile.rect.y+tile.rect.h*0.22,tile.rect.w,tile.rect.h*0.48);
  Label::new(glyph.as_ptr(),96,if row.is_video{[0.88,0.91,0.96,1.0]}else{[0.98,0.72,0.25,1.0]}).h(HAlign::Center).draw(p,frame);
 }
}
impl SmbConnectScreen {
    pub fn new(entry: EntryId) -> Self {
        let form = ConnectForm::new();
        // Use the HUD store's device-tested order: persistent locations first and
        // the app's /tmp runtime directory last. That final fallback survives closing and
        // reopening the app; SavedStore marks it as temporary so the UI warns about TV reboots.
        // Use the platform's dedicated persistent state root first, then retain the HUD-derived
        // fallbacks for installs whose state-root directory is not writable. Runtime storage is last.
        let mut saved_paths = vec![plx_base::paths::persistent_state_root().join("ntsmb-connections.json")];
        saved_paths.extend(plx_base::paths::hud_candidates().into_iter().filter(|path| path.parent() != Some(plx_base::paths::runtime_dir())).map(|path| {
            let file_name = path.file_name().and_then(|name| name.to_str()).unwrap_or("huds");
            let prefix = file_name.strip_suffix("huds").unwrap_or("");
            path.with_file_name(format!("{prefix}ntsmb-connections.json"))
        }));
        saved_paths.push(plx_base::paths::runtime_dir().join("ntsmb-connections.json"));
        saved_paths.dedup();
        let saved = plx_smb::store::SavedStore::open_with_runtime(saved_paths, Some(plx_base::paths::runtime_dir().to_path_buf()));
        let saved_view = !saved.connections().is_empty();
        Self {
            entry, host: TextBuffer::new(String::new(), 0),
            port: TextBuffer::new(form.port.clone(), form.port.len()),
            user: TextBuffer::new(String::new(), 0),
            password: TextBuffer::new(String::new(), 0),
            form, controller: BrowseController::new(),
            shares: Vec::new(), rows: Vec::new(), selected: 0, error: saved.saved_temporarily().then(|| "Saved in temporary TV storage; it may be lost after a TV restart".into()), connecting: false, editing: None, playback: None,
            saved, save_pending: false, saved_id: None, last_saved_path: String::new(),
            saved_view, menu_open: false, resume_path: None,
            browser_grid: Grid::new(entry, GridSpec::resting(324.0).columns(6, RowStyle { gap: (SCR_W - 2.0 * MARGIN_X - 44.0 - 6.0 * CARD_W) / 5.0, ..RowStyle::HOME }, MARGIN_X)),
            profile_pop: CtlPop::new(), menu_pop: CtlPop::new(),
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
    fn set_editing<H: Host>(&mut self, field: Option<u32>, fx: &mut Effects<'_, H>) {
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
        self.controller.connect(request)?;
        if let Some(path) = self.resume_path.take() { self.controller.resume_in(&path); }
        self.connecting = true;
        self.error = None;
        self.save_pending = self.form.save_connection;
        self.last_saved_path.clear();
        self.shares.clear(); self.rows.clear(); self.selected = 0;
        Ok(())
    }
    pub fn poll(&mut self) -> Result<(), String> {
        self.controller.pump();
        self.shares = self.controller.shares().to_vec();
        self.rows = self.controller.browser().entries().to_vec();
        self.selected = self.controller.browser().selected();
        self.connecting = matches!(self.controller.phase(), Phase::Connecting | Phase::OpeningShare) || self.controller.browser().is_loading();
        if let Some(message) = self.controller.message() { self.error = Some(message.to_owned()); }
        if let Some(error) = self.controller.browser().error() { self.error = Some(error.to_owned()); }
        if self.save_pending && matches!(self.controller.phase(), Phase::PickShare | Phase::Browsing) {
            self.save_pending = false;
            let result = self.form.validate().and_then(|request| {
                let mut profile = request.profile;
                let password = if profile.guest { None } else { Some(request.password().to_owned()) };
                let fallback = if let Some(secret) = password.as_deref() {
                    if profile.protect_password(secret).is_ok() { None } else { Some(secret.to_owned()) }
                } else { None };
                self.saved.upsert_with_password(profile, fallback)
            });
            match result {
                Ok(id) => {
                    self.saved_id = Some(id); self.saved_view = false;
                    if let Err(error) = self.saved.save() { self.error = Some(error); }
                    else if self.saved.saved_temporarily() { self.error = Some("Saved in temporary TV storage; it may be lost after a TV restart".into()); }
                    else if !self.form.guest && self.saved.get(self.saved_id.as_deref().unwrap_or("")).is_some_and(|s| s.password.is_some()) {
                        self.error = Some("Saved password in app-private storage because the TV secure key store is unavailable. This is less secure.".into());
                    }
                }
                Err(error) => self.error = Some(format!("Connected, but could not save this server: {error}")),
            }
        }
        if let Some(id) = self.saved_id.clone() {
            let path = self.controller.folder().to_owned();
            if path != self.last_saved_path {
                self.saved.set_last_path(&id, &path);
                self.last_saved_path = path;
                if let Err(error) = self.saved.save() { self.error = Some(error); }
            }
            if let Some(share) = self.controller.share() {
                let changed = self.saved.get(&id).is_some_and(|entry| entry.profile.share.as_deref() != Some(share));
                if changed { self.saved.set_share(&id, Some(share)); if let Err(error) = self.saved.save() { self.error = Some(error); } }
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
            SAVE => Some(Rect::new(x + 2.0 * (button_w + theme::space::MD), button_y, button_w, layout.action.h)),
            _ => None,
        }
    }
    fn stop_rect(&self, field: u32, measure: &dyn plx_machine::machine::Measure) -> Option<Rect> {
        if field >= SAVED_BASE && field < REMOVE_BASE {
            let i=(field-SAVED_BASE) as usize; let layout=RouteLayout::screen();
            return Some(Rect::new(layout.content.x,layout.content.y+i as f32*(FIELD_H+GAP),520.0,FIELD_H));
        }
        if field >= REMOVE_BASE && field < REMOVE_BASE + plx_smb::store::MAX_SAVED as u32 {
            let i=(field-REMOVE_BASE) as usize; let layout=RouteLayout::screen();
            return Some(Rect::new(layout.content.x+540.0,layout.content.y+i as f32*(FIELD_H+GAP),150.0,FIELD_H));
        }
        if field == NEW_CONNECTION {
            let layout=RouteLayout::screen();
            return Some(Rect::new(layout.content.x,layout.content.y+self.saved.connections().len() as f32*(FIELD_H+GAP),layout.content.w.min(FIELD_W),FIELD_H));
        }
        if field == CHANGE_SERVER {
            let base=Self::rect(HOST,measure)?;
            return Some(Rect::new(base.x,base.y+self.shares.len() as f32*(FIELD_H+GAP),base.w,FIELD_H));
        }
        if field >= SHARE_BASE && field < SHARE_BASE+self.shares.len() as u32 && self.controller.phase()!=Phase::Browsing {
            let i=(field-SHARE_BASE) as usize;
            let base=Self::rect(HOST,measure)?;
            return Some(Rect::new(base.x,base.y+SHARE_TOP_PAD+i as f32*(FIELD_H+GAP),base.w,FIELD_H));
        }
        if field == PROFILE { return Some(Self::profile_rect()); }
        if field == MENU_SAVED { return Some(Self::menu_rect(0)); }
        if field == MENU_ADD { return Some(Self::menu_rect(1)); }
        if field == MENU_DISCONNECT { return Some(Self::menu_rect(2)); }
        if field == MENU_LOCK { return Some(Self::menu_rect(3)); }
        if field == MENU_CLOSE { return Some(Self::menu_rect(4)); }
        let r = Self::rect(field, measure)?;
        Some(if Self::field(field) { plx_ui::text_field::plate(r) } else { r })
    }
    fn activate_browser_index(&mut self, index: usize) {
        if index >= self.rows.len() { return; }
        if self.rows[index].is_video {
            let request = self.controller.request().cloned();
            let share = self.controller.share().map(str::to_owned);
            let folder = self.controller.folder().to_owned();
            let queue = match (request, share) {
                (Some(request), Some(share)) => self.rows.iter().enumerate().skip(index + 1)
                    .filter(|(_, row)| row.is_video)
                    .filter_map(|(next_index, row)| {
                        plx_smb::browser::child_path(&folder, &row.entry.name).ok().map(|path| {
                            (next_index, plx_smb::controller::PlayRequest { request: request.clone(), share: share.clone(), path })
                        })
                    }).collect(),
                _ => Vec::new(),
            };
            plx_smb::playback::set_autoplay_queue(queue);
        } else {
            plx_smb::playback::clear_autoplay_queue();
        }
        let current = self.controller.browser().selected();
        self.controller.move_selection(index as isize - current as isize);
        self.selected = index;
        match self.controller.activate() {
            Activation::None => {}
            Activation::Play(play) => {
                if self.playback.is_some() {
                    self.error = Some("A video is already being prepared".into());
                } else {
                    match plx_smb::controller::spawn_prepare(play) {
                        Ok(rx) => { self.playback = Some(rx); self.connecting = true; self.error = None; }
                        Err(error) => { self.error = Some(error); self.connecting = false; }
                    }
                }
            }
        }
    }
    fn ids(&self) -> Vec<u32> {
        if self.menu_open {
            return vec![MENU_SAVED, MENU_ADD, MENU_DISCONNECT, MENU_LOCK, MENU_CLOSE];
        }
        if self.controller.phase()==Phase::Browsing {
            let rows: Vec<u32> = (0..self.rows.len()).map(|i| SHARE_BASE+i as u32).collect();
            if rows.is_empty() { vec![PROFILE] } else { rows }
        } else if !self.shares.is_empty() {
            (0..self.shares.len()).map(|i| SHARE_BASE + i as u32).chain(std::iter::once(CHANGE_SERVER)).chain(std::iter::once(PROFILE)).collect()
        } else if self.saved_view && !self.saved.connections().is_empty() {
            (0..self.saved.connections().len()).flat_map(|i| [SAVED_BASE+i as u32, REMOVE_BASE+i as u32]).chain(std::iter::once(NEW_CONNECTION)).chain(std::iter::once(PROFILE)).collect()
        } else {
            vec![HOST, PORT, USER, PASSWORD, CONNECT, GUEST, SAVE, PROFILE]
        }
    }
    fn profile_rect() -> Rect { Rect::new(MARGIN_X, 24.0, 150.0, 54.0) }
    fn menu_rect(index: usize) -> Rect {
        Rect::new(SCR_W - MARGIN_X - 320.0, 88.0 + index as f32 * (FIELD_H + GAP), 300.0, FIELD_H)
    }
    fn draw_profile_controls(&self, p: plx_ui::Painter, measure: &dyn Measure, focus: Option<u32>) {
        let label = CString::new("⚙  Profile").unwrap();
        Button::new(label.as_ptr(), theme::size::BODY, Self::profile_rect())
            .focused(focus == Some(PROFILE)).scale(1.0 + self.profile_pop.focus(0) * 0.08).draw(&Env::inert(), p);
        if self.menu_open {
            for (i, (id, title)) in [
                (MENU_SAVED, "Saved IPs"),
                (MENU_ADD, "Add IP"),
                (MENU_DISCONNECT, "Disconnect"),
                (MENU_LOCK, "App Lock / PIN"),
                (MENU_CLOSE, "Close"),
            ].iter().enumerate() {
                let title = CString::new(*title).unwrap();
                Button::new(title.as_ptr(), theme::size::BODY, Self::menu_rect(i))
                    .focused(focus == Some(*id)).scale(1.0 + self.menu_pop.focus(i) * 0.06).draw(&Env::inert(), p);
            }
        }
        let _ = measure;
    }
}
impl LogicalState for SmbConnectScreen {
 fn write(&self,c:&mut Canon){
  c.u32(self.host.text().len() as u32).u32(self.port.text().len() as u32)
   .u32(self.user.text().len() as u32).u32(self.password.text().len() as u32)
   .bool(self.form.guest).bool(self.form.save_connection).u32(self.editing.unwrap_or(0)).bool(self.connecting)
   .bool(self.error.is_some()).seq(self.shares.len()).seq(self.rows.len()).u32(self.selected as u32);
 }
 fn probe(&self,out:&mut String){out.push_str("smb_connect");}
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Machine<H> for SmbConnectScreen {
 type Ev=ScreenEvent<H>;
 fn step(&mut self,ev:&Self::Ev,_cx:&Cx<'_,H>,fx:&mut Effects<'_,H>)->Handled {
  if self.controller.phase()==Phase::Browsing {
   let src=SmbCardSource{rows:&self.rows};
   if let Some(CardEvent::Activate(elem))=self.browser_grid.on(ev,_cx,&src,fx) {
    if elem>=SHARE_BASE { self.activate_browser_index((elem-SHARE_BASE) as usize); }
    return Handled::Yes;
   }
  }
  match ev {
   ScreenEvent::Enter(Enter::Restored) => {
    if let Some((index, play)) = plx_smb::playback::take_autoplay_next() {
     let current=self.controller.browser().selected();
     self.controller.move_selection(index as isize-current as isize);
     self.selected=index;
     match plx_smb::controller::spawn_prepare(play) {
      Ok(rx)=>{self.playback=Some(rx);self.connecting=true;self.error=None;}
      Err(error)=>{self.error=Some(error);self.connecting=false;plx_smb::playback::clear_autoplay_queue();}
     }
    }
    Handled::Yes
   }
   ScreenEvent::Tick(t)=>{
    let focus=_cx.focus.current.map(|key|key.elem);
    self.profile_pop.step((focus==Some(PROFILE)).then_some(0),t.dt());
    let menu_focus=focus.and_then(|elem|[MENU_SAVED,MENU_ADD,MENU_DISCONNECT,MENU_LOCK,MENU_CLOSE].iter().position(|id|*id==elem));
    self.menu_pop.step(menu_focus,t.dt());
    if let Err(error)=self.poll(){self.error=Some(error);self.connecting=false;
    if let Some(rx)=self.playback.as_ref(){
     match rx.try_recv(){
      Ok(Ok(prepared))=>{self.playback=None;self.connecting=false;fx.push(Fx::App(crate::registry::AppFx::SmbPlaybackReady(prepared)));}
      Ok(Err(error))=>{self.playback=None;self.connecting=false;self.error=Some(format!("Could not prepare video: {error}"));}
      Err(std::sync::mpsc::TryRecvError::Empty)=>{self.connecting=true;}
      Err(std::sync::mpsc::TryRecvError::Disconnected)=>{self.playback=None;self.connecting=false;self.error=Some("The video preparation worker stopped unexpectedly".into());}
     }
    }
    Handled::Yes
   }
   ScreenEvent::Activate(elem) => {
    match *elem {
     PROFILE => { self.menu_open = true; self.set_editing(None,fx); fx.invalidate(plx_machine::present::Provenance::Input); }
     MENU_CLOSE => { self.menu_open = false; fx.invalidate(plx_machine::present::Provenance::Input); }
     MENU_SAVED => {
      self.menu_open = false;
      self.controller.disconnect(); self.shares.clear(); self.rows.clear(); self.connecting=false; self.save_pending=false; self.saved_id=None;
      self.saved_view = !self.saved.connections().is_empty();
      if !self.saved_view { self.error = Some("No saved IPs yet. Choose Add IP to connect to a server.".into()); }
      fx.invalidate(plx_machine::present::Provenance::Input);
     }
     MENU_ADD => {
      self.menu_open = false; self.controller.disconnect(); self.shares.clear(); self.rows.clear(); self.connecting=false; self.save_pending=false; self.saved_id=None; self.saved_view=false;
      self.host=TextBuffer::new(String::new(),0); self.port=TextBuffer::new("445".into(),3); self.user=TextBuffer::new(String::new(),0); self.password=TextBuffer::new(String::new(),0);
      self.form=ConnectForm::new(); self.error=None; self.set_editing(None,fx); fx.invalidate(plx_machine::present::Provenance::Input);
     }
     MENU_DISCONNECT => {
      self.menu_open = false; self.controller.disconnect(); self.shares.clear(); self.rows.clear(); self.selected=0; self.connecting=false; self.save_pending=false; self.saved_id=None;
      self.saved_view = !self.saved.connections().is_empty(); self.error=None; fx.invalidate(plx_machine::present::Provenance::Input);
     }
     MENU_LOCK => {
      self.menu_open = false; self.error=Some("App lock and PIN protection are planned for a later security update.".into());
      fx.invalidate(plx_machine::present::Provenance::Input);
     }
     e if e>=REMOVE_BASE && e<REMOVE_BASE+plx_smb::store::MAX_SAVED as u32 && self.saved_view => {
      let index=(e-REMOVE_BASE) as usize;
      if let Some(saved)=self.saved.connections().get(index) { let id=saved.id.clone(); self.saved.remove(&id); if let Err(error)=self.saved.save(){self.error=Some(error);} }
      if self.saved.connections().is_empty(){self.saved_view=false;}
      fx.invalidate(plx_machine::present::Provenance::Input);
     }
     HOST|PORT|USER|PASSWORD => self.set_editing(if self.editing==Some(*elem){None}else{Some(*elem)},fx),
     GUEST => { self.form.guest=!self.form.guest; if self.form.guest { self.set_editing(None,fx); } fx.invalidate(plx_machine::present::Provenance::Input); }
     SAVE => { self.form.save_connection=!self.form.save_connection; fx.invalidate(plx_machine::present::Provenance::Input); }
     NEW_CONNECTION => { self.saved_view=false; self.saved_id=None; self.error=None; fx.invalidate(plx_machine::present::Provenance::Input); }
     CHANGE_SERVER => { self.controller.disconnect(); self.shares.clear(); self.rows.clear(); self.selected=0; self.connecting=false; self.save_pending=false; self.saved_id=None; self.saved_view=false; self.error=None; self.set_editing(None,fx); fx.invalidate(plx_machine::present::Provenance::Input); }
     e if e>=SAVED_BASE && self.saved_view => {
      let index=(e-SAVED_BASE) as usize;
      if let Some(saved)=self.saved.connections().get(index).cloned(){
       self.host=TextBuffer::new(saved.profile.host.clone(),saved.profile.host.len());
       self.port=TextBuffer::new(saved.profile.port.to_string(),saved.profile.port.to_string().len());
       self.user=TextBuffer::new(saved.profile.username.clone(),saved.profile.username.len());
       let password=saved.profile.reveal_password().or_else(||saved.password.clone()).unwrap_or_default();
       self.password=TextBuffer::new(password.clone(),password.len());
       self.form.host=saved.profile.host.clone(); self.form.port=saved.profile.port.to_string();
       self.form.username=saved.profile.username.clone(); self.form.password=password;
       self.form.guest=saved.profile.guest; self.form.share=saved.profile.share.clone();
       self.form.save_connection=true; self.saved_id=Some(saved.id.clone());
       self.saved.mark_used(&saved.id); let _=self.saved.save();
       self.resume_path=Some(saved.last_path.clone());
       self.saved_view=false;
       if !saved.profile.guest && self.form.password.is_empty(){self.error=Some("Enter the password for this saved server".into());self.set_editing(Some(PASSWORD),fx);}
       else {if let Err(error)=self.connect(){self.error=Some(error);}}
       fx.invalidate(plx_machine::present::Provenance::Input);
      }
     }
     CONNECT => { self.set_editing(None,fx); if let Err(e)=self.connect(){self.error=Some(e);} }
     e if e>=SHARE_BASE && !self.shares.is_empty() && self.controller.phase()!=Phase::Browsing => { let i=(e-SHARE_BASE) as usize; if let Some(share)=self.shares.get(i){ self.controller.select_share(&share.name); self.connecting=true; self.error=None; } }
     e if e>=SHARE_BASE && self.controller.phase()==Phase::Browsing => {
      self.activate_browser_index((e-SHARE_BASE) as usize);
     }
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
      if *key==Key::Back { if self.menu_open {self.menu_open=false;fx.invalidate(plx_machine::present::Provenance::Input);Handled::Yes} else if self.saved_view {fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));Handled::Yes} else if self.controller.phase()==Phase::Browsing { match self.controller.go_back(){Back::Handled=>{self.error=None;self.connecting=true;},Back::Leave=>{self.controller.disconnect();self.shares.clear();self.rows.clear();self.connecting=false;self.saved_view=false;self.saved_id=None;self.error=None;} } Handled::Yes } else if !self.shares.is_empty() { self.controller.disconnect();self.shares.clear();self.rows.clear();self.connecting=false;self.saved_view=false;self.saved_id=None;self.error=None;fx.invalidate(plx_machine::present::Provenance::Input);Handled::Yes } else if !self.saved.connections().is_empty(){self.saved_view=true;self.error=self.saved.saved_temporarily().then(||"Saved in temporary TV storage; it may be lost after a TV restart".into());self.set_editing(None,fx);fx.invalidate(plx_machine::present::Provenance::Input);Handled::Yes} else {fx.push(Fx::Nav(NavOp::Dismiss(self.entry)));Handled::Yes} } else if matches!(key,Key::Up|Key::Down|Key::Left|Key::Right) { Handled::No } else {Handled::No}
    }
    _=>Handled::No
   },
   ScreenEvent::FocusMoved{to,..}=>{if self.editing.is_some_and(|e|e!=to.elem){self.set_editing(None,fx);} if self.controller.phase()==Phase::Browsing && to.elem>=SHARE_BASE {let next=(to.elem-SHARE_BASE) as usize;let delta=next as isize-self.controller.browser().selected() as isize;self.controller.move_selection(delta);self.selected=next;} Handled::Yes}
   ScreenEvent::WillLeave(_)|ScreenEvent::Cover|ScreenEvent::Suspend|ScreenEvent::Unmount=>{self.set_editing(None,fx);Handled::No}
   _=>Handled::No
  }
 }
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Focusable<H> for SmbConnectScreen {
 fn groups(&self,cx:&Cx<'_,H>,out:&mut Vec<GroupSpec>){
  let ids=self.ids(); if ids.is_empty(){return;}
  if self.controller.phase()==Phase::Browsing && !self.menu_open && !self.rows.is_empty() {
   out.push(GroupSpec{id:FORM_GROUP,kind:GroupKind::Grid{cols:6,holes:&[]},seat:Seat::Remembered,reachable:AxisMask::BOTH,edge:[EdgeRule::Geometric;4],extent:Rect::new(MARGIN_X,324.0,SCR_W-2.0*MARGIN_X,CARD_H),len:self.rows.len(),elem:ElemKind::Card});
   out.push(GroupSpec{id:PROFILE_GROUP,kind:GroupKind::Free,seat:Seat::First,reachable:AxisMask::BOTH,edge:[EdgeRule::Stop;4],extent:Self::profile_rect(),len:1,elem:ElemKind::Bare});
  } else {
   let extent=ids.iter().filter_map(|e|self.stop_rect(*e,cx.measure)).reduce(|a,b|a.union(b)).unwrap_or(Rect::FULL);
   out.push(GroupSpec{id:FORM_GROUP,kind:GroupKind::Free,seat:Seat::First,reachable:AxisMask::BOTH,edge:[EdgeRule::Stop;4],extent,len:ids.len(),elem:ElemKind::Bare});
  }
 }
 fn group_of(&self,elem:&u32,_cx:&Cx<'_,H>)->Option<plx_machine::machine::GroupId>{if !self.menu_open && self.controller.phase()==Phase::Browsing && !self.rows.is_empty() && *elem==PROFILE {Some(PROFILE_GROUP)} else {self.ids().contains(elem).then_some(FORM_GROUP)}}
 fn neighbour(&self,key:FocusKey<u32>,dir:Dir,_cx:&Cx<'_,H>)->Step<u32>{
  if self.menu_open {
   let menu=[MENU_SAVED,MENU_ADD,MENU_DISCONNECT,MENU_LOCK,MENU_CLOSE];
   if let Some(i)=menu.iter().position(|e|*e==key.elem) {
    let next=match dir {Dir::Up=>i.checked_sub(1),Dir::Down=>(i+1<menu.len()).then_some(i+1),_=>None};
    return next.map_or(Step::Edge,|j|Step::Move(FocusKey{entry:self.entry,elem:menu[j]}));
   }
   return Step::Edge;
  }
  if self.controller.phase()==Phase::Browsing && !self.menu_open {
   if key.elem==PROFILE { return if dir==Dir::Down && !self.rows.is_empty(){Step::Move(FocusKey{entry:self.entry,elem:SHARE_BASE})}else{Step::Edge}; }
   if key.elem>=SHARE_BASE && dir==Dir::Up && (key.elem-SHARE_BASE)<6 { return Step::Move(FocusKey{entry:self.entry,elem:PROFILE}); }
   let src=SmbCardSource{rows:&self.rows}; return self.browser_grid.neighbour::<H,_>(&src,key,dir);
  }
  if !self.shares.is_empty() {
   if key.elem==PROFILE {
    return if dir==Dir::Down && !self.shares.is_empty(){Step::Move(FocusKey{entry:self.entry,elem:SHARE_BASE})}else{Step::Edge};
   }
   if key.elem==SHARE_BASE && dir==Dir::Up { return Step::Move(FocusKey{entry:self.entry,elem:PROFILE}); }
   let ids: Vec<u32> = (0..self.shares.len()).map(|i|SHARE_BASE+i as u32).chain(std::iter::once(CHANGE_SERVER)).collect();
   if let Some(index)=ids.iter().position(|id|*id==key.elem) {
    let target=match dir { Dir::Up=>index.checked_sub(1), Dir::Down=>(index+1<ids.len()).then_some(index+1), _=>None };
    if let Some(next)=target { return Step::Move(FocusKey{entry:self.entry,elem:ids[next]}); }
   }
   return Step::Edge;
  }
  // The three actions share one horizontal row. Declare their D-pad links explicitly
  // before the general geometric search so remote navigation follows their actual layout.
  let horizontal = match (key.elem, dir) {
   (CONNECT, Dir::Right) => Some(GUEST),
   (GUEST, Dir::Left) => Some(CONNECT),
   (GUEST, Dir::Right) => Some(SAVE),
   (SAVE, Dir::Left) => Some(GUEST),
   _ => None,
  };
  if let Some(elem)=horizontal.filter(|elem|self.ids().contains(elem)) {
   return Step::Move(FocusKey{entry:self.entry,elem});
  }
  let ids=self.ids();
  let Some(from)=self.stop_rect(key.elem,_cx.measure) else{return Step::Edge};
  let mut best:Option<(f32,u32)>=None;
  for candidate in ids.into_iter().filter(|id|*id!=key.elem) {
   let Some(to)=self.stop_rect(candidate,_cx.measure) else{continue};
   let (primary,orthogonal)=match dir {
    Dir::Up=>(from.y-(to.y+to.h),(from.cx()-to.cx()).abs()),
    Dir::Down=>(to.y-(from.y+from.h),(from.cx()-to.cx()).abs()),
    Dir::Left=>(from.x-(to.x+to.w),(from.cy()-to.cy()).abs()),
    Dir::Right=>(to.x-(from.x+from.w),(from.cy()-to.cy()).abs()),
   };
   // Directional, situation-aware D-pad movement: only candidates genuinely
   // in the pressed half-plane qualify; nearest axis distance wins, with a
   // smaller penalty for horizontal/vertical misalignment.
   if primary < -1.0 {continue;}
   let score=primary.max(0.0)*3.0+orthogonal;
   if best.is_none_or(|(old,_)|score<old){best=Some((score,candidate));}
  }
  best.map_or(Step::Edge,|(_,elem)|Step::Move(FocusKey{entry:self.entry,elem}))
 }
 fn place(&self,elem:&u32,cx:&Cx<'_,H>,at:At)->Option<Placed>{
  if *elem==PROFILE { let r=Self::profile_rect(); return Some(Placed{rect:r,rest_rect:r,clip:Rect::FULL,index:Some(0)}); }
  if *elem==MENU_SAVED || *elem==MENU_ADD || *elem==MENU_DISCONNECT || *elem==MENU_LOCK || *elem==MENU_CLOSE {
   let i=match *elem {MENU_SAVED=>0,MENU_ADD=>1,MENU_DISCONNECT=>2,MENU_LOCK=>3,_=>4}; let r=Self::menu_rect(i);
   return Some(Placed{rect:r,rest_rect:r,clip:Rect::FULL,index:Some(i as u32)});
  }
  if *elem>=REMOVE_BASE && *elem<REMOVE_BASE+plx_smb::store::MAX_SAVED as u32 {
   let i=(*elem-REMOVE_BASE) as usize; let layout=RouteLayout::screen(); let r=Rect::new(layout.content.x+540.0,layout.content.y+i as f32*(FIELD_H+GAP),150.0,FIELD_H);
   return Some(Placed{rect:r,rest_rect:r,clip:Rect::FULL,index:Some(i as u32)});
  }
  if self.controller.phase()==Phase::Browsing && *elem>=SHARE_BASE { let src=SmbCardSource{rows:&self.rows}; return self.browser_grid.place(cx,&src,elem,at); }
  let r=if *elem>=SAVED_BASE && *elem<REMOVE_BASE {let i=(*elem-SAVED_BASE) as usize; let layout=RouteLayout::screen();Some(Rect::new(layout.content.x,layout.content.y+i as f32*(FIELD_H+GAP),520.0,FIELD_H))}
  else if *elem>=SHARE_BASE && *elem<SHARE_BASE+self.shares.len() as u32 {let i=(*elem-SHARE_BASE) as usize;let base=Self::rect(HOST,cx.measure)?;Some(Rect::new(base.x,base.y+i as f32*(FIELD_H+GAP),base.w,FIELD_H))}
  else if *elem==NEW_CONNECTION {let layout=RouteLayout::screen();Some(Rect::new(layout.content.x,layout.content.y+self.saved.connections().len() as f32*(FIELD_H+GAP),layout.content.w.min(FIELD_W),FIELD_H))}
  else if *elem==CHANGE_SERVER {let base=Self::rect(HOST,cx.measure)?;Some(Rect::new(base.x,base.y+self.shares.len() as f32*(FIELD_H+GAP),base.w,FIELD_H))}
  else{self.stop_rect(*elem,cx.measure)}?;
  Some(Placed{rect:r,rest_rect:r,clip:Rect::FULL,index:Some(self.ids().iter().position(|e|e==elem).unwrap_or(0) as u32)})
 }
 fn reconcile(&self,want:FocusKey<u32>,_cx:&Cx<'_,H>)->FocusKey<u32>{if (want.elem==PROFILE && !self.menu_open && self.controller.phase()==Phase::Browsing && !self.rows.is_empty()) || self.ids().contains(&want.elem){want}else{FocusKey{entry:self.entry,elem:self.ids().first().copied().unwrap_or(HOST)}}}
 fn seat(&self,g:plx_machine::machine::GroupId,from:Placed,_cx:&Cx<'_,H>)->FocusKey<u32>{
  if g==PROFILE_GROUP { return FocusKey{entry:self.entry,elem:PROFILE}; }
  if self.controller.phase()==Phase::Browsing && !self.menu_open { let src=SmbCardSource{rows:&self.rows}; if let Some(key)=self.browser_grid.seat::<H,SmbCardSource<'_>>(&src,from){return key;} }
  FocusKey{entry:self.entry,elem:self.ids().first().copied().unwrap_or(HOST)}
 }
}
impl<H:crate::registry::AppLike<Memory=crate::registry::PageMemory>> Screen<H> for SmbConnectScreen {
 fn name(&self)->&'static str{"smb_connect"}
 fn state(&self)->&dyn LogicalState{self}
 fn crumb(&self,_cx:&Cx<'_,H>)->Option<Cow<'_,str>>{None}
 fn prepare(&mut self,_b:&mut Budget,_cx:&Cx<'_,H>){}
 fn scrim(&self)->Scrim{Scrim::NONE}
 fn draw(&mut self,f:&mut DrawFrame<'_, '_,H>){
  let p=f.painter; let layout=RouteLayout::screen(); let measure=f.measure;
  let focus=f.focus.current.map(|k|k.elem);
  // Each screen state owns ONE header. Drawing the generic connect narrative first and
  // the browser/share narrative second caused both to occupy the same top-left area.
  if self.controller.phase()!=Phase::Browsing && !self.saved_view && self.shares.is_empty() {
   let body=if self.connecting{"Connecting to the SMB server and loading shares…".to_owned()}
    else if let Some(error)=self.error.as_deref(){format!("Connection failed: {error}")}
    else{"Enter your server details to browse and play videos.".to_owned()};
   layout.draw_narrative(p,None,"Connect to SMB",&body,measure);
  }
  if self.saved_view && !self.saved.connections().is_empty() {
   let saved_copy=self.error.as_deref().unwrap_or("Choose a saved server or create a new connection.");
   layout.draw_narrative(p,None,"Saved SMB servers",saved_copy,measure);
   for (i,saved) in self.saved.connections().iter().enumerate(){
    let id=SAVED_BASE+i as u32; let y=layout.content.y+i as f32*(FIELD_H+GAP);
    let r=Rect::new(layout.content.x,y,520.0,FIELD_H);
    let label=format!("{}  ·  {}",saved.name,if saved.profile.guest{"Guest"}else{&saved.profile.username});
    TextField::new(plx_os::i18n::msg::browse_jf_server_c(),&label).hot(if focus==Some(id){1.0}else{0.0}).draw(p,r,measure);
    let remove=CString::new("Remove").unwrap();
    Button::new(remove.as_ptr(),theme::size::BODY,Rect::new(layout.content.x+540.0,y,150.0,FIELD_H))
      .focused(focus==Some(REMOVE_BASE+i as u32)).draw(&Env::inert(),p);
   }
   let r=Rect::new(layout.content.x,layout.content.y+self.saved.connections().len() as f32*(FIELD_H+GAP),layout.content.w.min(FIELD_W),FIELD_H);
   Button::new(CString::new("New connection").unwrap().as_ptr(),theme::size::BODY,r).focused(focus==Some(NEW_CONNECTION)).draw(&Env::inert(),p);
   self.draw_profile_controls(p,measure,focus);
   return;
  }
  if self.controller.phase()==Phase::Browsing {
   let browser_copy=self.error.as_deref().map(str::to_owned).unwrap_or_else(||self.controller.location());
   layout.draw_narrative(p.translate(0.0, -self.browser_grid.scroll()),None,"SMB Browser",&browser_copy,measure);
   let src=SmbCardSource{rows:&self.rows};
   self.browser_grid.draw(f,p.clipped(Rect::new(MARGIN_X,300.0,SCR_W-2.0*MARGIN_X,plx_ui::consts::SCR_H-300.0)),&src);
   self.draw_profile_controls(p,measure,focus);
   return;
  }
  if !self.shares.is_empty(){
   // Keep save/credential-store failures visible on the share picker instead of
   // silently hiding them behind the early-returning share list.
   if let Some(error)=self.error.as_deref(){
    layout.draw_narrative(p,None,"SMB Shares",error,measure);
   } else {
    layout.draw_narrative(p,None,"SMB Shares","Select a share to browse its folders.",measure);
   }
   for (i,share) in self.shares.iter().enumerate(){
    let id=SHARE_BASE+i as u32; let Some(r)=Self::rect(HOST,measure).map(|r|Rect::new(r.x,r.y+i as f32*(FIELD_H+GAP),r.w,FIELD_H)) else{continue};
    TextField::new(plx_os::i18n::msg::browse_jf_server_c(),&share.name).hot(if focus==Some(id){1.0}else{0.0}).draw(p,r,measure);
   }
   let share_base=Self::rect(HOST,measure).unwrap_or(layout.content);
   let change_y=share_base.y+self.shares.len() as f32*(FIELD_H+GAP);
   let change_r=Rect::new(share_base.x,change_y,share_base.w,FIELD_H);
   Button::new(CString::new("Change server / IP").unwrap().as_ptr(),theme::size::BODY,change_r).focused(focus==Some(CHANGE_SERVER)).draw(&Env::inert(),p);
   self.draw_profile_controls(p,measure,focus);
   return;
  }
  let fields=[(HOST,"Server / IP",&self.host,false),(PORT,"Port",&self.port,false),(USER,"Username",&self.user,false),(PASSWORD,"Password",&self.password,true)];
  for (id,label,buffer,secret) in fields{
   if let Some(r)=Self::rect(id,measure){
    let label=CString::new(label).expect("static field label");
    TextField::new(label.as_c_str(),buffer.text()).caret(buffer.caret()).secret(secret).hot(if focus==Some(id){1.0}else{0.0}).editing(self.editing==Some(id),true).draw(p,r,measure);
   }
  }
  for (id,label) in [(CONNECT,"Connect"),(GUEST,if self.form.guest{"Guest: On"}else{"Guest: Off"}),(SAVE,if self.form.save_connection{"Save: On"}else{"Save: Off"})]{
   if let Some(r)=Self::rect(id,measure){
    let label=CString::new(label).expect("static button label");
    Button::new(label.as_ptr(),theme::size::BODY,r).focused(focus==Some(id)).draw(&Env::inert(),p);
   }
  }
  self.draw_profile_controls(p,measure,focus);
 }
 fn render(&self)->RenderStrategy{RenderStrategy::Page}
 fn focus_source(&self)->FocusSource{FocusSource::Engine}
 fn hit_source(&self)->HitSource{HitSource::Engine}
 fn text_entry(&self)->TextEntry{self.text_entry()}
 fn as_any(&self)->Option<&dyn std::any::Any>{Some(self)}
 fn as_any_mut(&mut self)->Option<&mut dyn std::any::Any>{Some(self)}
}
