mod layout;

use chrono::{Date, DateTime, Duration, Local};
use derive_builder::Builder;
use eframe::egui::{
  self, pos2, vec2, Button, Color32, CursorIcon, Label, LayerId, Pos2, Rect,
  Response, Sense, Ui, Vec2,
};

use layout::{Layout, LayoutAlgorithm};

#[derive(Builder, Clone, Debug, PartialEq)]
pub struct ScheduleUi {
  #[builder(default = "3")]
  day_count: usize,
  #[builder(default = "260.0")]
  day_width: f32,
  #[builder(default = "24")]
  segment_count: usize,
  #[builder(default = "60.0")]
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

  #[builder(default)]
  current_event: Option<EventBlock>,
}

type EventId = String;

#[derive(Debug, PartialEq, Clone)]
pub struct EventBlock {
  pub id: EventId,
  pub color: Color32,
  pub title: String,
  pub description: Option<String>,
  pub start: DateTime<Local>,
  pub end: DateTime<Local>,
}

#[derive(Debug)]
enum EventBlockType {
  Single(Date<Local>, [f32; 2]),
  #[allow(unused)]
  AllDay([Date<Local>; 2]),
  #[allow(unused)]
  Multi([DateTime<Local>; 2]),
}

impl EventBlock {
  fn layout_type(&self) -> EventBlockType {
    if self.start.date() == self.end.date() {
      // single day event
      let date = self.start.date();
      let a = day_progress(&self.start);
      let b = day_progress(&self.end);
      return EventBlockType::Single(date, [a, b]);
    }

    if self.end == (self.start.date() + one_day()).and_hms(0, 0, 0) {
      let date = self.start.date();
      let a = day_progress(&self.start);
      let b = 1.0;
      return EventBlockType::Single(date, [a, b]);
    }

    unimplemented!()
  }
}

const SECS_PER_DAY: u64 = 24 * 3600;

// can't be a constant because chrono::Duration constructors are not
// declared as const functions.
fn one_day() -> Duration {
  Duration::days(1)
}

