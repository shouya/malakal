use derive_builder::Builder;
use jiff::{Timestamp, tz::TimeZone};

use crate::util::seconds_since_midnight;

const SECS_PER_DAY: i64 = 24 * 3600;
pub type EventId = String;

#[derive(Builder, Clone, Debug, PartialEq)]
#[builder(try_setter, setter(into))]
pub struct Event {
  pub id: EventId,
  pub calendar: String,
  pub title: String,

  pub start: Timestamp,
  pub end: Timestamp,

  // RFC 5545 DTSTAMP field
  #[builder(default = "Timestamp::now()")]
  pub timestamp: Timestamp,

  #[builder(default = "Timestamp::now()")]
  pub created_at: Timestamp,

  #[builder(default = "Timestamp::now()")]
  pub modified_at: Timestamp,

  #[builder(default)]
  pub description: Option<String>,

  #[builder(default = "[0.3; 3]")]
  pub color: [f32; 3],

  #[builder(default = "false", setter(skip))]
  pub(crate) deleted: bool,

  #[builder(default = "false", setter(skip))]
  pub(crate) changed: bool,
}

impl Event {
  pub(crate) fn start_position_of_day(&self, tz: &TimeZone) -> f32 {
    let seconds = seconds_since_midnight(tz.to_datetime(self.start));
    (seconds as f32 / SECS_PER_DAY as f32).clamp(0.0, 1.0)
  }

  pub(crate) fn mark_changed(&mut self) {
    self.modified_at = Timestamp::now();
    self.changed = true;
  }

  pub(crate) fn mark_deleted(&mut self) {
    self.modified_at = Timestamp::now();
    self.deleted = true;
  }

  pub(crate) fn reset_dirty_flags(&mut self) {
    self.deleted = false;
    self.changed = false;
  }
}
