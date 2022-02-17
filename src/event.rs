use chrono::FixedOffset;
use derive_builder::Builder;

use crate::util::{now, DateTime};

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
  #[builder(default = "now()")]
  pub timestamp: DateTime,

  #[builder(default = "now()")]
  pub created_at: DateTime,
  #[builder(default = "now()")]
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
  pub(crate) fn mark_changed(&mut self) {
    self.modified_at = now();
    self.changed = true;
  }

  pub(crate) fn mark_deleted(&mut self) {
    self.modified_at = now();
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
