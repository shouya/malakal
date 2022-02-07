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
  #[builder(default = "Local::now()")]
  pub created_at: DateTime<Local>,

  #[builder(default)]
  pub description: Option<String>,

  #[builder(default = "[0.3; 3]")]
  pub color: [f32; 3],

  #[builder(default, setter(skip))]
  pub(crate) updated_title: Option<String>,
  #[builder(default = "false", setter(skip))]
  pub(crate) pending_deletion: bool,
}
