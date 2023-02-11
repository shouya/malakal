use std::sync::Arc;

use bimap::BiMap;
use chrono::Timelike;
use eframe::egui::{
  self, text::LayoutJob, CursorIcon, Key, Label, LayerId, Modifiers, Rect,
  Response, Sense, Ui,
};
use humantime;

use crate::{
  event::Event,
  util::{on_the_same_day, reorder_times, DateTime},
};

use super::{
  layout::Layout, move_event, move_event_end, move_event_start, EventId,
  ScheduleUi,
};

#[derive(Clone, Copy, Debug)]
struct DraggingEventYOffset(f32);

#[derive(Clone, Debug, PartialEq)]
enum Change {
  Added { new: Event },
  Removed { old: Event },
  Modified { old: Event, new: Event },
}

impl Change {
  fn reverse(self) -> Self {
    use Change::*;

    match self {
      Added { new } => Removed { old: new },
      Removed { old } => Added { new: old },
      Modified { old, new } => Modified { new: old, old: new },
    }
  }

  fn new_removed(events: &[Event], event_id: &EventId) -> Option<Self> {
    events
      .iter()
      .find(|&e| &e.id == event_id)
      .cloned()
      .map(|old| Change::Removed { old })
  }

  fn new_changed(events: &[Event], changed_event: Event) -> Self {
    if let Some(existing) =
      events.iter().find(|&e| e.id == changed_event.id).cloned()
    {
      Change::Modified {
        old: existing,
        new: changed_event,
      }
    } else {
      Change::Added { new: changed_event }
    }
  }

  fn apply(&self, events: &mut Vec<Event>) {
    match self.clone() {
      Change::Added { mut new } => {
        new.mark_changed();
        events.push(new)
      }
      Change::Removed { old } => {
        if let Some(e) = events.iter_mut().find(|e| e.id == old.id) {
          e.mark_deleted();
        }
      }
      Change::Modified { old, mut new } => {
        new.mark_changed();

        if let Some(e) = events.iter_mut().find(|e| e.id == old.id) {
          *e = new;
        }
      }
    }
  }
}

#[derive(Clone, Debug, Default)]
struct EventFocusRegistry {
  events: BiMap<egui::Id, EventId>,
}

impl EventFocusRegistry {
  fn with<F, A>(ui: &Ui, f: F) -> A
  where
    F: Fn(&mut Self) -> A,
  {
    f(ui.memory().data.get_temp_mut_or_default(ui.id()))
  }

  fn reset(&mut self) {
    self.events.clear();
  }

  fn register(&mut self, ui_id: egui::Id, event_id: &EventId) {
    self.events.insert(ui_id, event_id.clone());
  }

  fn get_event_id(&self, ui_id: &egui::Id) -> Option<&EventId> {
    self.events.get_by_left(ui_id)
  }

  fn get_ui_id(&self, event_id: &EventId) -> Option<&egui::Id> {
    self.events.get_by_right(event_id)
  }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct History {
  changes: Vec<Change>,
}

impl History {
  pub(super) fn clear(&mut self) {
    self.changes.clear()
  }

  fn save(&mut self, change: Change) {
    self.changes.push(change);
  }

  fn pop(&mut self) -> Option<Change> {
    self.changes.pop()
  }
}

#[derive(Clone, Debug)]
struct InteractingEvent {
  event: Event,
  state: FocusedEventState,
}

impl InteractingEvent {
  fn id() -> egui::Id {
    egui::Id::new("interacting_event")
  }

  fn get(ui: &Ui) -> Option<Self> {
    ui.memory().data.get_temp(Self::id())
  }

  fn set(ui: &Ui, event: Event, state: FocusedEventState) {
    let value = InteractingEvent { event, state };
    ui.memory().data.insert_temp(Self::id(), value)
  }

  fn save(self, ui: &Ui) {
    Self::set(ui, self.event.clone(), self.state)
  }

  fn discard(ui: &Ui) {
    ui.memory().data.remove::<Self>(Self::id())
  }

  fn commit(self, ui: &Ui) {
    ui.memory().data.insert_temp(Self::id(), self.event);
    Self::discard(ui);
  }

  fn take_commited_event(ui: &Ui) -> Option<Event> {
    let event = ui.memory().data.get_temp(Self::id());
    ui.memory().data.remove::<Event>(Self::id());
    event
  }

  fn get_id(ui: &Ui, id: &EventId) -> Option<Self> {
    Self::get(ui).and_then(|value| (&value.event.id == id).then_some(value))
  }

