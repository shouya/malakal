mod layout;

use chrono::{Duration, FixedOffset, NaiveTime, Timelike};
use derive_builder::Builder;
use eframe::egui::{
  self, pos2, text::LayoutJob, vec2, Color32, CursorIcon, Label, LayerId, Pos2,
  Rect, Response, Sense, Ui, Vec2,
};
use uuid::Uuid;

use layout::{Layout, LayoutAlgorithm};

use crate::{
  event::{Event, EventBuilder},
  util::{now, today, Date, DateTime},
};

pub(crate) struct ScheduleUiState {
  pub events: Vec<Event>,
  pub scope_updated: bool,
  pub refresh_requested: bool,
  pub day_count: usize,
  pub first_day: Date,
}

#[derive(Builder, Clone, Debug, PartialEq)]
#[builder(try_setter, setter(into))]
pub struct ScheduleUi {
  #[builder(default = "3")]
  day_count: usize,
  #[builder(default = "260.0")]
  day_width: f32,
  #[builder(default = "24")]
  segment_count: usize,
  #[builder(default = "80.0")]
  segment_height: f32,
  #[builder(default = "80.0")]
  time_marker_margin_width: f32,
  #[builder(default = "60.0")]
  day_header_margin_height: f32,
  #[builder(default = "\"%H:%M\"")]
  time_marker_format: &'static str,
  #[builder(default = "\"%F %a\"")]
  day_header_format: &'static str,

  first_day: Date,

  // a small margin on the right of day columns reserved for creating
  // new events
  #[builder(default = "20.0")]
  new_event_margin: f32,

  // used to render current time indicator
  current_time: Option<DateTime>,

  // used to refresh every second
  #[builder(default = "std::time::Instant::now()", setter(skip))]
  last_update: std::time::Instant,

  #[builder(default = "5.0")]
  resizer_height: f32,
  #[builder(default = "20.0")]
  resizer_width_margin: f32,

  #[builder(default = "Duration::minutes(15)")]
  min_event_duration: Duration,

  #[builder(default = "Duration::minutes(15)")]
  snapping_duration: Duration,

  #[builder(default = "\"%H:%M\"")]
  event_resizing_hint_format: &'static str,

  #[builder(default = "Color32::LIGHT_BLUE")]
  new_event_color: Color32,

  timezone: FixedOffset,

  new_event_calendar: String,
}

type EventId = String;

#[derive(Clone, Copy, Debug)]
struct DraggingEventYOffset(f32);

#[derive(Debug)]
enum EventLayoutType {
  Single(Date, [f32; 2]),
  #[allow(unused)]
  AllDay([Date; 2]),
  #[allow(unused)]
  Multi([DateTime; 2]),
}

