use std::{collections::HashMap, sync::Arc};

use bimap::BiMap;
use chrono::{Duration, Timelike};
use eframe::egui::{
  self, text::LayoutJob, CursorIcon, Key, Label, LayerId, Modifiers, Rect,
  Response, Sense, Ui,
};
use humantime;

use crate::{
  event::Event,
  util::{local_now, on_the_same_day, reorder_times, today, DateTime},
};

use super::{
  layout::Layout, move_event, move_event_end, move_event_start, EventId,
  ScheduleUi,
};

#[derive(Clone, Copy, Debug)]
enum Direction {
  Left,
  Right,
  Up,
  Down,
}

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
  events: BiMap<EventId, egui::Id>,
  event_rects: HashMap<EventId, Rect>,
}

impl EventFocusRegistry {
  fn with_this<R>(ui: &Ui, f: impl FnOnce(&mut Self) -> R) -> R {
    ui.memory_mut(|mem| {
      let this: &mut Self = mem.data.get_temp_mut_or_default(ui.id());
      f(this)
    })
  }

  fn register(ui: &Ui, event_id: &EventId, resp: &Response) {
    Self::with_this(ui, |this| {
      this.events.insert(event_id.clone(), resp.id);
      this.event_rects.insert(event_id.clone(), resp.rect);
    })
  }

  fn get_event_id(ui: &Ui, ui_id: egui::Id) -> Option<EventId> {
    Self::with_this(ui, |this| this.events.get_by_right(&ui_id).cloned())
  }

  fn get_ui_id(ui: &Ui, event_id: &EventId) -> Option<egui::Id> {
    Self::with_this(ui, |this| this.events.get_by_left(event_id).copied())
  }

  fn get_event_rect(ui: &Ui, event_id: &EventId) -> Option<Rect> {
    Self::with_this(ui, |this| this.event_rects.get(event_id).copied())
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
    ui.memory(|mem| mem.data.get_temp(Self::id()))
  }

  fn set(ui: &Ui, event: Event, state: FocusedEventState) {
    let value = InteractingEvent { event, state };
    ui.memory_mut(|mem| mem.data.insert_temp(Self::id(), value))
  }

  fn save(self, ui: &Ui) {
    Self::set(ui, self.event.clone(), self.state)
  }

  fn discard(ui: &Ui) {
    ui.memory_mut(|mem| mem.data.remove::<Self>(Self::id()))
  }

  fn commit(self, ui: &Ui) {
    ui.memory_mut(|mem| mem.data.insert_temp(Self::id(), self.event));
    Self::discard(ui);
  }

  fn take_commited_event(ui: &Ui) -> Option<Event> {
    let event = ui.memory(|mem| mem.data.get_temp(Self::id()));
    ui.memory_mut(|mem| mem.data.remove::<Event>(Self::id()));
    event
  }

  fn get_id(ui: &Ui, id: &EventId) -> Option<Self> {
    Self::get(ui).and_then(|value| (&value.event.id == id).then_some(value))
  }

  fn get_event(ui: &Ui) -> Option<Event> {
    Self::get(ui).map(|v| v.event)
  }

  fn is_interacting(ui: &Ui) -> bool {
    InteractingEvent::get(ui).is_some()
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
    ui.memory_mut(|mem| mem.data.insert_temp(Self::id(ui), refocusing_event));
  }

  fn take(ui: &Ui) -> Option<Self> {
    let egui_id = Self::id(ui);
    let rfe = ui.memory(|mem| mem.data.get_temp::<RefocusingEvent>(egui_id))?;
    ui.memory_mut(|mem| mem.data.remove::<Self>(egui_id));

    Some(rfe)
  }

