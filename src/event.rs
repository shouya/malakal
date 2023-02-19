use chrono::{FixedOffset, Offset, Timelike};
use derive_builder::Builder;

use crate::util::{now, utc_now, DateTime};

const SECS_PER_DAY: u64 = 24 * 3600;
pub type EventId = String;

#[derive(Builder, Clone, Debug, PartialEq)]
#[builder(try_setter, setter(into))]
pub struct Event {
  pub id: EventId,
  pub calendar: String,
  pub title: String,

  pub start: DateTime,
  pub end: DateTime,

  // RFC 5545 DTSTAMP field
  #[builder(default = "utc_now()")]
  pub timestamp: DateTime,

  #[builder(default = "utc_now()")]
  pub created_at: DateTime,

  #[builder(default = "utc_now()")]
  pub modified_at: DateTime,

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
  pub(crate) fn start_position_of_day(&self) -> f32 {
    (self.start.num_seconds_from_midnight() as f32 / SECS_PER_DAY as f32)
      .clamp(0.0, 1.0)
  }

  pub(crate) fn mark_changed(&mut self) {
    self.modified_at = now(&self.modified_at.offset().fix());
    self.changed = true;
  }

  pub(crate) fn mark_deleted(&mut self) {
    self.modified_at = now(&self.modified_at.offset().fix());
    self.deleted = true;
  }

  pub(crate) fn reset_dirty_flags(&mut self) {
    self.deleted = false;
    self.changed = false;
  }

  pub(crate) fn set_timezone(&mut self, tz: &FixedOffset) {
    self.created_at = self.created_at.with_timezone(tz);
    self.modified_at = self.modified_at.with_timezone(tz);
    self.timestamp = self.timestamp.with_timezone(tz);
    self.start = self.start.with_timezone(tz);
    self.end = self.end.with_timezone(tz);
  }
}