impl ScheduleUi {
  // the caller must ensure the events are all within the correct days
  fn layout_events<'a>(&self, events: &'a [EventBlock]) -> Layout {
    let mut layout = Layout::default();
    for day in 0..self.day_count {
      // layout for each day
      let events: Vec<layout::Ev<'a>> = events
        .iter()
        .filter(|&e| self.date_to_day(e.start.date()) == Some(day))
        .filter(|&e| matches!(e.layout_type(), EventBlockType::Single(..)))
        .map(|e| (&e.id, e.start.timestamp(), e.end.timestamp()).into())
        .collect();

      layout.merge(layout::MarkusAlgorithm::compute(events))
    }
    layout
  }

  fn add_event_block(
    &self,
    ui: &mut Ui,
    event_block: &mut EventBlock,
    widget_rect: Rect,
    layout: &Layout,
  ) -> Option<Response> {
    match event_block.layout_type() {
      EventBlockType::Single(date, y) => {
        let rel_x = layout.query(&event_block.id)?;
        let day = self.date_to_day(date)?;
        let event_rect =
          self.layout_single_day_event(widget_rect, day, y, rel_x);

        self.put_event_block(ui, event_block, event_rect, widget_rect)
      }
      _ => unimplemented!(),
    }
  }

  fn layout_single_day_event(
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

  fn put_event_block(
    &self,
    ui: &mut Ui,
    event: &mut EventBlock,
    event_rect: Rect,
    widget_rect: Rect,
  ) -> Option<Response> {
    let event_rect = event_rect.shrink(1.0);
    let id = egui::Id::new("event").with(&event.id);

    let button = Button::new(event.title.clone()).fill(event.color);
    let button_resp = ui.put(event_rect, button);

    if let Some(updated_event) =
      self.event_resizers(ui, id, event, event_rect, widget_rect)
    {
      *event = updated_event;
    }

    Some(button_resp)
  }

  fn event_resizers(
    &self,
    ui: &mut Ui,
    id: egui::Id,
    event: &EventBlock,
    event_rect: Rect,
    widget_rect: Rect,
  ) -> Option<EventBlock> {
    let [upper_rect, lower_rect] = self.event_block_resizer_regions(event_rect);

    let upper_resizer =
      self.draw_resizer(ui, id.with("res.upper"), widget_rect, upper_rect);
    let lower_resizer =
      self.draw_resizer(ui, id.with("res.lower"), widget_rect, lower_rect);

    if let Some(t) = upper_resizer {
      let mut changed_event = event.clone();
      self.set_event_start(&mut changed_event, t);
      self.show_resizer_hint(ui, upper_rect, changed_event.start);

      return Some(changed_event);
    }
    if let Some(t) = lower_resizer {
      let mut changed_event = event.clone();

      self.set_event_end(&mut changed_event, t);
      self.show_resizer_hint(ui, lower_rect, changed_event.end);

      return Some(changed_event);
    }

    None
  }

  fn draw_resizer(
    &self,
    ui: &mut Ui,
    id: egui::Id,
    widget_rect: Rect,
    rect: Rect,
  ) -> Option<DateTime<Local>> {
    if rect.area() == 0.0 {
      return None;
    }

    let is_being_dragged = ui.memory().is_being_dragged(id);

    if !is_being_dragged {
      let response = ui.interact(rect, id, Sense::drag());
      if response.hovered() {
        ui.output().cursor_icon = CursorIcon::ResizeVertical;
      }

      return None;
    }

    // dragging
    ui.output().cursor_icon = CursorIcon::ResizeVertical;

    let pointer_pos = ui.input().pointer.interact_pos()?;

    let datetime = if ui.input().modifiers.shift_only() {
      // no snapping when shift is held down
      self.pointer_pos_to_datetime(widget_rect, pointer_pos)
    } else {
      // enable snapping otherwise
      self.pointer_pos_to_datetime_snapping(widget_rect, pointer_pos)
    };

    datetime
  }

  fn show_resizer_hint(&self, ui: &mut Ui, rect: Rect, time: DateTime<Local>) {
    let layer_id = egui::Id::new("resizer_hint");
    let layer = LayerId::new(egui::Order::Tooltip, layer_id);

    let text = format!("{}", time.format(self.event_resizing_hint_format));
    let label = Label::new(egui::RichText::new(text).monospace());

    ui.with_layer_id(layer, |ui| ui.put(rect, label));
  }

  // squeeze event end if necessary
  fn set_event_start(
    &self,
    event: &mut EventBlock,
    mut new_start: DateTime<Local>,
  ) {
    if event.end <= new_start {
      let mut new_end = new_start + self.min_event_duration;
      if new_end.date() != event.end.date() {
        let end_of_day = (event.end.date() + one_day()).and_hms(0, 0, 0);
        new_end = end_of_day - Duration::seconds(1);
        new_start = end_of_day - self.min_event_duration;
      }
      event.end = new_end;
    }
    event.start = new_start;
  }

  fn set_event_end(
    &self,
    event: &mut EventBlock,
    mut new_end: DateTime<Local>,
  ) {
    if new_end <= event.start {
      let mut new_start = new_end - self.min_event_duration;
      if new_start.date() != event.start.date() {
        let beginning_of_day = event.start.date().and_hms(0, 0, 0);
        new_start = beginning_of_day;
        new_end = new_start + self.min_event_duration;
      }
      event.start = new_start;
    }
    event.end = new_end;
  }

  fn pointer_pos_to_datetime(
    &self,
    widget_rect: Rect,
    pointer_pos: Pos2,
  ) -> Option<DateTime<Local>> {
    let rel_pos = pointer_pos - self.content_offset(widget_rect);
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
    widget_rect: Rect,
    pointer_pos: Pos2,
  ) -> Option<DateTime<Local>> {
    let rel_pos = pointer_pos - self.content_offset(widget_rect);
    let day = (rel_pos.x / self.day_width) as i64;
    if !(day >= 0 && day < self.day_count as i64) {
      return None;
    }

    let vert_pos = rel_pos.y / self.content_height();
    if !(vert_pos > 0.0 && vert_pos < 1.0) {
      return None;
    }

    let seconds = SECS_PER_DAY as f32 * vert_pos;
    let snapped_seconds =
      (seconds / self.snapping_duration.num_seconds() as f32).round() as i64
        * self.snapping_duration.num_seconds() as i64;

    let date = self.first_day + Duration::days(day);
    let time = date.and_hms(0, 0, 0) + Duration::seconds(snapped_seconds);
    Some(time)
  }

  fn event_block_resizer_regions(&self, rect: Rect) -> [Rect; 2] {
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

  pub fn show(
    &mut self,
    ui: &mut Ui,
    events: &mut Vec<EventBlock>,
  ) -> Response {
    let (rect, mut response) =
      ui.allocate_exact_size(self.desired_size(ui), egui::Sense::hover());

    if ui.is_rect_visible(rect) {
      self.draw_ticks(ui, rect);
    }

    let layout = self.layout_events(events);

    for event in events.iter_mut() {
      if let Some(event_response) =
        self.add_event_block(ui, event, rect, &layout)
      {
        response = response.union(event_response);
      }
    }

    response
  }
}

fn day_progress(datetime: &DateTime<Local>) -> f32 {
  let beginning_of_day = datetime.date().and_hms(0, 0, 0);
  let seconds_past_midnight = (*datetime - beginning_of_day).num_seconds();
  (seconds_past_midnight as f32 / SECS_PER_DAY as f32).clamp(0.0, 1.0)
}