  fn apply_focus(ui: &Ui) {
    let rfe = match Self::take(ui) {
      Some(x) => x,
      None => return,
    };

    let event_id = rfe.0.as_ref();
    if let Some(ui_id) = EventFocusRegistry::get_ui_id(ui, event_id) {
      ui.memory_mut(|mem| mem.request_focus(ui_id));
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
    ui.memory_mut(|mem| {
      mem.data.insert_temp(
        Self::id(),
        Self {
          event_id: event_id.clone(),
        },
      )
    });
  }

  fn take(ui: &Ui) -> Option<EventId> {
    let deleted_event = ui.memory(|mem| mem.data.get_temp(Self::id()));
    ui.memory_mut(|mem| mem.data.remove::<Self>(Self::id()));
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
    if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Enter)) {
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
          ui.output_mut(|out| out.cursor_icon = CursorIcon::ResizeVertical);
        } else if event_rect.contains(interact_pos) {
          ui.output_mut(|out| out.cursor_icon = CursorIcon::Grab);
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
        ui.memory_mut(|mem| mem.data.insert_temp(egui::Id::null(), offset));
        if ui.input(|input| input.modifiers.ctrl) {
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
    if !ui.memory(|mem| mem.is_anything_being_dragged()) {
      return Some(true);
    }

    ui.output_mut(|out| out.cursor_icon = CursorIcon::ResizeVertical);

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
    if !ui.memory(|mem| mem.is_anything_being_dragged()) {
      return Some(true);
    }

    ui.output_mut(|out| out.cursor_icon = CursorIcon::Grabbing);

    let mut pointer_pos = self.relative_pointer_pos(ui).unwrap();
    if let Some(offset_y) = ui
      .memory(|mem| mem.data.get_temp::<DraggingEventYOffset>(egui::Id::null()))
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

    EventFocusRegistry::register(ui, &event.id, &resp);

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

  pub(super) fn handle_hotkeys(&mut self, ui: &Ui) {
    self.handle_keyboard_focus_move(ui);
    self.handle_keyboard_focused_event_move(ui);
    self.handle_keyboard_focused_event_resize(ui);
    self.handle_keyboard_new_event(ui);
    self.handle_keyboard_delete_event(ui);
  }

  fn key_direction_input(
    &self,
    ui: &Ui,
    modifiers: Modifiers,
  ) -> Option<Direction> {
    use Direction::*;
    let pressed = |k| ui.input_mut(|input| input.consume_key(modifiers, k));

    // do not interrupt interacting events
    if InteractingEvent::is_interacting(ui) {
      return None;
    }

    if pressed(Key::J) || pressed(Key::ArrowDown) {
      Some(Down)
    } else if pressed(Key::K) || pressed(Key::ArrowUp) {
      Some(Up)
    } else if pressed(Key::H) || pressed(Key::ArrowLeft) {
      Some(Left)
    } else if pressed(Key::L) || pressed(Key::ArrowRight) {
      Some(Right)
    } else {
      None
    }
  }

  fn handle_keyboard_new_event(&mut self, ui: &Ui) -> Option<()> {
    if InteractingEvent::is_interacting(ui) {
      return None;
    }

    if !ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::N)) {
      return None;
    }

    let mut event = self.new_event();
    let today = today(&self.timezone);
    let last_event_end_in_today = self
      .events
      .iter()
      .filter(|x| x.end.date_naive() == today)
      .filter(|x| x.end.num_seconds_from_midnight() > 0)
      .max_by_key(|x| x.end)
      .map(|x| x.end);

    let last_event_end =
      self.events.iter().max_by_key(|x| x.end).map(|x| x.end);
    let nearest_snapping = {
      let t = self.snap_to_nearest(&local_now());
      self.is_visible(&t).then_some(t)
    };

    let new_event_start = last_event_end_in_today
      .or(nearest_snapping)
      .or(last_event_end)?;

    move_event(&mut event, new_event_start);
    let position = event.start_position_of_day();

    InteractingEvent::set(ui, event, FocusedEventState::Editing);

    self.scroll_to_vertical_position(ui, position);

    Some(())
  }

