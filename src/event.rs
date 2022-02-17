use chrono::{DateTime, Local};
use derive_builder::Builder;

pub type EventId = String;

#[derive(Builder, Clone, Debug, PartialEq)]
#[builder(try_setter, setter(into))]
pub struct Event {
  pub id: EventId,
  pub calendar: String,
  pub title: String,

  pub start: DateTime<Local>,
  pub end: DateTime<Local>,

  // RFC 5545 DTSTAMP field
  #[builder(default = "Local::now()")]
  pub timestamp: DateTime<Local>,

  #[builder(default = "Local::now()")]
  pub created_at: DateTime<Local>,
  #[builder(default = "Local::now()")]
  pub modified_at: DateTime<Local>,

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
    self.modified_at = Local::now();
    self.changed = true;
  }

  pub(crate) fn mark_deleted(&mut self) {
    self.modified_at = Local::now();
    self.deleted = true;
  }

  pub(crate) fn reset_dirty_flags(&mut self) {
    self.deleted = false;
    self.changed = false;
  }
}
