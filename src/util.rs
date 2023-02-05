pub use anyhow::{anyhow, bail, ensure, Result};

use chrono::{Datelike, Duration, FixedOffset, Local, Offset, Utc};

pub type DateTime = chrono::DateTime<FixedOffset>;
pub type Date = chrono::NaiveDate;
pub type Shared<T> = std::sync::Arc<std::sync::Mutex<T>>;

pub fn shared<T>(t: T) -> Shared<T> {
  std::sync::Arc::new(std::sync::Mutex::new(t))
}

pub(crate) fn now(tz: &FixedOffset) -> DateTime {
  local_now().with_timezone(tz)
}

pub(crate) fn today(tz: &FixedOffset) -> Date {
  now(tz).date_naive()
}

pub(crate) fn utc_now() -> DateTime {
  let now = Utc::now();
  now.with_timezone(&now.offset().fix())
}

pub(crate) fn local_tz() -> FixedOffset {
  let now = Local::now();
  now.offset().fix()
}

pub(crate) fn local_now() -> DateTime {
  let now = Local::now();
  now.with_timezone(now.offset())
}

// return if the times were been swapped
pub fn reorder_times(t1: &mut DateTime, t2: &mut DateTime) -> bool {
  if t1 < t2 {
    return false;
  }
  std::mem::swap(t1, t2);
  true
}

pub fn on_the_same_day(mut t1: DateTime, mut t2: DateTime) -> bool {
  if t1.date_naive() == t2.date_naive() {
    return true;
  }

  if t2 < t1 {
    std::mem::swap(&mut t1, &mut t2);
  }

  let t1_midnight = (t1.date_naive() + one_day())
    .and_hms_opt(0, 0, 0)
    .expect("date overflow");
  if t1_midnight == t2.naive_local() {
    // to midnight
    return true;
  }

  false
}

// can't be a constant because chrono::Duration constructors are not
// declared as const functions.
pub fn one_day() -> Duration {
  Duration::days(1)
}

pub fn beginning_of_month(date: Date) -> Date {
  let bom_date = chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), 1);
  bom_date.expect("date overflow")
}

pub fn end_of_month(date: Date) -> Date {
  let (year, month) = if date.month() == 12 {
    (date.year() + 1, 1)
  } else {
    (date.year(), date.month() + 1)
  };

  let bom_next_month =
    chrono::NaiveDate::from_ymd_opt(year, month, 1).expect("date overflow");

  bom_next_month - Duration::days(1)
}