  fn handle_keyboard_delete_event(&mut self, ui: &Ui) -> Option<()> {
    if InteractingEvent::is_interacting(ui) {
      return None;
    }

    let ui_id = ui.memory(|mem| mem.focus())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, ui_id)?;

    let del_key_pressed = ui
      .input_mut(|mem| mem.consume_key(Modifiers::NONE, Key::X))
      || ui.input_mut(|mem| mem.consume_key(Modifiers::NONE, Key::Delete));

    if !del_key_pressed {
      return None;
    }

    DeletedEvent::set(ui, &ev_id);

    Some(())
  }

  fn handle_keyboard_focus_move(&mut self, ui: &Ui) -> Option<()> {
    use Direction::*;

    let dir = self.key_direction_input(ui, Modifiers::NONE)?;

    let ui_id = ui.memory(|mem| mem.focus());
    let ev_id = ui_id.and_then(|id| EventFocusRegistry::get_event_id(ui, id));
    let events = self.events.as_slice();

    // focus the first event when there is no event
    let new_focus = match (ev_id, dir) {
      (None, _) => find_nearest_event(events, &self.current_time?),
      (Some(ev_id), dir) => find_next_focus(&ev_id, dir, events),
    };

    if let Some(new_ev_id) = new_focus {
      RefocusingEvent::request_focus(ui, &new_ev_id);
      self.scroll_event_into_view(ui, &new_ev_id);
    } else {
      match dir {
        Left => self.scroll_horizontally(-1),
        Right => self.scroll_horizontally(1),
        _ => (),
      }
    }

    Some(())
  }

  fn handle_keyboard_focused_event_move(&mut self, ui: &Ui) -> Option<()> {
    use Direction::*;

    let focused_id = ui.memory(|mem| mem.focus())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, focused_id)?;
    let dir = self.key_direction_input(ui, Modifiers::CTRL)?;

    let event = self.events.iter_mut().find(|x| x.id == ev_id)?;

    match dir {
      Left => super::move_event(event, event.start + Duration::days(-1)),
      Right => super::move_event(event, event.start + Duration::days(1)),
      Up => super::move_event(event, event.start - self.min_event_duration),
      Down => super::move_event(event, event.start + self.min_event_duration),
    }

    Some(())
  }

  fn handle_keyboard_focused_event_resize(&mut self, ui: &Ui) -> Option<()> {
    use Direction::*;

    let focused_id = ui.memory(|mem| mem.focus())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, focused_id)?;
    let dir = self.key_direction_input(ui, Modifiers::SHIFT)?;

    let event = self.events.iter_mut().find(|x| x.id == ev_id)?;

    match dir {
      Left => super::move_event_end(
        event,
        event.end + Duration::days(-1),
        self.min_event_duration,
      ),
      Right => super::move_event_end(
        event,
        event.end + Duration::days(1),
        self.min_event_duration,
      ),
      Up => super::move_event_end(
        event,
        event.end - self.min_event_duration,
        self.min_event_duration,
      ),
      Down => super::move_event_end(
        event,
        event.end + self.min_event_duration,
        self.min_event_duration,
      ),
    }

    Some(())
  }

  fn scroll_event_into_view(&mut self, ui: &Ui, event_id: &EventId) {
    let rect = match EventFocusRegistry::get_event_rect(ui, event_id) {
      Some(rect) => rect,
      _ => return,
    };

    ui.scroll_to_rect(rect, Some(eframe::emath::Align::Center));
  }

  fn scroll_to_vertical_position(&mut self, ui: &Ui, position: f32) {
    let mut rect = ui.max_rect();
    rect.set_width(1.0);
    rect.set_top(rect.top() + position * rect.height());
    rect.set_height(1.0);
    ui.scroll_to_rect(rect, Some(eframe::emath::Align::Center));
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
    let line_height = ui.fonts(|fonts| job.font_height(fonts));
    let mut galley = ui.fonts(|fonts| fonts.layout_job(job));

    if galley.size().y <= line_height {
      // multiline
      return (galley, false);
    }

    for n in (0..(label.len() - 3)).rev() {
      let text = format!("{}..", &label[0..n]);
      galley = ui.fonts(|fonts| fonts.layout_job(layout_job(text)));
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
    let anything_else_dragging = ui
      .memory(|mem| mem.is_anything_being_dragged())
      && !resp.dragged()
      && !resp.drag_released();

    // We cannot use key_released here, because it will be taken
    // precedence by resp.lost_focus() and commit the change.
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
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

        ui.memory_mut(|mem| mem.data.insert_temp(id, event.id.clone()));
        ui.memory_mut(|mem| mem.data.insert_temp(id, init_time));

        InteractingEvent::set(ui, event, new_state);

        return Some(());
      }
      Some(Interaction::DragReleased) => {
        let event_id = ui.memory(|mem| mem.data.get_temp(id))?;
        let mut value = InteractingEvent::get_id(ui, &event_id)?;
        value.state = Editing;
        value.save(ui);
      }
      Some(Interaction::Dragged)
        if response.dragged_by(egui::PointerButton::Primary) =>
      {
        let event_id: String = ui.memory(|mem| mem.data.get_temp(id))?;
        let init_time = ui.memory(|mem| mem.data.get_temp(id))?;
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
    let ctrl_z =
      ui.input_mut(|input| input.consume_key(Modifiers::CTRL, egui::Key::Z));

    if !ctrl_z {
      return;
    }

    if let Some(change) = self.history.pop() {
      change.reverse().apply(&mut self.events)
    }
  }
}

