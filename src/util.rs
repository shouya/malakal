pub use anyhow::{anyhow, bail, ensure, Result};
use chrono::{FixedOffset, Local};

pub type DateTime = chrono::DateTime<FixedOffset>;
pub type Date = chrono::Date<FixedOffset>;

pub(crate) fn now() -> DateTime {
  let now = Local::now();
  now.with_timezone(now.offset())
}

pub(crate) fn today() -> Date {
  let today = Local::today();
  today.with_timezone(today.offset())
}