  fn get_event(ui: &Ui) -> Option<Event> {
    Self::get(ui).map(|v| v.event)
  }
}

#[derive(Debug, Clone)]
struct RefocusingEvent(Arc<EventId>);

impl RefocusingEvent {
  fn new(event_id: &EventId) -> Self {
    Self(Arc::new(event_id.clone()))
  }

  fn id(_ui: &Ui) -> egui::Id {
    // because ui.id() doesn't seem to be consistent enough
    egui::Id::null()
  }

  fn request_focus(ui: &Ui, event_id: &EventId) {
    let refocusing_event = Self::new(event_id);
    ui.memory().data.insert_temp(Self::id(ui), refocusing_event);
  }

  fn take(ui: &Ui) -> Option<Self> {
    let egui_id = Self::id(ui);
    let rfe = ui.memory().data.get_temp::<RefocusingEvent>(egui_id)?;
    ui.memory().data.remove::<Self>(egui_id);

    Some(rfe)
  }

  fn apply_focus(ui: &Ui) {
    let rfe = match Self::take(ui) {
      Some(x) => x,
      None => return,
    };

    let event_id = rfe.0.as_ref();
    let ui_id =
      EventFocusRegistry::with(ui, |r| r.get_ui_id(event_id).copied());

    if let Some(ui_id) = ui_id {
      ui.memory().request_focus(ui_id);
    }
  }
}

#[derive(Clone, Debug)]
struct DeletedEvent {
  event_id: EventId,
}

impl DeletedEvent {
  fn id() -> egui::Id {
    egui::Id::new("deleted_event")
  }

  fn set(ui: &Ui, event_id: &EventId) {
    ui.memory().data.insert_temp(
      Self::id(),
      Self {
        event_id: event_id.clone(),
      },
    );
  }

