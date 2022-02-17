pub use anyhow::{anyhow, bail, ensure, Result};
use chrono::{FixedOffset, Local, Offset, Utc};

pub type DateTime = chrono::DateTime<FixedOffset>;
pub type Date = chrono::Date<FixedOffset>;

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

pub(crate) fn local_now() -> DateTime {
  let now = Local::now();
  now.with_timezone(now.offset())
}

pub(crate) fn local_today() -> Date {
  let today = Local::today();
  today.with_timezone(today.offset())
}