fn find_nearest_event(events: &[Event], now: &DateTime) -> Option<EventId> {
  let now_ts = now.timestamp();

  events
    .iter()
    .min_by_key(|e| e.start.timestamp().abs_diff(now_ts))
    .map(|x| x.id.clone())
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

  let pointer = response.ctx.input(|input| input.pointer.clone());

  let set_flag = |value| {
    response.ctx.memory_mut(|mem| {
      mem
        .data
        .get_temp_mut_or(response.id, DetectionFinishFlag(false))
        .0 = value
    });
  };

  let get_flag = || {
    response.ctx.memory_mut(|mem| {
      mem
        .data
        .get_temp_mut_or(response.id, DetectionFinishFlag(false))
        .0
    })
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

    let dt = response.ctx.input(|input| input.time)
      - pointer.press_start_time().unwrap();
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

fn find_next_focus(
  event_id: &EventId,
  dir: Direction,
  events: &[Event],
) -> Option<EventId> {
  use Direction::*;

  match dir {
    Left => find_juxtaposed_event(event_id, -1, events),
    Right => find_juxtaposed_event(event_id, 1, events),
    Up => find_adjacent_event(event_id, -1, events),
    Down => find_adjacent_event(event_id, 1, events),
  }
}

fn find_juxtaposed_event(
  event_id: &EventId,
  offset: isize,
  events: &[Event],
) -> Option<EventId> {
  let ev = match events.iter().find(|x| &x.id == event_id) {
    Some(ev) => ev,
    None => return None,
  };

  let t = ev.start + Duration::days(offset as i64);
  let dist = |e: &&Event| e.start.timestamp().abs_diff(t.timestamp());
  let candidate_ev = match events.iter().min_by_key(dist) {
    Some(ev) => ev,
    None => return None,
  };

  // only consider the move successful if it actually moved a day.
  if candidate_ev.id != ev.id
    && candidate_ev.start.date_naive() != ev.start.date_naive()
  {
    return Some(candidate_ev.id.clone());
  }

  None
}

fn find_adjacent_event(
  event_id: &EventId,
  offset: isize,
  events: &[Event],
) -> Option<EventId> {
  let i = match events.iter().position(|x| x.id == *event_id) {
    Some(i) => i,
    None => return None,
  };

  let new_i = i as isize + offset;
  if new_i < 0 || new_i >= events.len() as isize {
    return None;
  }

  Some(events[new_i as usize].id.clone())
}
