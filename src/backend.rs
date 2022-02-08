mod local_dir;

use super::event::{Event, EventId};

use chrono::{DateTime, Local};

trait Backend {
  fn get_event(&self, event_id: &EventId) -> Option<Event>;

  // get events which overlap with the from..to interval.
  fn get_events(
    &self,
    from: DateTime<Local>,
    to: DateTime<Local>,
  ) -> Option<Vec<Event>>;

  fn delete_event(&mut self, event_id: &EventId) -> Option<()>;

  fn update_event(&mut self, updated_event: Event) -> Option<()>;

  // create a event

  fn create_event(&mut self, event: Event) -> Option<()>;
}
