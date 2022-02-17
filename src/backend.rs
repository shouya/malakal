mod indexed_local_dir;
mod local_dir;

use super::event::{Event, EventId};
use crate::util::{DateTime, Result};

pub use indexed_local_dir::IndexedLocalDir;
pub use local_dir::{LocalDir, LocalDirBuilder};

pub trait Backend {
  fn get_event(&mut self, event_id: &EventId) -> Result<Event>;

  // get events which overlap with the from..to interval.
  fn get_events(&mut self, from: DateTime, to: DateTime) -> Result<Vec<Event>>;

  fn delete_event(&mut self, event_id: &EventId) -> Result<()>;

  fn update_event(&mut self, updated_event: &Event) -> Result<()>;

  fn create_event(&mut self, event: &Event) -> Result<()>;

  fn force_refresh(&mut self) -> Result<()> {
    Ok(())
  }
}
