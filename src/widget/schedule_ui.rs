mod layout;

use chrono::{Date, DateTime, Duration, Local};
use derive_builder::Builder;
use eframe::egui::{
  self, pos2, vec2, Color32, CursorIcon, Label, LayerId, Pos2, Rect, Response,
  Sense, Ui, Vec2,
};
use uuid::Uuid;

use layout::{Layout, LayoutAlgorithm};

use crate::event::{Event, EventBuilder};

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
  #[builder(default = "\"%F\"")]
  day_header_format: &'static str,
  #[builder(default = "Local::today()")]
  first_day: Date<Local>,

  // a small margin on the right of day columns reserved for creating
  // new events
  #[builder(default = "20.0")]
  new_event_margin: f32,

  // used to render current time indicator
  #[builder(default = "Some(Local::now())")]
  current_time: Option<DateTime<Local>>,

  // used to refresh every second
  #[builder(default = "std::time::Instant::now()", setter(skip))]
  last_update: std::time::Instant,

  #[builder(default = "15.0")]
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

  new_event_calendar: String,
}

type EventId = String;

#[derive(Clone, Copy, Debug)]
struct DraggingEventYOffset(f32);

#[derive(Debug)]
enum EventLayoutType {
  Single(Date<Local>, [f32; 2]),
  #[allow(unused)]
  AllDay([Date<Local>; 2]),
  #[allow(unused)]
  Multi([DateTime<Local>; 2]),
}

const SECS_PER_DAY: u64 = 24 * 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedEventState {
  Editing,
  Dragging,
  DraggingEventStart,
  DraggingEventEnd,
}

// can't be a constant because chrono::Duration constructors are not
// declared as const functions.
fn one_day() -> Duration {
  Duration::days(1)
}

