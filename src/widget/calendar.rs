use chrono::{Datelike, Duration};
use derive_builder::Builder;

use eframe::egui::{self, Color32, Rect, RichText, Ui};

use crate::util::Date;

#[derive(Builder, Clone, Debug)]
pub struct Calendar {
  first_date: Date,
  last_date: Date,

  // used to show today indicator
  current_date: Option<Date>,

  #[builder(default = "[20.0, 20.0]")]
  day_square_size: [f32; 2],

  // 0: sunday first, 1: monday first
  #[builder(default = "0")]
  weekday_offset: usize,

  #[builder(default = "Vec::new()")]
  highlight_dates: Vec<Date>,
}

#[allow(unused)]
impl Calendar {
  const DAYS_PER_WEEK: usize = 7;
  const WEEK_DAYS: [&'static str; Self::DAYS_PER_WEEK] =
    ["S", "M", "T", "W", "T", "F", "S"];

  fn calc_bounding_rect(_ui: &Ui) -> Rect {
    todo!()
  }

  pub(crate) fn show_ui(&mut self, ui: &mut Ui) {
    egui::Grid::new("calendar")
      .num_columns(Self::DAYS_PER_WEEK)
      .min_col_width(self.day_square_size[0])
      .max_col_width(self.day_square_size[0])
      .min_row_height(self.day_square_size[1])
      .show(ui, |ui| {
        self.draw_week_header(ui);
        self.draw_days(ui);
      });
  }

  fn draw_week_header(&self, ui: &mut Ui) {
    let weekdays_in_order = Self::WEEK_DAYS
      .iter()
      .cycle()
      .skip(self.weekday_offset)
      .take(Self::DAYS_PER_WEEK);

    for weekday in weekdays_in_order {
      ui.vertical_centered(|ui| ui.label(*weekday));
    }

    ui.end_row();
  }

  fn draw_days(&self, ui: &mut Ui) {
    let days_to_skip = self.calc_weekday_location(self.first_date);
    for _ in 0..days_to_skip {
      // skip by drawing an empty label
      ui.label("");
    }

    let mut col = days_to_skip;
    let mut d = self.first_date;

    while d <= self.last_date {
      self.draw_day(ui, d);

      col += 1;
      if col == Self::DAYS_PER_WEEK {
        col = 0;
        ui.end_row();
      }

      d = d + Duration::days(1);
    }
  }

  fn draw_day(&self, ui: &mut Ui, date: Date) {
    let visuals = ui.visuals();
    let mut text = RichText::new(format!("{}", date.day()));

    if self.current_date == Some(date) {
      text = text.strong()
    }

    if self.highlight_dates.contains(&date) {
      text = text.underline();
    }

    ui.vertical_centered(|ui| {
      ui.button(text);
    });
  }

  fn calc_weekday_location(&self, date: Date) -> usize {
    let weekday = date.weekday().num_days_from_sunday() as usize;
    (weekday - self.weekday_offset) % Self::DAYS_PER_WEEK
  }
}
