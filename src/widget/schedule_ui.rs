use chrono::{Date, DateTime, Duration, Local};
use derive_builder::Builder;
use eframe::egui::{self, Rect, Response, Ui, Vec2};

#[derive(Builder, Debug, PartialEq)]
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
  day_marker_margin_height: f32,
  #[builder(default = "\"%H:%M\"")]
  time_marker_format: &'static str,
  #[builder(default = "Local::today()")]
  first_day: Date<Local>,
}

impl ScheduleUi {
  fn draw_ticks(&self, ui: &mut Ui, rect: Rect) {
    let visuals = ui.style().visuals.clone();
    let widget_visuals = ui.style().noninteractive();

    let base_pos = rect.left_top()
      + egui::vec2(
        self.time_marker_margin_width,
        self.day_marker_margin_height,
      );
    let painter = ui.painter_at(rect);

    // vertical lines
    for day in 0..=self.day_count {
      let x = self.day_width * day as f32;
      let y0 = 0.0;
      let y1 = self.segment_height * self.segment_count as f32;
      let ends = [base_pos + egui::vec2(x, y0), base_pos + egui::vec2(x, y1)];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // horizontal lines
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x0 = 0.0;
      let x1 = self.day_width * self.day_count as f32;
      let ends = [base_pos + egui::vec2(x0, y), base_pos + egui::vec2(x1, y)];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // draw the time marks
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x = -visuals.clip_rect_margin;

      let text = self.time_marker_text(seg).expect("segment out of bound");
      painter.text(
        base_pos + egui::vec2(x, y),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::TextStyle::Monospace,
        widget_visuals.text_color(),
      );
    }
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
    let offset = 24 * 3600 / self.segment_count * segment;
    Some(beginning_of_day + Duration::seconds(offset as i64))
  }

  fn desired_size(&self, _ui: &Ui) -> Vec2 {
    // give a bit more vertical space to display the last time mark
    let text_safe_margin = 10.0;

    Vec2::new(
      self.day_width * (self.day_count + 1) as f32,
      self.segment_height * (self.segment_count + 1) as f32 + text_safe_margin,
    )
  }
}

impl egui::Widget for &mut ScheduleUi {
  fn ui(self, ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(
      self.desired_size(ui),
      egui::Sense::click_and_drag(),
    );

    if ui.is_rect_visible(rect) {
      self.draw_ticks(ui, rect);
    }

    response
  }
}
