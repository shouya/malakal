use derive_builder::Builder;
use eframe::egui::{self, Rect, RichText, Ui};
use jiff::{ToSpan, civil::Date};

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
      .num_columns(Self::DAYS_PER_WEEK + 1)
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
        self.date -= 1.month();
      }

      ui.label(self.date.strftime("%Y-%m").to_string());

      if ui.button(">>").clicked() {
        self.date += 1.month();
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

    let bom = self.date.first_of_month();
    let eom = self.date.last_of_month();

    let days_form_previous_month = self.calc_weekday_location(bom);
    let days_from_next_month =
      Self::DAYS_PER_WEEK - self.calc_weekday_location(eom);

    let total_days = days_form_previous_month
      + days_from_next_month
      + (eom - bom).get_days() as usize;

    let mut date = bom - (days_form_previous_month as i64).days();

    // draw days of the previous month
    for i in 0..total_days {
      let col = i % Self::DAYS_PER_WEEK;

      action = action.or_else(|| self.draw_day(ui, date));
      if col == Self::DAYS_PER_WEEK - 1 {
        ui.end_row();
      }

      date += 1.day();
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
    let weekday = date.weekday().to_sunday_zero_offset() as usize;
    // avoid overflow
    (Self::DAYS_PER_WEEK + weekday - self.weekday_offset) % Self::DAYS_PER_WEEK
  }
}

fn same_month(d1: Date, d2: Date) -> bool {
  d1.year() == d2.year() && d1.month() == d2.month()
}