impl ScheduleUi {
  // the caller must ensure the events are all within the correct days
  fn layout_events<'a>(&self, events: &'a [Event]) -> Layout {
    let mut layout = Layout::default();
    for day in 0..self.day_count {
      // layout for each day
      let events: Vec<layout::Ev<'a>> = events
        .iter()
        .filter(|&e| !e.deleted)
        .filter(|&e| self.date_to_day(e.start.date()) == Some(day))
        .filter(|&e| matches!(layout_type(e), EventLayoutType::Single(..)))
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

  fn add_event(
    &self,
    ui: &mut Ui,
    event: &mut Event,
    layout: &Layout,
  ) -> Option<Response> {
    let widget_rect = ui.max_rect();
    match layout_type(event) {
      EventLayoutType::Single(date, y) => {
        let rel_x = layout.query(&event.id)?;
        let day = self.date_to_day(date)?;
        let event_rect = self.layout_event(widget_rect, day, y, rel_x);
        self.put_event_block(ui, event, event_rect)
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

  fn event_interaction_state(
    &self,
    id: egui::Id,
    ui: &mut Ui,
  ) -> Option<FocusedEventState> {
    ui.memory().data.get_temp(id)
  }

  fn set_event_interaction_state(
    &self,
    id: egui::Id,
    ui: &mut Ui,
    state: Option<FocusedEventState>,
  ) {
    if let Some(state) = state {
      ui.memory().data.insert_temp(id, state);
    } else {
      ui.memory().data.remove::<FocusedEventState>(id);
    }
  }

  fn interact_event_region(
    &self,
    ui: &mut Ui,
    resp: Response,
  ) -> Option<FocusedEventState> {
    let event_rect = resp.rect;
    let [upper, lower] = self.event_resizer_regions(event_rect);

    let _lmb = egui::PointerButton::Primary;

    let interact_pos =
      resp.interact_pointer_pos().or_else(|| resp.hover_pos())?;

    if upper.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::ResizeVertical;
      if resp.drag_started() {
        return Some(FocusedEventState::DraggingEventStart);
      }
      return None;
    }

    if lower.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::ResizeVertical;
      if resp.drag_started() {
        return Some(FocusedEventState::DraggingEventEnd);
      }
      return None;
    }

    if event_rect.contains(interact_pos) {
      ui.output().cursor_icon = CursorIcon::Grab;

      if resp.clicked() {
        return Some(FocusedEventState::Editing);
      }

      if resp.drag_started() {
        let offset = DraggingEventYOffset(event_rect.top() - interact_pos.y);
        ui.memory().data.insert_temp(egui::Id::null(), offset);
        return Some(FocusedEventState::Dragging);
      }

      return None;
    }

    None
  }

  // return None if the interaction is finished
  fn interact_event(
    &self,
    ui: &mut Ui,
    event_rect: Rect,
    state: FocusedEventState,
    event: &mut Event,
  ) -> Option<Response> {
    let [upper, lower] = self.event_resizer_regions(event_rect);

    let resp = self.place_event_button(ui, event_rect, event);
    let active = match state {
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
      FocusedEventState::Editing => unreachable!(),
    };

    active.then(|| resp)
  }

  fn handle_event_resizing(
    &self,
    ui: &mut Ui,
    rect: Rect,
    set_time: impl FnOnce(DateTime<Local>) -> DateTime<Local>,
  ) -> bool {
    if !ui.memory().is_anything_being_dragged() {
      return false;
    }

    ui.output().cursor_icon = CursorIcon::ResizeVertical;

    let pointer_pos = self.relative_pointer_pos(ui).unwrap();
    if let Some(datetime) = self.pointer_to_datetime_auto(ui, pointer_pos) {
      let updated_time = set_time(datetime);
      self.show_resizer_hint(ui, rect, updated_time);
    }

    true
  }

  fn handle_event_dragging(
    &self,
    ui: &mut Ui,
    rect: Rect,
    set_time: impl FnOnce(DateTime<Local>) -> (DateTime<Local>, DateTime<Local>),
  ) -> bool {
    if !ui.memory().is_anything_being_dragged() {
      return false;
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

    true
  }

  fn put_event_block(
    &self,
    ui: &mut Ui,
    event: &mut Event,
    event_rect: Rect,
  ) -> Option<Response> {
    let event_rect =
      event_rect.shrink(ui.style().visuals.clip_rect_margin / 2.0);
    let id = event_egui_id(event);

    let interaction_state = self.event_interaction_state(id, ui);

    match interaction_state {
      None => {
        let resp = self.place_event_button(ui, event_rect, event);
        if let Some(state) = self.interact_event_region(ui, resp) {
          self.set_event_interaction_state(id, ui, Some(state));
        }
      }
      Some(FocusedEventState::Editing) => {
        if self.place_event_editor(ui, event_rect, event).is_none() {
          self.set_event_interaction_state(id, ui, None);
        }
      }
      Some(state) => match self.interact_event(ui, event_rect, state, event) {
        None => self.set_event_interaction_state(id, ui, None),
        Some(resp) => {
          if let Some(new_state) = self.interact_event_region(ui, resp) {
            let final_state = state_override(state, new_state);
            self.set_event_interaction_state(id, ui, Some(final_state));
          }
        }
      },
    };

    None
  }

  fn place_event_button(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &Event,
  ) -> Response {
    let button = egui::Button::new(&event.title).sense(Sense::click_and_drag());
    ui.put(rect, button)
  }

  fn place_event_editor(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &mut Event,
  ) -> Option<()> {
    event
      .updated_title
      .get_or_insert_with(|| event.title.clone());

    let editor =
      egui::TextEdit::singleline(event.updated_title.as_mut().unwrap());
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
      discard_event_title(event);
      return None;
    }

    if resp.lost_focus() || resp.clicked_elsewhere() || anything_else_dragging {
      change_event_title(event);
      return None;
    }

    resp.request_focus();
    Some(())
  }

  fn show_resizer_hint(&self, ui: &mut Ui, rect: Rect, time: DateTime<Local>) {
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
  fn move_event_start(&self, event: &mut Event, new_start: DateTime<Local>) {
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

  fn move_event_end(&self, event: &mut Event, new_end: DateTime<Local>) {
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

  fn move_event(&self, event: &mut Event, new_start: DateTime<Local>) {
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

  fn pointer_pos_to_datetime(&self, rel_pos: Pos2) -> Option<DateTime<Local>> {
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
  ) -> Option<DateTime<Local>> {
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

  fn date_to_day(&self, date: Date<Local>) -> Option<usize> {
    let diff_days = (date - self.first_day).num_days();
    if diff_days < 0 || diff_days >= self.day_count as i64 {
      return None;
    }

    Some(diff_days as usize)
  }

  fn draw_ticks(&self, ui: &mut Ui, rect: Rect) {
    let visuals = ui.style().visuals.clone();
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

    // draw the day marks
    for nth_day in 0..self.day_count {
      let y = -(self.day_header_margin_height - visuals.clip_rect_margin) / 2.0;
      let x = self.day_width * (nth_day as f32 + 0.5);

      let text = self.day_header_text(nth_day).expect("day out of bound");

      painter.text(
        pos2(x, y) + offset,
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace,
        widget_visuals.text_color(),
      );
    }

    // draw the time marks
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x = -(self.time_marker_margin_width - visuals.clip_rect_margin) / 2.0;

      let text = self.time_marker_text(seg).expect("segment out of bound");
      painter.text(
        pos2(x, y) + offset,
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace,
        widget_visuals.text_color(),
      );
    }

    // draw current time indicator
    if let Some(now) = self.current_time.as_ref() {
      let y = day_progress(now) * self.content_height();
      let x0 = -visuals.clip_rect_margin;
      let x1 = self.content_width();

      let p0 = pos2(x0, y) + offset;
      let p1 = pos2(x1, y) + offset;
      let mut indicator_stroke = widget_visuals.bg_stroke;
      indicator_stroke.color = Color32::RED;
      painter.line_segment([p0, p1], indicator_stroke);
    }
  }

  fn content_height(&self) -> f32 {
    self.segment_height * self.segment_count as f32
  }

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

  fn time_marker_time(
    &self,
    segment: usize,
    day: usize,
  ) -> Option<DateTime<Local>> {
    if segment > self.segment_count {
      return None;
    }
    let day = self.first_day + Duration::days(day as i64);
    let beginning_of_day = day.and_hms(0, 0, 0);
    let offset = SECS_PER_DAY as usize / self.segment_count * segment;
    Some(beginning_of_day + Duration::seconds(offset as i64))
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

  pub fn show(&mut self, parent_ui: &mut Ui, events: &mut Vec<Event>) {
    parent_ui.ctx().set_debug_on_hover(true);

    let (_id, rect) = parent_ui.allocate_space(self.desired_size(parent_ui));

    if parent_ui.is_rect_visible(rect) {
      self.draw_ticks(parent_ui, rect);
    }

    let mut ui = parent_ui.child_ui(rect, egui::Layout::left_to_right());

    let layout = self.layout_events(events);

    let mut event_overlay_ui = ui.child_ui(rect, egui::Layout::left_to_right());

    for event in events.iter_mut() {
      if event.deleted {
        continue;
      }

      self.add_event(&mut event_overlay_ui, event, &layout);
    }

    self.handle_new_event(&mut ui, events);

    remove_empty_events(events);
  }

  fn handle_new_event(
    &self,
    ui: &mut Ui,
    events: &mut Vec<Event>,
  ) -> Option<Response> {
    use FocusedEventState::{DraggingEventEnd, Editing};

    let id = ui.make_persistent_id("empty_area").with("dragging");
    let response = ui.interact(ui.max_rect(), id, Sense::drag());

    if response.drag_started() {
      let mut event = self.new_event();
      let pointer_pos = self.relative_pointer_pos(ui)?;
      let init_time = self.pointer_to_datetime_auto(ui, pointer_pos)?;

      self.assign_new_event_dates(ui, init_time, &mut event)?;

      ui.memory().data.insert_temp(id, event.id.clone());
      ui.memory().data.insert_temp(id, init_time);

      ui.memory()
        .data
        .insert_temp(event_egui_id(&event), DraggingEventEnd);
      events.push(event);
      return Some(response);
    }

    if response.drag_released() {
      let event_id = ui.memory().data.get_temp(id)?;
      let event = find_event_mut(events, &event_id)?;
      ui.memory().data.insert_temp(event_egui_id(event), Editing);
      ui.memory().data.remove::<Event>(id);
      return Some(response);
    }

    if response.dragged() {
      let event_id = ui.memory().data.get_temp(id)?;
      let event = find_event_mut(events, &event_id)?;
      let init_time = ui.memory().data.get_temp(id)?;
      let state = self.assign_new_event_dates(ui, init_time, event)?;
      ui.memory().data.insert_temp(event_egui_id(event), state);
    }

    Some(response)
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
      .color([color.r(), color.g(), color.b()])
      .build()
      .unwrap();

    event.updated_title = Some("".into());
    event.mark_changed();
    event
  }

  fn pointer_to_datetime_auto(
    &self,
    ui: &Ui,
    pos: Pos2,
  ) -> Option<DateTime<Local>> {
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
    init_time: DateTime<Local>,
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
      if day_progress(&init_time) < 0.5 {
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
}

fn day_progress(datetime: &DateTime<Local>) -> f32 {
  let beginning_of_day = datetime.date().and_hms(0, 0, 0);
  let seconds_past_midnight = (*datetime - beginning_of_day).num_seconds();
  (seconds_past_midnight as f32 / SECS_PER_DAY as f32).clamp(0.0, 1.0)
}

// HACK: allow editing to override existing drag state, because it
// seems that dragging always takes precedence.
//
// At the same time, do not allow resizing to be overridden by editing.
fn state_override(
  old_state: FocusedEventState,
  new_state: FocusedEventState,
) -> FocusedEventState {
  if old_state == FocusedEventState::Dragging
    && new_state == FocusedEventState::Editing
  {
    return new_state;
  }

  old_state
}

fn event_egui_id(event: &Event) -> egui::Id {
  egui::Id::new("event").with(&event.id)
}

fn new_event_id() -> EventId {
  format!("{}", Uuid::new_v4().to_hyphenated())
}

fn on_the_same_day(mut t1: DateTime<Local>, mut t2: DateTime<Local>) -> bool {
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
fn reorder_times(t1: &mut DateTime<Local>, t2: &mut DateTime<Local>) -> bool {
  if t1 < t2 {
    return false;
  }
  std::mem::swap(t1, t2);
  true
}

fn find_event_mut<'a>(
  events: &'a mut Vec<Event>,
  id: &EventId,
) -> Option<&'a mut Event> {
  events.iter_mut().find(|x| x.id == *id)
}

fn remove_empty_events(events: &mut Vec<Event>) {
  for event in events.iter_mut() {
    if event.title.is_empty() && event.updated_title.is_none() {
      event.mark_deleted();
    }
  }
}

fn layout_type(event: &Event) -> EventLayoutType {
  if event.start.date() == event.end.date() {
    // single day event
    let date = event.start.date();
    let a = day_progress(&event.start);
    let b = day_progress(&event.end);
    return EventLayoutType::Single(date, [a, b]);
  }

  if event.end == (event.start.date() + one_day()).and_hms(0, 0, 0) {
    let date = event.start.date();
    let a = day_progress(&event.start);
    let b = 1.0;
    return EventLayoutType::Single(date, [a, b]);
  }

  unimplemented!()
}

fn change_event_title(event: &mut Event) {
  if let Some(new_title) = event.updated_title.take() {
    if !new_title.is_empty() && event.title != new_title {
      event.mark_changed();
      event.title = new_title;
    }
  }
}

fn discard_event_title(event: &mut Event) {
  event.updated_title.take();
}
