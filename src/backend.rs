mod ical;
mod local;

use super::event::{Event, EventId};

use chrono::{DateTime, Local};

trait Backend {
  // get events which overlap with the from..to interval.
  fn get_events(
    &self,
    from: DateTime<Local>,
    to: DateTime<Local>,
  ) -> Option<Vec<Event>>;

  fn delete_event(&mut self, event_id: &EventId) -> Option<()>;

  // return old event if possible
  fn update_event(
    &mut self,
    event_id: &EventId,
    updated_event: Event,
  ) -> Option<Event>;

  // create a event
  fn create_event(
    &mut self,
    event_id: &EventId,
    updated_event: Event,
  ) -> Option<Event>;
}
