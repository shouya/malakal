pub use anyhow::{Result, anyhow};

use jiff::{
  Timestamp, ToSpan,
  civil::{Date, DateTime, Time},
  tz::TimeZone,
};

pub type Shared<T> = std::sync::Arc<std::sync::Mutex<T>>;

pub fn shared<T>(t: T) -> Shared<T> {
  std::sync::Arc::new(std::sync::Mutex::new(t))
}

pub(crate) fn today(tz: &TimeZone) -> Date {
  tz.to_datetime(Timestamp::now()).date()
}

// return if the times were been swapped
pub fn reorder_times(t1: &mut Timestamp, t2: &mut Timestamp) -> bool {
  if t1 < t2 {
    return false;
  }
  std::mem::swap(t1, t2);
  true
}

pub fn on_the_same_day(t1: Timestamp, t2: Timestamp, tz: &TimeZone) -> bool {
  let (earlier, later) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
  let earlier = tz.to_datetime(earlier);
  let later = tz.to_datetime(later);

  if earlier.date() == later.date() {
    return true;
  }

  let next_midnight = (earlier.date() + 1.day()).to_datetime(Time::midnight());
  later == next_midnight
}

pub fn add_days(t: Timestamp, days: i64, tz: &TimeZone) -> Timestamp {
  let local = tz.to_datetime(t) + days.days();
  tz.to_ambiguous_timestamp(local)
    .compatible()
    .expect("date overflow")
}

pub fn seconds_since_midnight(local: DateTime) -> i64 {
  local.time().duration_since(Time::midnight()).as_secs()
}
