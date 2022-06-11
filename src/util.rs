pub use anyhow::{anyhow, bail, ensure, Result};
use chrono::{Duration, FixedOffset, Local, Offset, Utc};

pub type DateTime = chrono::DateTime<FixedOffset>;
pub type Date = chrono::Date<FixedOffset>;
pub type Shared<T> = std::sync::Arc<std::sync::Mutex<T>>;

pub(crate) fn now(tz: &FixedOffset) -> DateTime {
  local_now().with_timezone(tz)
}

pub(crate) fn today(tz: &FixedOffset) -> Date {
  local_today().with_timezone(tz)
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

pub(crate) fn local_today() -> Date {
  let today = Local::today();
  today.with_timezone(today.offset())
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

// can't be a constant because chrono::Duration constructors are not
// declared as const functions.
pub fn one_day() -> Duration {
  Duration::days(1)
}
