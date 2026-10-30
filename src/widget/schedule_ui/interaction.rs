use std::{collections::HashMap, sync::Arc};

use bimap::BiMap;
use eframe::egui::{
  self, CursorIcon, EventFilter, Key, KeyboardShortcut, Label, LayerId,
  Modifiers, Rect, Response, Sense, Ui, UiBuilder, text::LayoutJob,
};
use egui_autocomplete::AutoCompleteTextEdit;
use jiff::{Timestamp, tz::TimeZone};

use crate::{
  event::Event,
  util::{
    add_days, on_the_same_day, reorder_times, seconds_since_midnight, today,
  },
};

use super::{
  EventId, ScheduleUi, layout::Layout, move_event, move_event_end,
  move_event_start,
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
    ui.data_mut(|data| f(data.get_temp_mut_or_default(ui.scope_id())))
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
    egui::Id::unique("interacting_event")
  }

  fn get(ui: &Ui) -> Option<Self> {
    ui.data(|data| data.get_temp(Self::id()))
  }

  fn set(ui: &Ui, event: Event, state: FocusedEventState) {
    let value = InteractingEvent { event, state };
    ui.data_mut(|data| data.insert_temp(Self::id(), value));
  }

  fn save(self, ui: &Ui) {
    Self::set(ui, self.event.clone(), self.state)
  }

  fn discard(ui: &Ui) {
    ui.data_mut(|data| data.remove::<Self>(Self::id()))
  }

  fn commit(self, ui: &Ui) {
    ui.data_mut(|data| data.insert_temp(Self::id(), self.event));
    Self::discard(ui);
  }

  fn take_commited_event(ui: &Ui) -> Option<Event> {
    let event = ui.data(|data| data.get_temp(Self::id()));
    ui.data_mut(|data| data.remove::<Event>(Self::id()));
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
    egui::Id::NULL
  }

  fn request_focus(ui: &Ui, event_id: &EventId) {
    let refocusing_event = Self::new(event_id);
    ui.data_mut(|data| data.insert_temp(Self::id(ui), refocusing_event));
  }

  fn take(ui: &Ui) -> Option<Self> {
    let egui_id = Self::id(ui);
    let rfe = ui.data(|data| data.get_temp::<RefocusingEvent>(egui_id))?;
    ui.data_mut(|data| data.remove::<Self>(egui_id));

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
    egui::Id::unique("deleted_event")
  }

  fn set(ui: &Ui, event_id: &EventId) {
    let ev = Self {
      event_id: event_id.clone(),
    };
    ui.data_mut(|data| data.insert_temp(Self::id(), ev));
  }

  fn take(ui: &Ui) -> Option<EventId> {
    let deleted_event = ui.data(|data| data.get_temp(Self::id()));
    ui.data_mut(|data| data.remove::<Self>(Self::id()));
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

    let lmb = egui::PointerButton::Primary;

    let interact_pos =
      resp.interact_pointer_pos().or_else(|| resp.hover_pos())?;

    if resp.clicked_by(lmb) {
      // enter edit mode
      return Some(Editing);
    }

    if resp.drag_started_by(lmb) {
      // drag started, decide to resize or move the event
      let origin = ui.input(|i| i.pointer.press_origin())?;
      if upper.contains(origin) {
        return Some(DraggingEventStart);
      }
      if lower.contains(origin) {
        return Some(DraggingEventEnd);
      }

      let offset = DraggingEventYOffset(event_rect.top() - origin.y);
      ui.data_mut(|data| data.insert_temp(egui::Id::NULL, offset));
      if ui.input(|input| input.modifiers.ctrl) {
        return Some(EventCloning);
      } else {
        return Some(Dragging);
      }
    }

    // hovering, show the cursor icon, no action
    if upper.contains(interact_pos) || lower.contains(interact_pos) {
      ui.set_cursor_icon(CursorIcon::ResizeVertical);
    } else if event_rect.contains(interact_pos) {
      ui.set_cursor_icon(CursorIcon::Grab);
    }

    None
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
          move_event_start(
            event,
            time,
            self.min_event_duration,
            &self.timezone,
          );
          event.start
        })
      }
      FocusedEventState::DraggingEventEnd => {
        self.handle_event_resizing(ui, lower, |time| {
          move_event_end(event, time, self.min_event_duration, &self.timezone);
          event.end
        })
      }
      FocusedEventState::Dragging => {
        self.handle_event_dragging(ui, event_rect, |time| {
          move_event(event, time, &self.timezone);
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
    set_time: impl FnOnce(Timestamp) -> Timestamp,
  ) -> Option<bool> {
    if !is_drag_ongoing(ui) {
      return Some(true);
    }

    ui.set_cursor_icon(CursorIcon::ResizeVertical);

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
    set_time: impl FnOnce(Timestamp) -> (Timestamp, Timestamp),
  ) -> Option<bool> {
    if !is_drag_ongoing(ui) {
      return Some(true);
    }

    ui.output_mut(|out| out.cursor_icon = CursorIcon::Grabbing);

    let mut pointer_pos = self.relative_pointer_pos(ui).unwrap();
    if let Some(offset_y) =
      ui.data(|data| data.get_temp::<DraggingEventYOffset>(egui::Id::NULL))
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
    self.handle_keyboard_focused_event_resize(ui);
    self.handle_keyboard_focused_event_move(ui);
    self.handle_keyboard_focus_move(ui);
    self.handle_keyboard_new_event(ui);
    self.handle_keyboard_delete_event(ui);
  }

  fn key_direction_input(
    &self,
    ui: &Ui,
    modifiers: Modifiers,
  ) -> Option<Direction> {
    use Direction::*;
    let shortcut = |k| KeyboardShortcut::new(modifiers, k);
    let pressed =
      |k| ui.input_mut(|input| input.consume_shortcut(&shortcut(k)));

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
    let focused_event = ui
      .memory(|mem| mem.focused())
      .and_then(|id| EventFocusRegistry::get_event_id(ui, id))
      .and_then(|id| self.events.iter().find(|x| x.id == id))
      .map(|x| x.end);

    let ends_in_today = |end: &Timestamp| {
      let local = self.to_local(*end);
      local.date() == today && seconds_since_midnight(local) > 0
    };
    let last_event_end_in_today = self
      .events
      .iter()
      .map(|x| x.end)
      .filter(ends_in_today)
      .max();

    let last_event_end =
      self.events.iter().max_by_key(|x| x.end).map(|x| x.end);
    let nearest_snapping = {
      let t = self.snap_to_nearest(Timestamp::now());
      self.is_visible(t).then_some(t)
    };

    let new_event_start = focused_event
      .or(last_event_end_in_today)
      .or(nearest_snapping)
      .or(last_event_end)?;

    move_event(&mut event, new_event_start, &self.timezone);
    let position = event.start_position_of_day(&self.timezone);

    InteractingEvent::set(ui, event, FocusedEventState::Editing);

    self.scroll_to_vertical_position(ui, position);

    Some(())
  }

  fn handle_keyboard_delete_event(&mut self, ui: &Ui) -> Option<()> {
    if InteractingEvent::is_interacting(ui) {
      return None;
    }

    let ui_id = ui.memory(|mem| mem.focused())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, ui_id)?;

    let del_key_pressed = ui
      .input_mut(|input| input.consume_key(Modifiers::NONE, Key::X))
      || ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Delete));

    if !del_key_pressed {
      return None;
    }

    DeletedEvent::set(ui, &ev_id);

    Some(())
  }

  fn handle_keyboard_focus_move(&mut self, ui: &Ui) -> Option<()> {
    use Direction::*;

    let dir = self.key_direction_input(ui, Modifiers::NONE)?;

    let ui_id = ui.memory(|mem| mem.focused());
    let ev_id = ui_id.and_then(|id| EventFocusRegistry::get_event_id(ui, id));
    let events = self.events.as_slice();

    // focus the first event when there is no event
    let new_focus = match (ev_id, dir) {
      (None, _) => find_nearest_event(events, self.current_time?),
      (Some(ev_id), dir) => {
        find_next_focus(&ev_id, dir, events, &self.timezone)
      }
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

    let focused_id = ui.memory(|mem| mem.focused())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, focused_id)?;
    let dir = self.key_direction_input(ui, Modifiers::CTRL)?;

    let event = self.events.iter_mut().find(|x| x.id == ev_id)?;
    let tz = &self.timezone;

    match dir {
      Left => move_event(event, add_days(event.start, -1, tz), tz),
      Right => move_event(event, add_days(event.start, 1, tz), tz),
      Up => move_event(event, event.start - self.min_event_duration, tz),
      Down => move_event(event, event.start + self.min_event_duration, tz),
    }

    Some(())
  }

  fn handle_keyboard_focused_event_resize(&mut self, ui: &Ui) -> Option<()> {
    use Direction::*;

    let focused_id = ui.memory(|mem| mem.focused())?;
    let ev_id = EventFocusRegistry::get_event_id(ui, focused_id)?;
    let dir = self.key_direction_input(ui, Modifiers::SHIFT)?;

    let event = self.events.iter_mut().find(|x| x.id == ev_id)?;
    let tz = &self.timezone;
    let min = self.min_event_duration;

    match dir {
      Left => move_event_end(event, add_days(event.end, -1, tz), min, tz),
      Right => move_event_end(event, add_days(event.end, 1, tz), min, tz),
      Up => move_event_end(event, event.end - min, min, tz),
      Down => move_event_end(event, event.end + min, min, tz),
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
    disable_built_in_keyboard_focus_navigation(&resp);

    if clipped {
      // text is clipped, show a tooltip
      resp.clone().on_hover_text(event.title.clone());
    }

    self.event_context_menu(event, &resp);

    resp
  }

  fn event_context_menu(&self, event: &Event, resp: &Response) {
    let format_time = |time: Timestamp| {
      let local = self.to_local(time);
      if local.second() == 0 {
        local.strftime("%H:%M").to_string()
      } else {
        local.strftime("%H:%M:%S").to_string()
      }
    };

    resp.context_menu(|ui| {
      if let Some(desc) = &event.description {
        ui.label(desc.to_string());
      }

      ui.label(format!(
        "{}--{} ({:#})",
        format_time(event.start),
        format_time(event.end),
        event.start.duration_until(event.end)
      ));

      ui.separator();

      if ui.button("Delete").clicked() {
        DeletedEvent::set(ui, &event.id);
        ui.close();
      }

      if ui.button("Close menu").clicked() {
        ui.close();
      }
    });
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

    // first we attempt to fit the entire label within the button
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    if galley.rows.len() <= 1 {
      return (galley, false);
    }

    // if that fails, we try to fit the shrunk label with an ellipsis
    for n in (0..(label.len().saturating_sub(3))).rev() {
      let text = format!("{}..", &label[0..n]);
      let galley = ui.fonts_mut(|fonts| fonts.layout_job(layout_job(text)));
      if galley.rows.len() <= 1 {
        return (galley, true);
      }
    }

    // return the original label if we can't a fit
    (galley, false)
  }

  fn auto_suggest_event_titles(&self) -> Vec<String> {
    let mut titles = self
      .events
      .iter()
      .map(|x| x.title.clone())
      .collect::<Vec<_>>();
    titles.sort();
    titles.dedup();

    titles
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
    // let text_edit = egui::TextEdit::singleline(&mut event.title);

    let popup_id = egui::Id::unique("event_title_completion");
    let candidates = self.auto_suggest_event_titles();
    let text_edit = AutoCompleteTextEdit::new(&mut event.title, candidates)
      .highlight_matches(true)
      .max_suggestions(5);

    let resp = ui.put(rect, text_edit);

    // Anything dragging outside the textedit should be equivalent to
    // losing focus. Note: we still need to allow dragging within the
    // textedit widget to allow text selection, etc.
    let anything_else_dragging =
      ui.dragged_id().is_some() && !resp.dragged() && !resp.drag_stopped();

    // We cannot use key_released here, because it will be taken
    // precedence by resp.lost_focus() and commit the change.
    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
      return Some(false);
    }

    if resp.lost_focus() || resp.clicked_elsewhere() || anything_else_dragging {
      return Some(true);
    }

    // request_focus interrupts the IME, which breaks key repeat
    if !resp.has_focus() {
      resp.request_focus();
    }
    disable_built_in_keyboard_focus_navigation(&resp);
    None
  }

  fn show_resizer_hint(&self, ui: &mut Ui, rect: Rect, time: Timestamp) {
    let layer_id = egui::Id::unique("resizer_hint");
    let layer = LayerId::new(egui::Order::Tooltip, layer_id);

    let local = self.to_local(time);
    let text = local.strftime(self.event_resizing_hint_format).to_string();
    let bg = ui.style().visuals.widgets.noninteractive.bg_fill;
    let rich_text = egui::RichText::new(text)
      .monospace()
      .size(10.0)
      .background_color(bg);
    let label = Label::new(rich_text);

    let builder = UiBuilder::new().layer_id(layer);
    let mut child_ui = ui.new_child(builder);
    child_ui.put(rect, label);
  }

  pub(super) fn handle_new_event(
    &self,
    ui: &mut Ui,
    response: &Response,
  ) -> Option<()> {
    use FocusedEventState::Editing;

    let lmb = egui::PointerButton::Primary;
    let id = response.id;

    if response.clicked_by(lmb) {
      // just a click on blank space, no new event
      InteractingEvent::discard(ui);
      return Some(());
    }

    if response.drag_started_by(lmb) {
      // drag started, create new event and track its state
      let mut event = self.new_event();
      let pointer_pos = self.relative_pointer_pos(ui)?;
      let init_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;
      let new_state = self.assign_new_event_dates(ui, init_time, &mut event)?;

      ui.data_mut(|data| data.insert_temp(id, event.id.clone()));
      ui.data_mut(|data| data.insert_temp(id, init_time));

      InteractingEvent::set(ui, event, new_state);

      return Some(());
    }

    if response.drag_stopped() {
      // drag stopped, save the event
      let event_id = ui.data(|data| data.get_temp(id))?;
      let mut value = InteractingEvent::get_id(ui, &event_id)?;
      value.state = Editing;
      value.save(ui);
      return Some(());
    }

    if response.dragged_by(lmb) {
      // dragging, update the event's timestamps
      let event_id: String = ui.data(|data| data.get_temp(id))?;
      let init_time = ui.data(|data| data.get_temp(id))?;
      let mut value = InteractingEvent::get_id(ui, &event_id)?;
      let new_state =
        self.assign_new_event_dates(ui, init_time, &mut value.event)?;
      value.state = new_state;
      value.save(ui);
      return Some(());
    }

    None
  }

  fn assign_new_event_dates(
    &self,
    ui: &Ui,
    init_time: Timestamp,
    event: &mut Event,
  ) -> Option<FocusedEventState> {
    use FocusedEventState::{DraggingEventEnd, DraggingEventStart};

    let pointer_pos = self.relative_pointer_pos(ui)?;
    let new_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;

    let (mut start, mut end) = (init_time, new_time);
    let reordered = reorder_times(&mut start, &mut end);

    // the event crossed the day boundary, we need to pick a direction
    // based on the initial drag position
    if !on_the_same_day(start, end, &self.timezone) {
      if self.day_progress(init_time) < 0.5 {
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
      if event.title.is_empty() {
        DeletedEvent::set(ui, &event.id);
      } else {
        RefocusingEvent::request_focus(ui, &event.id);

        let change = Change::new_changed(&self.events, event);
        change.apply(&mut self.events);
        self.history.save(change);
      }
    }

    // commit deleted event
    if let Some(event_id) = DeletedEvent::take(ui)
      && let Some(change) = Change::new_removed(&self.events, &event_id)
    {
      change.apply(&mut self.events);
      self.history.save(change);
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

fn find_nearest_event(events: &[Event], now: Timestamp) -> Option<EventId> {
  let now_ts = now.as_second();

  events
    .iter()
    .min_by_key(|e| e.start.as_second().abs_diff(now_ts))
    .map(|x| x.id.clone())
}

fn find_next_focus(
  event_id: &EventId,
  dir: Direction,
  events: &[Event],
  tz: &TimeZone,
) -> Option<EventId> {
  use Direction::*;

  match dir {
    Left => find_juxtaposed_event(event_id, -1, events, tz),
    Right => find_juxtaposed_event(event_id, 1, events, tz),
    Up => find_adjacent_event(event_id, -1, events),
    Down => find_adjacent_event(event_id, 1, events),
  }
}

fn find_juxtaposed_event(
  event_id: &EventId,
  offset: isize,
  events: &[Event],
  tz: &TimeZone,
) -> Option<EventId> {
  let ev = events.iter().find(|x| &x.id == event_id)?;

  let t = add_days(ev.start, offset as i64, tz);
  let dist = |e: &&Event| e.start.as_second().abs_diff(t.as_second());
  let candidate_ev = events.iter().min_by_key(dist)?;

  // only consider the move successful if it actually moved a day.
  let start_date = |e: &Event| tz.to_datetime(e.start).date();
  if candidate_ev.id != ev.id && start_date(candidate_ev) != start_date(ev) {
    return Some(candidate_ev.id.clone());
  }

  None
}

fn find_adjacent_event(
  event_id: &EventId,
  offset: isize,
  events: &[Event],
) -> Option<EventId> {
  let i = events.iter().position(|x| x.id == *event_id)?;

  let new_i = i as isize + offset;
  if new_i < 0 || new_i >= events.len() as isize {
    return None;
  }

  Some(events[new_i as usize].id.clone())
}

fn is_drag_ongoing(ui: &Ui) -> bool {
  let dragging = ui.dragged_id().is_some();
  let just_stopped = ui.ctx().drag_stopped_id().is_some();
  dragging || just_stopped
}

fn disable_built_in_keyboard_focus_navigation(resp: &Response) {
  // avoid built-in arrow navigation
  let event_filter = EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    ..Default::default()
  };

  resp
    .ctx
    .memory_mut(|mem| mem.set_focus_lock_filter(resp.id, event_filter));
}