const SECS_PER_DAY: u64 = 24 * 3600;

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
    debug_assert!(Self::get(ui).is_some());

    ui.memory().data.remove::<Self>(Self::id())
  }

  fn commit(self, ui: &Ui) {
    ui.memory().data.insert_temp(Self::id(), self.event);
    Self::discard(ui);
  }

  fn get_commited_event(ui: &Ui) -> Option<Event> {
    let event = ui.memory().data.get_temp(Self::id());
    ui.memory().data.remove::<Event>(Self::id());
    event
  }

  fn get_id(ui: &Ui, id: &EventId) -> Option<Self> {
    Self::get(ui).and_then(|value| (&value.event.id == id).then(|| value))
  }

  fn get_event(ui: &Ui) -> Option<Event> {
    Self::get(ui).map(|v| v.event)
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

// can't be a constant because chrono::Duration constructors are not
// declared as const functions.
fn one_day() -> Duration {
  Duration::days(1)
}

impl ScheduleUi {
  // the caller must ensure the events are all within the correct days
  fn layout_events(
    &self,
    events: &[Event],
    interacting: &Option<Event>,
  ) -> Layout {
    let mut layout = Layout::default();
    let mut events = events.to_vec();

    if let Some(ev) = interacting.clone().take() {
      events.retain(|x| x.id != ev.id);
      events.insert(0, ev);
    }

    for day in 0..self.day_count {
      // layout for each day
      let events: Vec<layout::Ev> = events
        .iter()
        .filter(|&e| !e.deleted)
        .filter(|&e| self.date_to_day(e.start.date()) == Some(day))
        .filter(|&e| matches!(self.layout_type(e), EventLayoutType::Single(..)))
        .map(|e| {
          if e.end - e.start < self.min_event_duration {
            let end = e.start + self.min_event_duration;
            (&e.id, e.start.timestamp(), end.timestamp()).into()
          } else {
            (&e.id, e.start.timestamp(), e.end.timestamp()).into()
          }
        })
        .collect();

      layout.merge(layout::MarkusAlgorithm::compute(events))
    }

    layout
  }

  fn event_rect(
    &self,
    ui: &Ui,
    layout: &Layout,
    event: &Event,
  ) -> Option<Rect> {
    let widget_rect = ui.max_rect();
    match self.layout_type(event) {
      EventLayoutType::Single(date, y) => {
        let rel_x = layout.query(&event.id)?;
        let day = self.date_to_day(date)?;
        let rect = self.layout_event(widget_rect, day, y, rel_x);
        let margin = ui.style().visuals.clip_rect_margin / 2.0;

        Some(rect.shrink(margin))
      }
      _ => unimplemented!(),
    }
  }

  fn layout_event(
    &self,
    widget_rect: Rect,
    day: usize,
    y: [f32; 2],
    x: [f32; 2],
  ) -> Rect {
    let mut rect = self.day_column(day);

    // leave a margin for creating new events
    rect.set_right(rect.right() - self.new_event_margin);

    let w = rect.width();
    let h = rect.height();

    rect.set_right(rect.left() + x[1] * w);
    rect.set_left(rect.left() + x[0] * w);

    rect.set_bottom(rect.top() + y[1] * h);
    rect.set_top(rect.top() + y[0] * h);

    rect.translate(self.content_offset(widget_rect))
  }

  fn interact_event_region(
    &self,
    ui: &mut Ui,
    resp: Response,
  ) -> Option<FocusedEventState> {
    use FocusedEventState::*;
    let event_rect = resp.rect;
    let [upper, lower] = self.event_resizer_regions(event_rect);

    let _lmb = egui::PointerButton::Primary;

    let interact_pos =
      resp.interact_pointer_pos().or_else(|| resp.hover_pos())?;

    if resp.clicked_by(egui::PointerButton::Primary) {
      return Some(Editing);
    }

    if upper.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::ResizeVertical;
      if resp.drag_started() && resp.dragged_by(egui::PointerButton::Primary) {
        return Some(DraggingEventStart);
      }
      return None;
    }

    if lower.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::ResizeVertical;
      if resp.drag_started() && resp.dragged_by(egui::PointerButton::Primary) {
        return Some(DraggingEventEnd);
      }
      return None;
    }

    if event_rect.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::Grab;

      if resp.drag_started()
        && resp.dragged_by(egui::PointerButton::Primary)
        && ui.input().modifiers.ctrl
      {
        let offset = DraggingEventYOffset(event_rect.top() - interact_pos.y);
        ui.memory().data.insert_temp(egui::Id::null(), offset);
        return Some(EventCloning);
      }

      if resp.drag_started() && resp.dragged_by(egui::PointerButton::Primary) {
        let offset = DraggingEventYOffset(event_rect.top() - interact_pos.y);
        ui.memory().data.insert_temp(egui::Id::null(), offset);
        return Some(Dragging);
      }

      return None;
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
          self.move_event_start(event, time);
          event.start
        })
      }
      FocusedEventState::DraggingEventEnd => {
        self.handle_event_resizing(ui, lower, |time| {
          self.move_event_end(event, time);
          event.end
        })
      }
      FocusedEventState::Dragging => {
        self.handle_event_dragging(ui, event_rect, |time| {
          self.move_event(event, time);
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

  fn put_non_interacting_event_block(
    &self,
    ui: &mut Ui,
    layout: &Layout,
    event: &mut Event,
  ) -> Option<Response> {
    let event_rect = self.event_rect(ui, layout, event)?;

    let resp = self.place_event_button(ui, event_rect, event);
    match self.interact_event_region(ui, resp) {
      None => (),
      Some(FocusedEventState::EventCloning) => {
        let new_event = self.clone_to_new_event(event);
        InteractingEvent::set(ui, new_event, FocusedEventState::Dragging);
      }
      Some(state) => InteractingEvent::set(ui, event.clone(), state),
    }

    None
  }

  fn put_interacting_event_block(
    &self,
    ui: &mut Ui,
    layout: &Layout,
  ) -> Option<Response> {
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

        let (resp, commit) =
          self.interact_event(ui, event_rect, ie.state, &mut ie.event);

        match commit {
          None => {
            // two possibilities:
            // 1. a brief click
            // 2. really dragging something
            if let Some(new_state) = self.interact_event_region(ui, resp) {
              ie.state = state_override(ie.state, new_state);
            }
            ie.save(ui)
          }
          Some(true) => ie.commit(ui),
          Some(false) => InteractingEvent::discard(ui),
        }
      }
    }

    None
  }

  fn place_event_button(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &mut Event,
  ) -> Response {
    let (layout, clipped) = self.shorten_event_label(ui, rect, &event.title);

    let button = egui::Button::new(layout).sense(Sense::click_and_drag());
    let resp = ui.put(rect, button);

    if clipped {
      // text is clipped, show a tooltip
      resp.clone().on_hover_text(event.title.clone());
    }

    resp.clone().context_menu(|ui| {
      if ui.button("Delete").clicked() {
        event.deleted = true;
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
    let text_style = egui::TextStyle::Button;
    let color = ui.visuals().text_color();

    let layout_job = |text| {
      let mut j = LayoutJob::simple_singleline(text, text_style, color);
      j.wrap_width = rect.shrink2(ui.spacing().button_padding).width();
      j
    };

    let job = layout_job(label.into());
    let line_height = job.font_height(ui.fonts());
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

  // These move_event_{start,end} functions aim to enforce few constraints:
  //
  // 1. event end must be later than event start
  // 2. event duration must be at least self.min_event_duration long
  // 3. event date can't be changed
  fn move_event_start(&self, event: &mut Event, new_start: DateTime) {
    if event.end < new_start + self.min_event_duration {
      return;
    }

    if !on_the_same_day(new_start, event.end) {
      return;
    }

    if event.start != new_start {
      event.mark_changed();
      event.start = new_start;
    }
  }

  fn move_event_end(&self, event: &mut Event, new_end: DateTime) {
    if new_end < event.start + self.min_event_duration {
      return;
    }

    if !on_the_same_day(event.start, new_end) {
      return;
    }

    if event.end != new_end {
      event.mark_changed();
      event.end = new_end;
    }
  }

  fn move_event(&self, event: &mut Event, new_start: DateTime) {
    let duration = event.end - event.start;
    let new_end = new_start + duration;

    if !on_the_same_day(new_start, new_end) {
      return;
    }

    if event.start != new_start || event.end != new_end {
      event.mark_changed();
      event.start = new_start;
      event.end = new_end;
    }
  }

  fn pointer_pos_to_datetime(&self, rel_pos: Pos2) -> Option<DateTime> {
    let day = (rel_pos.x / self.day_width) as i64;
    if !(day >= 0 && day < self.day_count as i64) {
      return None;
    }

    let vert_pos = rel_pos.y / self.content_height();
    if !(vert_pos > 0.0 && vert_pos < 1.0) {
      return None;
    }

    let seconds = (SECS_PER_DAY as f32 * vert_pos) as i64;

    let date = self.first_day + Duration::days(day);
    let time = date.and_hms(0, 0, 0) + Duration::seconds(seconds);
    Some(time)
  }

  fn pointer_pos_to_datetime_snapping(
    &self,
    rel_pos: Pos2,
  ) -> Option<DateTime> {
    let day = (rel_pos.x / self.day_width) as i64;
    if !(day >= 0 && day < self.day_count as i64) {
      return None;
    }

    let vert_pos = rel_pos.y / self.content_height();
    if vert_pos < 0.0 {
      // Note: we must allow vert_pos to exceed 1.0, otherwise we
      // can't snap to the end of day.
      return None;
    }

    let seconds = SECS_PER_DAY as f32 * vert_pos;
    let mut snapped_seconds =
      (seconds / self.snapping_duration.num_seconds() as f32).floor() as i64
        * self.snapping_duration.num_seconds() as i64;

    if snapped_seconds > SECS_PER_DAY as i64 {
      snapped_seconds = SECS_PER_DAY as i64;
    }

    let date = self.first_day + Duration::days(day);
    let time = date.and_hms(0, 0, 0) + Duration::seconds(snapped_seconds);
    Some(time)
  }

  fn event_resizer_regions(&self, rect: Rect) -> [Rect; 2] {
    let mut upper_resizer = rect.shrink2(vec2(self.resizer_width_margin, 0.0));
    upper_resizer.set_height(self.resizer_height);

    let mut lower_resizer = rect.shrink2(vec2(self.resizer_width_margin, 0.0));
    lower_resizer.set_top(rect.bottom() - self.resizer_height);

    if upper_resizer.intersects(lower_resizer) {
      // overlaps, then we keep only the lower resizer
      upper_resizer.set_height(0.0);
    }

    [upper_resizer, lower_resizer]
  }

  fn date_to_day(&self, date: Date) -> Option<usize> {
    let diff_days = (date - self.first_day).num_days();
    if diff_days < 0 || diff_days >= self.day_count as i64 {
      return None;
    }

    Some(diff_days as usize)
  }

  fn draw_ticks(&self, ui: &mut Ui, rect: Rect) {
    self.draw_grid(ui, rect);
  }

  fn draw_grid(&self, ui: &mut Ui, rect: Rect) {
    let widget_visuals = ui.style().noninteractive();

    let offset = self.content_offset(rect);
    let painter = ui.painter_at(rect);

    // vertical lines
    for day in 0..=self.day_count {
      let x = self.day_width * day as f32;
      let y0 = 0.0;
      let y1 = self.segment_height * self.segment_count as f32;
      let ends = [pos2(x, y0) + offset, pos2(x, y1) + offset];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // horizontal lines
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x0 = 0.0;
      let x1 = self.day_width * self.day_count as f32;
      let ends = [pos2(x0, y) + offset, pos2(x1, y) + offset];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }
  }

  fn draw_current_time_indicator(&self, ui: &mut Ui, rect: Rect, alpha: f32) {
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);
    let offset = rect.left_top().to_vec2();

    if let Some(now) = self.current_time.as_ref() {
      let y = self.day_progress(now) * rect.height();
      let x0 = 0.0;
      let x1 = rect.width();

      let p0 = pos2(x0, y) + offset;
      let p1 = pos2(x1, y) + offset;
      let mut indicator_stroke = widget_visuals.bg_stroke;
      indicator_stroke.color = Color32::RED.linear_multiply(alpha);
      painter.line_segment([p0, p1], indicator_stroke);
    }
  }

  fn draw_time_marks(&self, ui: &mut Ui, rect: Rect) {
    let offset = self.content_offset(rect);

    let visuals = ui.style().visuals.clone();
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);

    let mut time_mark_region = Rect::from_min_size(
      rect.left_top() + vec2(0.0, self.day_header_margin_height),
      vec2(
        self.time_marker_margin_width,
        self.segment_height * self.segment_count as f32,
      ),
    );

    let mut alpha = 1.0;

    if time_mark_region.center().x <= ui.clip_rect().left() {
      // floating time mark region
      time_mark_region.set_left(ui.clip_rect().left());
      time_mark_region.set_width(self.time_marker_margin_width);

      alpha = ui.ctx().animate_bool(
        egui::Id::new("time_mark"),
        !ui.rect_contains_pointer(time_mark_region),
      );
    }

    painter.rect_filled(
      time_mark_region.shrink(visuals.clip_rect_margin),
      widget_visuals.corner_radius,
      widget_visuals.bg_fill.linear_multiply(alpha * 0.8),
    );

    for seg in 0..=self.segment_count {
      let y = offset.y + seg as f32 * self.segment_height;
      let x = time_mark_region.center().x;

      let text = self.time_marker_text(seg).expect("segment out of bound");
      painter.text(
        pos2(x, y),
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace,
        widget_visuals.text_color().linear_multiply(alpha),
      );
    }
  }

  fn draw_day_marks(&self, ui: &mut Ui, rect: Rect) {
    let visuals = ui.style().visuals.clone();
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);

    let today_index = self
      .current_time
      .map(|t| (t.date() - self.first_day).num_days());

    let mut day_mark_region = Rect::from_min_size(
      rect.left_top() + vec2(self.time_marker_margin_width, 0.0),
      vec2(
        self.day_width * self.day_count as f32,
        self.day_header_margin_height,
      ),
    );

    let mut alpha = 1.0;

    if day_mark_region.center().y <= ui.clip_rect().top() {
      // floating day mark region
      day_mark_region.set_top(ui.clip_rect().top());
      day_mark_region.set_height(self.day_header_margin_height);
      alpha = ui.ctx().animate_bool(
        egui::Id::new("day_mark"),
        !ui.rect_contains_pointer(day_mark_region),
      );
    }

    painter.rect_filled(
      day_mark_region.shrink(visuals.clip_rect_margin),
      widget_visuals.corner_radius,
      widget_visuals.bg_fill.linear_multiply(alpha * 0.8),
    );

    for nth_day in 0..self.day_count {
      let x = day_mark_region.left() + (nth_day as f32 + 0.5) * self.day_width;

      let text = self.day_header_text(nth_day).expect("day out of bound");

      let text_rect = painter.text(
        pos2(x, day_mark_region.center().y),
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace,
        widget_visuals.text_color().linear_multiply(alpha),
      );

      if Some(nth_day as i64) == today_index {
        // current day indicator
        let mut stroke = widget_visuals.bg_stroke;
        stroke.color = stroke.color.linear_multiply(alpha);

        painter.circle(
          text_rect.center_bottom() + vec2(0.0, 6.0),
          2.0,
          Color32::RED.linear_multiply(alpha),
          stroke,
        );
      }
    }
  }

  fn content_height(&self) -> f32 {
    self.segment_height * self.segment_count as f32
  }

  #[allow(unused)]
  fn content_width(&self) -> f32 {
    self.day_width * self.day_count as f32
  }

  fn content_offset(&self, widget_rect: Rect) -> Vec2 {
    widget_rect.min.to_vec2() + self.content_offset0()
  }

  fn content_offset0(&self) -> Vec2 {
    vec2(self.time_marker_margin_width, self.day_header_margin_height)
  }

  fn day_column(&self, day: usize) -> Rect {
    let x0 = self.day_width * day as f32;
    let y0 = 0.0;

    let w = self.day_width;
    let h = self.content_height();

    Rect::from_min_size(pos2(x0, y0), vec2(w, h))
  }

  fn day_header_text(&self, nth_day: usize) -> Option<String> {
    if nth_day >= self.day_count {
      return None;
    }

    let day = self.first_day + Duration::days(nth_day as i64);
    let formatted_day = day.format(self.day_header_format);

    Some(format!("{formatted_day}"))
  }

  fn time_marker_text(&self, segment: usize) -> Option<String> {
    if segment > self.segment_count {
      return None;
    }

    let time = self.time_marker_time(segment, 0).unwrap();
    let formatted_time = time.format(self.time_marker_format);

    Some(format!("{formatted_time}"))
  }

  fn time_marker_time(&self, segment: usize, day: usize) -> Option<DateTime> {
    if segment > self.segment_count {
      return None;
    }
    let day = self.first_day + Duration::days(day as i64);
    let seconds = SECS_PER_DAY as usize / self.segment_count * segment;
    if seconds >= SECS_PER_DAY as usize {
      (day + Duration::days(1))
        .and_time(NaiveTime::from_num_seconds_from_midnight(0, 0))
    } else {
      let offset = NaiveTime::from_num_seconds_from_midnight(seconds as u32, 0);
      day.and_time(offset)
    }
  }

  fn desired_size(&self, ui: &Ui) -> Vec2 {
    let visuals = ui.style().visuals.clone();
    let clip_margin = visuals.clip_rect_margin;

    // give a bit more vertical space to display the last time mark
    let text_safe_margin = 10.0;

    vec2(
      self.time_marker_margin_width
        + self.day_width * self.day_count as f32
        + clip_margin,
      self.day_header_margin_height
        + self.segment_height * self.segment_count as f32
        + text_safe_margin
        + clip_margin,
    )
  }

  pub(crate) fn show(
    &mut self,
    parent_ui: &mut Ui,
    state: &mut ScheduleUiState,
  ) {
    let (_id, rect) = parent_ui.allocate_space(self.desired_size(parent_ui));

    if !parent_ui.is_rect_visible(rect) {
      return;
    }

    self.regularize_events(&mut state.events);

    let mut child_ui = parent_ui.child_ui(rect, egui::Layout::left_to_right());
    let ui = &mut child_ui;

    self.draw_ticks(ui, rect);
    self.draw_current_time_indicator(ui, rect, 1.0);

    let interacting_event = InteractingEvent::get_event(ui);
    let interacting_event_id = interacting_event.as_ref().map(|x| x.id.clone());

    let layout = self.layout_events(&state.events, &interacting_event);

    let mut interacting_event_shown = false;
    for event in state.events.iter_mut() {
      if event.deleted {
        continue;
      }

      if interacting_event_id.as_ref() == Some(&event.id) {
        self.put_interacting_event_block(ui, &layout);
        interacting_event_shown = true;
      } else {
        self.put_non_interacting_event_block(ui, &layout, event);
      }
    }

    if interacting_event_id.is_some() && !interacting_event_shown {
      self.put_interacting_event_block(ui, &layout);
    }

    self.draw_day_marks(ui, rect);
    self.draw_time_marks(ui, rect);

    let response =
      ui.interact(ui.max_rect(), ui.id().with("empty_area"), Sense::drag());

    self.handle_new_event(ui, &response);
    self.handle_context_menu(ui, state, &response);

    if let Some(event) = InteractingEvent::get_commited_event(&ui) {
      commit_updated_event(&mut state.events, event);
    }
    remove_empty_events(&mut state.events);
  }

  fn handle_context_menu(
    &self,
    _ui: &mut Ui,
    state: &mut ScheduleUiState,
    response: &Response,
  ) {
    response.clone().context_menu(|ui| {
      if ui.button("Refresh").clicked() {
        state.refresh_requested = true;
        state.scope_updated = true;
        ui.label("Refreshing events...");
        ui.close_menu();
      }

      ui.separator();
      if ui.button("3-day view").clicked() {
        state.day_count = 3;
        state.scope_updated = true;
        ui.close_menu();
      }
      if ui.button("Weekly view").clicked() {
        state.day_count = 7;
        state.scope_updated = true;
        ui.close_menu();
      }

      ui.separator();

      ui.horizontal(|ui| {
        if ui.button("<<").clicked() {
          state.first_day =
            self.first_day - Duration::days(self.day_count as i64);
          state.scope_updated = true;
        }
        if ui.button("<").clicked() {
          state.first_day = self.first_day - Duration::days(1);
          state.scope_updated = true;
        }
        if ui.button("Today").clicked() {
          state.first_day =
            today(&self.timezone) - Duration::days(self.day_count as i64 / 2);
          state.scope_updated = true;
        }
        if ui.button(">").clicked() {
          state.first_day = self.first_day + Duration::days(1);
          state.scope_updated = true;
        }
        if ui.button(">>").clicked() {
          state.first_day =
            self.first_day + Duration::days(self.day_count as i64);
          state.scope_updated = true;
        }
      });

      ui.separator();

      if ui.button("Close menu").clicked() {
        ui.close_menu();
      }
    });
  }

  fn handle_new_event(&self, ui: &mut Ui, response: &Response) -> Option<()> {
    use FocusedEventState::Editing;

    let id = response.id;

    if response.drag_started()
      && response.dragged_by(egui::PointerButton::Primary)
    {
      let mut event = self.new_event();
      let pointer_pos = self.relative_pointer_pos(ui)?;
      let init_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;
      let new_state = self.assign_new_event_dates(ui, init_time, &mut event)?;

      ui.memory().data.insert_temp(id, event.id.clone());
      ui.memory().data.insert_temp(id, init_time);

      InteractingEvent::set(ui, event, new_state);

      return Some(());
    }

    if response.clicked_by(egui::PointerButton::Primary) {
      InteractingEvent::discard(ui);
      return Some(());
    }

    if response.drag_released() {
      let event_id = ui.memory().data.get_temp(id)?;
      let mut value = InteractingEvent::get_id(ui, &event_id)?;
      value.state = Editing;
      value.save(ui);
    }

    if response.dragged() && response.dragged_by(egui::PointerButton::Primary) {
      let event_id: String = ui.memory().data.get_temp(id)?;
      let init_time = ui.memory().data.get_temp(id)?;
      let mut value = InteractingEvent::get_id(ui, &event_id)?;
      let new_state =
        self.assign_new_event_dates(ui, init_time, &mut value.event)?;
      value.state = new_state;
      value.save(ui);
    }

    Some(())
  }

  fn new_event(&self) -> Event {
    let color = egui::Rgba::from(self.new_event_color);
    let mut event = EventBuilder::default()
      .id(new_event_id())
      .calendar(self.new_event_calendar.as_str())
      .title("")
      .description(None)
      .start(self.first_day.and_hms(0, 0, 0))
      .end(self.first_day.and_hms(0, 0, 0) + self.min_event_duration)
      .timestamp(now(&self.timezone))
      .created_at(now(&self.timezone))
      .modified_at(now(&self.timezone))
      .color([color.r(), color.g(), color.b()])
      .build()
      .unwrap();

    event.mark_changed();
    event
  }

  fn normalize_time(&self, time: &DateTime) -> DateTime {
    time.with_timezone(&self.timezone)
  }

  fn normalize_date(&self, date: &Date) -> Date {
    date.with_timezone(&self.timezone)
  }

  fn clone_to_new_event(&self, event: &Event) -> Event {
    let mut new_event = event.clone();
    new_event.id = new_event_id();
    new_event.mark_changed();
    new_event
  }

  fn pointer_to_datetime_auto(&self, ui: &Ui, pos: Pos2) -> Option<DateTime> {
    if ui.input().modifiers.shift_only() {
      // no snapping when shift is held down
      self.pointer_pos_to_datetime(pos)
    } else {
      // enable snapping otherwise
      self.pointer_pos_to_datetime_snapping(pos)
    }
  }

  // Need to ensure the ui's max_rect is the rect allocated for the
  // whole widget
  fn relative_pointer_pos(&self, ui: &Ui) -> Option<Pos2> {
    let mut pointer_pos = None
      .or_else(|| ui.input().pointer.interact_pos())
      .or_else(|| ui.input().pointer.hover_pos())?;
    pointer_pos -= self.content_offset(ui.max_rect());
    Some(pointer_pos)
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

  fn regularize_events(&self, events: &mut Vec<Event>) {
    remove_empty_events(events);

    for event in events.iter_mut() {
      event.set_timezone(&self.timezone);

      if event.end - event.start < self.min_event_duration {
        self.move_event_end(event, event.start + self.min_event_duration);
      }
    }
  }

  pub fn scroll_position(&self, time: &DateTime) -> f32 {
    self.date_time_to_pos(time).y
  }

  fn date_time_to_pos(&self, time: &DateTime) -> Pos2 {
    let x = (time.date() - self.first_day).num_days() as f32 / self.day_width
      + self.time_marker_margin_width;
    let y = self.day_progress(time) * self.content_height()
      + self.day_header_margin_height;
    pos2(x, y)
  }

  fn day_progress(&self, datetime: &DateTime) -> f32 {
    let datetime = self.normalize_time(datetime);
    let seconds_past_midnight = datetime.num_seconds_from_midnight();
    (seconds_past_midnight as f32 / SECS_PER_DAY as f32).clamp(0.0, 1.0)
  }

  fn layout_type(&self, event: &Event) -> EventLayoutType {
    if event.start.date() == event.end.date() {
      // single day event
      let date = self.normalize_date(&event.start.date());
      let a = self.day_progress(&event.start);
      let b = self.day_progress(&event.end);
      return EventLayoutType::Single(date, [a, b]);
    }

    if event.end == (event.start.date() + one_day()).and_hms(0, 0, 0) {
      let date = self.normalize_date(&event.start.date());
      let a = self.day_progress(&event.start);
      let b = 1.0;
      return EventLayoutType::Single(date, [a, b]);
    }

    unimplemented!()
  }
}

// HACK: allow editing to override existing drag state, because it
// seems that dragging always takes precedence.
fn state_override(
  old_state: FocusedEventState,
  new_state: FocusedEventState,
) -> FocusedEventState {
  if new_state == FocusedEventState::Editing {
    return new_state;
  }

  old_state
}

fn new_event_id() -> EventId {
  format!("{}", Uuid::new_v4().to_hyphenated())
}

fn on_the_same_day(mut t1: DateTime, mut t2: DateTime) -> bool {
  if t1.date() == t2.date() {
    return true;
  }

  if t2 < t1 {
    std::mem::swap(&mut t1, &mut t2);
  }

  if (t1.date() + one_day()).and_hms(0, 0, 0) == t2 {
    // to midnight
    return true;
  }

  false
}

// return if the times were been swapped
fn reorder_times(t1: &mut DateTime, t2: &mut DateTime) -> bool {
  if t1 < t2 {
    return false;
  }
  std::mem::swap(t1, t2);
  true
}

fn remove_empty_events(events: &mut Vec<Event>) {
  for event in events.iter_mut() {
    if event.title.is_empty() {
      event.mark_deleted();
    }
  }
}

fn commit_updated_event(events: &mut Vec<Event>, mut commited_event: Event) {
  let mut updated = false;

  for event in events.iter_mut() {
    if event.id == commited_event.id {
      event.mark_changed();
      *event = commited_event.clone();
      updated = true;
    }
  }

  if !updated {
    commited_event.mark_changed();
    events.push(commited_event);
  }
}
