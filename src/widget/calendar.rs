use chrono::{Datelike, Duration};
use derive_builder::Builder;

use eframe::egui::{self, Rect, RichText, Ui};

use crate::util::{beginning_of_month, end_of_month, Date};

#[derive(Builder, Clone, Debug, PartialEq)]
pub struct Calendar {
  // we only use the year and month part of this date. The day is irrelevant.
  date: Date,

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

pub enum CalendarAction {
  DateClicked(Date),
}

#[allow(unused)]
impl Calendar {
  const DAYS_PER_WEEK: usize = 7;
  const WEEK_DAYS: [&'static str; Self::DAYS_PER_WEEK] =
    ["S", "M", "T", "W", "T", "F", "S"];

  fn calc_bounding_rect(_ui: &Ui) -> Rect {
    todo!()
  }

  pub(crate) fn show_ui(&mut self, ui: &mut Ui) -> Option<CalendarAction> {
    let mut action = None;

    self.draw_month_header(ui);

    egui::Grid::new("calendar")
      .num_columns(Self::DAYS_PER_WEEK)
      .min_col_width(self.day_square_size[0])
      .max_col_width(self.day_square_size[0])
      .min_row_height(self.day_square_size[1])
      .show(ui, |ui| {
        self.draw_week_header(ui);
        action = self.draw_days(ui);
      });

    action
  }

  fn draw_month_header(&mut self, ui: &mut Ui) {
    ui.horizontal(|ui| {
      if ui.button("<<").clicked() {
        self.date = month_offset(self.date, -1);
      }

      ui.label(format!("{}", self.date.format("%Y-%m")));

      if ui.button(">>").clicked() {
        self.date = month_offset(self.date, 1);
      }
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

  fn draw_days(&self, ui: &mut Ui) -> Option<CalendarAction> {
    let mut action = None;

    let bom = beginning_of_month(self.date);
    let eom = end_of_month(self.date);

    let days_form_previous_month = self.calc_weekday_location(bom);
    let days_from_next_month =
      Self::DAYS_PER_WEEK - self.calc_weekday_location(eom);

    let total_days = days_form_previous_month
      + days_from_next_month
      + (eom - bom).num_days() as usize;

    let mut date = bom - Duration::days(days_form_previous_month as i64);

    // draw days of the previous month
    for i in 0..total_days {
      let col = i % Self::DAYS_PER_WEEK;

      action = action.or_else(|| self.draw_day(ui, date));
      if col + 1 == Self::DAYS_PER_WEEK {
        ui.end_row();
      }

      date = date + Duration::days(1);
    }

    action
  }

  fn draw_day(&self, ui: &mut Ui, date: Date) -> Option<CalendarAction> {
    let visuals = ui.visuals();
    let mut text = RichText::new(format!("{}", date.day()));

    if !same_month(date, self.date) {
      text = text.weak();
    }

    if self.current_date == Some(date) {
      text = text.strong()
    }

    if self.highlight_dates.contains(&date) {
      text = text.underline();
    }

    if ui.vertical_centered(|ui| ui.button(text)).inner.clicked() {
      return Some(CalendarAction::DateClicked(date));
    }

    None
  }

  fn calc_weekday_location(&self, date: Date) -> usize {
    let weekday = date.weekday().num_days_from_sunday() as usize;
    // avoid overflow
    (Self::DAYS_PER_WEEK + weekday - self.weekday_offset) % Self::DAYS_PER_WEEK
  }
}

fn same_month(d1: Date, d2: Date) -> bool {
  d1.year() == d2.year() && d1.month() == d2.month()
}

fn month_offset(date: Date, num_months: i32) -> Date {
  if num_months == 0 {
    return date;
  }

  if num_months > 0 {
    let date = end_of_month(date) + Duration::days(1);
    month_offset(date, num_months - 1)
  } else {
    let date = beginning_of_month(date) - Duration::days(1);
    month_offset(date, num_months + 1)
  }
}
