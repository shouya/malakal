use std::collections::HashMap;

use chrono::{Date, DateTime, Duration, Local};
use derive_builder::Builder;
use eframe::egui::{self, vec2, Button, Color32, Rect, Response, Ui, Vec2};

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

  // used to render current time indicator
  #[builder(default = "Some(Local::now())")]
  current_time: Option<DateTime<Local>>,

  // used to refresh every second
  #[builder(default = "std::time::Instant::now()", setter(skip))]
  last_update: std::time::Instant,

  #[builder(default, setter(skip))]
  layout: HashMap<String, Vec<Rect>>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EventBlock {
  pub id: String,
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

    unimplemented!()
  }
}

const SECS_PER_DAY: u64 = 24 * 3600;

impl ScheduleUi {
  // the caller must ensure the events are all within the correct days
  fn layout_events(&mut self, events: &[EventBlock]) {
    use intervaltree::{Element, IntervalTree};
    // EventId => (col_start, col_end, total_cols)
    let mut event_column: HashMap<&String, (usize, usize, usize)> =
      HashMap::new();

    // First step: assign each event a column
    for day in 0..self.day_count {
      // layout for each day
      let filtered_events: HashMap<_, _> = events
        .iter()
        .filter(|&e| matches!(e.layout_type(), EventBlockType::Single(..)))
        .filter(|&e| self.date_to_day(e.start.date()) == Some(day))
        .map(|e| (&e.id, e))
        .collect();

      let tree: IntervalTree<DateTime<Local>, &String> = filtered_events
        .iter()
        .map(|(&id, e)| (e.start..e.end, id))
        .collect();

      for Element { range, value: id } in tree.iter_sorted() {
        if event_column.contains_key(id) {
          continue;
        }

        let range = range.clone();
        let mut siblings: Vec<_> = tree.query(range.clone()).collect();
        siblings.sort_by_key(|e| e.range.start);

        let num_cols = siblings.len();
        for (n, Element { value: id, .. }) in siblings.into_iter().enumerate() {
          event_column.entry(*id).or_insert((n, n + 1, num_cols));
        }
      }
    }

    self.layout.clear();
    // Second step: assign each event a rect
    for e in events {
      if let EventBlockType::Single(date, [a, b]) = e.layout_type() {
        if let Some(day) = self.date_to_day(date) {
          let (col0, col1, tcol) = event_column.get(&e.id).unwrap();
          let t0 = *col0 as f32 / *tcol as f32;
          let t1 = *col1 as f32 / *tcol as f32;

          let x0 = self.day_width * (day as f32 + t0);
          let x1 = self.day_width * (day as f32 + t1);
          let y0 = self.content_height() * a;
          let y1 = self.content_height() * b;

          let rect = Rect::from_x_y_ranges(x0..=x1, y0..=y1);
          self.layout.insert(e.id.clone(), vec![rect]);
        }
      }
    }
  }

  fn add_event_block(
    &mut self,
    ui: &mut Ui,
    event_block: &mut EventBlock,
    rect: Rect,
  ) -> Option<Response> {
    match event_block.layout_type() {
      EventBlockType::Single(..) => {
        return self.add_single_day_event_block(ui, event_block, rect);
      }
      _ => unimplemented!(),
    }
  }

  fn add_single_day_event_block(
    &mut self,
    ui: &mut Ui,
    event: &mut EventBlock,
    rect: Rect,
  ) -> Option<Response> {
    let event_rect = self.layout.get(&event.id)?.first()?;

    let event_rect_margin = ui.visuals().clip_rect_margin;
    let event_rect_offset = rect.left_top().to_vec2() + self.content_offset();

    let event_rect = event_rect
      .shrink(event_rect_margin)
      .translate(event_rect_offset);

    let layout = egui::Layout::left_to_right();
    let event_id = event.id.clone();

    let mut event_ui = ui.child_ui_with_id_source(event_rect, layout, event_id);

    let button = Button::new(event.title.clone());
    Some(event_ui.add_sized(event_rect.size(), button))
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

    let base_pos = rect.left_top() + self.content_offset();
    let painter = ui.painter_at(rect);

    // vertical lines
    for day in 0..=self.day_count {
      let x = self.day_width * day as f32;
      let y0 = 0.0;
      let y1 = self.segment_height * self.segment_count as f32;
      let ends = [base_pos + vec2(x, y0), base_pos + vec2(x, y1)];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // horizontal lines
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x0 = 0.0;
      let x1 = self.day_width * self.day_count as f32;
      let ends = [base_pos + vec2(x0, y), base_pos + vec2(x1, y)];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // draw the day marks
    for nth_day in 0..self.day_count {
      let y = -(self.day_header_margin_height - visuals.clip_rect_margin) / 2.0;
      let x = self.day_width * (nth_day as f32 + 0.5);

      let text = self.day_header_text(nth_day).expect("day out of bound");

      painter.text(
        base_pos + vec2(x, y),
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
        base_pos + vec2(x, y),
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

      let p0 = base_pos + vec2(x0, y);
      let p1 = base_pos + vec2(x1, y);
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
  fn content_offset(&self) -> Vec2 {
    vec2(self.time_marker_margin_width, self.day_header_margin_height)
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
    let (rect, mut response) = ui.allocate_exact_size(
      self.desired_size(ui),
      egui::Sense::click_and_drag(),
    );

    if ui.is_rect_visible(rect) {
      self.draw_ticks(ui, rect);
    }

    self.layout_events(events);

    for event in events.iter_mut() {
      if let Some(event_response) = self.add_event_block(ui, event, rect) {
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