  fn take(ui: &Ui) -> Option<EventId> {
    let deleted_event = ui.memory().data.get_temp(Self::id());
    ui.memory().data.remove::<Self>(Self::id());
    deleted_event.map(|x: Self| x.event_id)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedEventState {
  Editing,
  Dragging,
  DraggingEventStart,
  DraggingEventEnd,
  EventCloning,
}

impl ScheduleUi {
  // pub(super) fn handle_focus_move(ui: &mut Ui) {
  //   let current_focus = match ui.memory().focus() {
  //     Some(f) => f,
  //     None => return,
  //   };

  //   EventFocusRegistry::with_ui(ui, |r| r.reset())
  // }

  fn interact_event_region_keyboard(
    &self,
    ui: &mut Ui,
    resp: &Response,
  ) -> Option<FocusedEventState> {
    use FocusedEventState::*;

    if !resp.has_focus() {
      return None;
    }

    // pressing enter on a focused event - change to edit mode
    if ui.input_mut().consume_key(Modifiers::NONE, Key::Enter) {
      return Some(Editing);
    }

    None
  }

  fn interact_event_region(
    &self,
    ui: &mut Ui,
    resp: &Response,
  ) -> Option<FocusedEventState> {
    use FocusedEventState::*;
    let event_rect = resp.rect;
    let [upper, lower] = self.event_resizer_regions(event_rect);

    let _lmb = egui::PointerButton::Primary;

    let interact_pos =
      resp.interact_pointer_pos().or_else(|| resp.hover_pos())?;

    match detect_interaction(resp) {
      None => {
        if upper.contains(interact_pos) || lower.contains(interact_pos) {
          ui.output().cursor_icon = CursorIcon::ResizeVertical;
        } else if event_rect.contains(interact_pos) {
          ui.output().cursor_icon = CursorIcon::Grab;
        }
        None
      }
      Some(Interaction::Clicked)
        if resp.clicked_by(egui::PointerButton::Primary) =>
      {
        Some(Editing)
      }
      Some(Interaction::DragStarted { origin })
        if resp.dragged_by(egui::PointerButton::Primary) =>
      {
        if upper.contains(origin) {
          return Some(DraggingEventStart);
        }
        if lower.contains(origin) {
          return Some(DraggingEventEnd);
        }

        let offset = DraggingEventYOffset(event_rect.top() - origin.y);
        ui.memory().data.insert_temp(egui::Id::null(), offset);
        if ui.input().modifiers.ctrl {
          Some(EventCloning)
        } else {
          Some(Dragging)
        }
      }
      _ => None,
    }
  }

  fn interact_event(
    &self,
    ui: &mut Ui,
    event_rect: Rect,
    state: FocusedEventState,
    event: &mut Event,
  ) -> (Response, Option<bool>) {
    let [upper, lower] = self.event_resizer_regions(event_rect);

    let resp = self.place_event_button(ui, event_rect, event);
    let commit = match state {
      FocusedEventState::DraggingEventStart => {
        self.handle_event_resizing(ui, upper, |time| {
          move_event_start(event, time, self.min_event_duration);
          event.start
        })
      }
      FocusedEventState::DraggingEventEnd => {
        self.handle_event_resizing(ui, lower, |time| {
          move_event_end(event, time, self.min_event_duration);
          event.end
        })
      }
      FocusedEventState::Dragging => {
        self.handle_event_dragging(ui, event_rect, |time| {
          move_event(event, time);
          (event.start, event.end)
        })
      }
      _ => unreachable!(),
    };

    (resp, commit)
  }

  fn handle_event_resizing(
    &self,
    ui: &mut Ui,
    rect: Rect,
    set_time: impl FnOnce(DateTime) -> DateTime,
  ) -> Option<bool> {
    if !ui.memory().is_anything_being_dragged() {
      return Some(true);
    }

    ui.output().cursor_icon = CursorIcon::ResizeVertical;

    let pointer_pos = self.relative_pointer_pos(ui).unwrap();

    if let Some(datetime) = self.pointer_to_datetime_auto(ui, pointer_pos) {
      let updated_time = set_time(datetime);
      self.show_resizer_hint(ui, rect, updated_time);
    }

    None
  }

  fn handle_event_dragging(
    &self,
    ui: &mut Ui,
    rect: Rect,
    set_time: impl FnOnce(DateTime) -> (DateTime, DateTime),
  ) -> Option<bool> {
    if !ui.memory().is_anything_being_dragged() {
      return Some(true);
    }

    ui.output().cursor_icon = CursorIcon::Grabbing;

    let mut pointer_pos = self.relative_pointer_pos(ui).unwrap();
    if let Some(offset_y) = ui
      .memory()
      .data
      .get_temp::<DraggingEventYOffset>(egui::Id::null())
    {
      pointer_pos.y += offset_y.0;
    }

    if let Some(datetime) = self.pointer_to_datetime_auto(ui, pointer_pos) {
      let (beg, end) = set_time(datetime);
      let [upper, lower] = self.event_resizer_regions(rect);
      self.show_resizer_hint(ui, upper, beg);
      self.show_resizer_hint(ui, lower, end);
    }

    None
  }

  pub(super) fn put_non_interacting_event_block(
    &self,
    ui: &mut Ui,
    layout: &Layout,
    event: &Event,
  ) -> Option<()> {
    let event_rect = self.event_rect(ui, layout, event)?;

    let resp = self.place_event_button(ui, event_rect, event);

    EventFocusRegistry::with(ui, |r| r.register(resp.id, &event.id));

    let interaction = self
      .interact_event_region_keyboard(ui, &resp)
      .or_else(|| self.interact_event_region(ui, &resp));

    match interaction {
      None => (),
      Some(FocusedEventState::EventCloning) => {
        let new_event = self.clone_to_new_event(event);
        InteractingEvent::set(ui, new_event, FocusedEventState::Dragging);
      }
      Some(state) => InteractingEvent::set(ui, event.clone(), state),
    }

    Some(())
  }

  pub(super) fn put_interacting_event_block(
    &self,
    ui: &mut Ui,
    layout: &Layout,
  ) -> Option<()> {
    use FocusedEventState::*;

    let mut ie = InteractingEvent::get(ui)?;
    let event_rect = self.event_rect(ui, layout, &ie.event)?;

    match ie.state {
      Editing => match self.place_event_editor(ui, event_rect, &mut ie.event) {
        None => ie.save(ui),
        Some(true) => ie.commit(ui),
        Some(false) => InteractingEvent::discard(ui),
      },
      _ => {
        let event_rect = self.event_rect(ui, layout, &ie.event)?;

        let (_resp, commit) =
          self.interact_event(ui, event_rect, ie.state, &mut ie.event);

        match commit {
          None => ie.save(ui),
          Some(true) => ie.commit(ui),
          Some(false) => InteractingEvent::discard(ui),
        }
      }
    }

    Some(())
  }

  fn place_event_button(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &Event,
  ) -> Response {
    let (layout, clipped) = self.shorten_event_label(ui, rect, &event.title);

    let button = egui::Button::new(layout).sense(Sense::click_and_drag());
    let resp = ui.put(rect, button);

    if clipped {
      // text is clipped, show a tooltip
      resp.clone().on_hover_text(event.title.clone());
    }

    let format_time = |time: DateTime| {
      if time.second() == 0 {
        time.format("%H:%M")
      } else {
        time.format("%H:%M:%S")
      }
    };

    resp.clone().context_menu(|ui| {
      if let Some(desc) = &event.description {
        ui.label(desc.to_string());
      }

      ui.label(format!(
        "{}--{} ({})",
        format_time(event.start),
        format_time(event.end),
        (event.end - event.start)
          .to_std()
          .map(|d| humantime::format_duration(d).to_string())
          .unwrap_or_else(|_| "negative duration".to_string())
      ));

      ui.separator();

      if ui.button("Delete").clicked() {
        DeletedEvent::set(ui, &event.id);
        ui.close_menu();
      }

      if ui.button("Close menu").clicked() {
        ui.close_menu();
      }
    });

    resp
  }

  fn shorten_event_label(
    &self,
    ui: &mut Ui,
    rect: Rect,
    label: &str,
  ) -> (impl Into<egui::WidgetText>, bool) {
    let font_id = egui::TextStyle::Button.resolve(ui.style());
    let color = ui.visuals().text_color();

    let layout_job = |text| {
      let mut j = LayoutJob::simple_singleline(text, font_id.clone(), color);
      j.wrap.max_width = rect.shrink2(ui.spacing().button_padding).width();
      j
    };

    let job = layout_job(label.into());
    let line_height = job.font_height(&ui.fonts());
    let mut galley = ui.fonts().layout_job(job);

    if galley.size().y <= line_height {
      // multiline
      return (galley, false);
    }

    for n in (0..(label.len() - 3)).rev() {
      let text = format!("{}..", &label[0..n]);
      galley = ui.fonts().layout_job(layout_job(text));
      if galley.size().y <= line_height {
        return (galley, true);
      }
    }

    (galley, false)
  }

  // Some(true) => commit change
  // Some(false) => discard change
  // None => still editing
  fn place_event_editor(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &mut Event,
  ) -> Option<bool> {
    let editor = egui::TextEdit::singleline(&mut event.title);
    let resp = ui.put(rect, editor);

    // Anything dragging outside the textedit should be equivalent to
    // losing focus. Note: we still need to allow dragging within the
    // textedit widget to allow text selection, etc.
    let anything_else_dragging = ui.memory().is_anything_being_dragged()
      && !resp.dragged()
      && !resp.drag_released();

    // We cannot use key_released here, because it will be taken
    // precedence by resp.lost_focus() and commit the change.
    if ui.input().key_pressed(egui::Key::Escape) {
      return Some(false);
    }

    if resp.lost_focus() || resp.clicked_elsewhere() || anything_else_dragging {
      return Some(true);
    }

    resp.request_focus();
    None
  }

  fn show_resizer_hint(&self, ui: &mut Ui, rect: Rect, time: DateTime) {
    let layer_id = egui::Id::new("resizer_hint");
    let layer = LayerId::new(egui::Order::Tooltip, layer_id);

    let text = format!("{}", time.format(self.event_resizing_hint_format));
    let label = Label::new(egui::RichText::new(text).monospace());

    ui.with_layer_id(layer, |ui| ui.put(rect, label));
  }

  pub(super) fn handle_new_event(
    &self,
    ui: &mut Ui,
    response: &Response,
  ) -> Option<()> {
    use FocusedEventState::Editing;

    let id = response.id;
    let interaction = detect_interaction(response);

    match interaction {
      None => (),
      Some(Interaction::Clicked)
        if response.clicked_by(egui::PointerButton::Primary) =>
      {
        InteractingEvent::discard(ui);
        return Some(());
      }
      Some(Interaction::DragStarted { .. })
        if response.dragged_by(egui::PointerButton::Primary) =>
      {
        let mut event = self.new_event();
        let pointer_pos = self.relative_pointer_pos(ui)?;
        let init_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;
        let new_state =
          self.assign_new_event_dates(ui, init_time, &mut event)?;

        ui.memory().data.insert_temp(id, event.id.clone());
        ui.memory().data.insert_temp(id, init_time);

        InteractingEvent::set(ui, event, new_state);

        return Some(());
      }
      Some(Interaction::DragReleased) => {
        let event_id = ui.memory().data.get_temp(id)?;
        let mut value = InteractingEvent::get_id(ui, &event_id)?;
        value.state = Editing;
        value.save(ui);
      }
      Some(Interaction::Dragged)
        if response.dragged_by(egui::PointerButton::Primary) =>
      {
        let event_id: String = ui.memory().data.get_temp(id)?;
        let init_time = ui.memory().data.get_temp(id)?;
        let mut value = InteractingEvent::get_id(ui, &event_id)?;
        let new_state =
          self.assign_new_event_dates(ui, init_time, &mut value.event)?;
        value.state = new_state;
        value.save(ui);
      }

      _ => (),
    }

    Some(())
  }

  fn assign_new_event_dates(
    &self,
    ui: &Ui,
    init_time: DateTime,
    event: &mut Event,
  ) -> Option<FocusedEventState> {
    use FocusedEventState::{DraggingEventEnd, DraggingEventStart};

    let pointer_pos = self.relative_pointer_pos(ui)?;
    let new_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;

    let (mut start, mut end) = (init_time, new_time);
    let reordered = reorder_times(&mut start, &mut end);

    // the event crossed the day boundary, we need to pick a direction
    // based on the initial drag position
    if !on_the_same_day(start, end) {
      if self.day_progress(&init_time) < 0.5 {
        start = init_time;
        end = init_time + self.min_event_duration;
      } else {
        end = init_time;
        start = init_time - self.min_event_duration;
      }
    };

    event.start = start;
    event.end = end;

    if reordered {
      Some(DraggingEventStart)
    } else {
      Some(DraggingEventEnd)
    }
  }

  pub(super) fn get_interacting_event(&self, ui: &Ui) -> Option<Event> {
    InteractingEvent::get_event(ui)
  }

  pub(super) fn apply_interacting_events(&mut self, ui: &Ui) {
    if let Some(event) = InteractingEvent::take_commited_event(ui) {
      RefocusingEvent::request_focus(ui, &event.id);

      let change = Change::new_changed(&self.events, event);
      change.apply(&mut self.events);
      self.history.save(change);
    }

    // commit deleted event
    if let Some(event_id) = DeletedEvent::take(ui) {
      if let Some(change) = Change::new_removed(&self.events, &event_id) {
        change.apply(&mut self.events);
        self.history.save(change);
      }
    }
  }

  pub(super) fn refocus_edited_event(&self, ui: &Ui) {
    RefocusingEvent::apply_focus(ui);
  }

  pub(super) fn handle_undo(&mut self, ui: &mut Ui) {
    let ctrl_z = ui.input_mut().consume_key(Modifiers::CTRL, egui::Key::Z);

    if !ctrl_z {
      return;
    }

    if let Some(change) = self.history.pop() {
      change.reverse().apply(&mut self.events)
    }
  }
}

#[derive(Debug)]
enum Interaction {
  Clicked,
  DragStarted { origin: egui::Pos2 },
  DragReleased,
  Dragged,
}

// https://docs.rs/egui/latest/src/egui/input_state.rs.html#11-15
const MAX_CLICK_DIST: f32 = 6.0;
const MAX_CLICK_DURATION: f64 = 0.6;

fn detect_interaction(response: &Response) -> Option<Interaction> {
  use Interaction::*;

  // this state remembers if we have detected any click/drag_started
  // already.
  #[derive(Clone)]
  struct DetectionFinishFlag(bool);

  let pointer = response.ctx.input().pointer.clone();

  let set_flag = |value| {
    response
      .ctx
      .memory()
      .data
      .get_temp_mut_or(response.id, DetectionFinishFlag(false))
      .0 = value;
  };

  let get_flag = || {
    response
      .ctx
      .memory()
      .data
      .get_temp_mut_or(response.id, DetectionFinishFlag(false))
      .0
  };

  if !pointer.any_down() {
    set_flag(false);
  }

  if !get_flag() && response.clicked() {
    set_flag(true);
    return Some(Clicked);
  }

  if response.drag_released() {
    return Some(DragReleased);
  }

  if !get_flag() && response.dragged() {
    let origin = pointer.press_origin().unwrap();
    if let Some(pos) = pointer.hover_pos() {
      let dx = (pos - origin).length_sq();
      if dx > MAX_CLICK_DIST * MAX_CLICK_DIST {
        set_flag(true);
        return Some(DragStarted { origin });
      }
    }

    let dt = response.ctx.input().time - pointer.press_start_time().unwrap();
    if dt > MAX_CLICK_DURATION {
      set_flag(true);
      return Some(DragStarted { origin });
    }

    return None;
  }

  if get_flag() && response.dragged() {
    return Some(Dragged);
  }

  None
}
