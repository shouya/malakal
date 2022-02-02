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

  // events to render
  #[builder(default = "ScheduleUi::default_events()")]
  events: Vec<EventBlock>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct EventBlock {
  id: String,
  color: Color32,
  title: String,
  description: Option<String>,
  start: DateTime<Local>,
  end: DateTime<Local>,
}

#[derive(Debug)]
enum EventBlockType {
  SingleDay(Date<Local>, [f32; 2]),
  AllDay([Date<Local>; 2]),
  CrossDay([DateTime<Local>; 2]),
}

impl EventBlock {
  fn layout_type(&self) -> EventBlockType {
    if self.start.date() == self.end.date() {
      // single day event
      let date = self.start.date();
      let a = day_progress(&self.start);
      let b = day_progress(&self.end);
      return EventBlockType::SingleDay(date, [a, b]);
    }

    unimplemented!()
  }
}

const SECS_PER_DAY: u64 = 24 * 3600;

impl ScheduleUi {
  // just for debugging
  fn default_events() -> Vec<EventBlock> {
    let mut events = vec![];

    events.push(EventBlock {
      id: "1".into(),
      color: Color32::GREEN,
      title: "C: vocab".into(),
      description: None,
      start: Local::today().and_hms(14, 0, 0),
      end: Local::today().and_hms(15, 0, 0),
    });
    events.push(EventBlock {
      id: "2".into(),
      color: Color32::GREEN,
      title: "C: feynman".into(),
      description: None,
      start: Local::today().and_hms(15, 0, 0),
      end: Local::today().and_hms(16, 0, 0),
    });

    events
  }

  fn add_event_block(
    &mut self,
    ui: &mut Ui,
    event_block: &EventBlock,
    rect: Rect,
  ) -> Option<Response> {
    match event_block.layout_type() {
      EventBlockType::SingleDay(date, progress) => {
        return self.add_single_day_event_block(
          ui,
          event_block,
          date,
          progress,
          rect,
        );
      }
      _ => unimplemented!(),
    }
  }

  fn add_single_day_event_block(
    &mut self,
    ui: &mut Ui,
    event: &EventBlock,
    date: Date<Local>,
    progress: [f32; 2],
    rect: Rect,
  ) -> Option<Response> {
    let day_number = self.date_to_day(date)?;
    let x0 = self.day_width * day_number as f32;
    let x1 = self.day_width * (day_number + 1) as f32;
    let y0 = self.content_height() * progress[0];
    let y1 = self.content_height() * progress[1];

    let margin = ui.visuals().clip_rect_margin;

    let top_left = rect.left_top() + self.content_offset() + vec2(x0, y0);
    let bottom_right = rect.left_top() + self.content_offset() + vec2(x1, y1);
    let rect = Rect::from_min_max(top_left, bottom_right).shrink(margin);
    let layout = egui::Layout::left_to_right();
    let event_id = event.id.clone();

    let mut event_ui = ui.child_ui_with_id_source(rect, layout, event_id);

    let button = Button::new(event.title.clone());
    Some(event_ui.add_sized(rect.size(), button))
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
}

impl egui::Widget for &mut ScheduleUi {
  fn ui(self, ui: &mut Ui) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(
      self.desired_size(ui),
      egui::Sense::click_and_drag(),
    );

    if ui.is_rect_visible(rect) {
      self.draw_ticks(ui, rect);
    }

    for event in self.events.clone().iter() {
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
