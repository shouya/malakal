use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::time::Duration;
use std::{cell::RefCell, fs::Metadata, path::Path, time::Instant};

use crate::{
  backend::Backend,
  event::{Event, EventId},
};

use super::LocalDir;

pub struct IndexedLocalDir {
  backend: LocalDir,
  conn: Connection,
  refresh_interval: Duration,
  next_refresh_at: RefCell<Instant>,
}

struct ICSFileEntry {
  size: usize,
  modified_at: chrono::DateTime<Utc>,
}

impl IndexedLocalDir {
  pub fn new<P: AsRef<Path>>(backend: LocalDir, index_path: P) -> Option<Self> {
    let conn = Connection::open(index_path).ok()?;
    let refresh_interval = Duration::from_secs(60);
    let next_refresh_at = RefCell::new(Instant::now() + refresh_interval);
    Some(Self {
      backend,
      conn,
      refresh_interval,
      next_refresh_at,
    })
  }

  fn get_single_event_entry(&self, event_id: &str) -> Option<ICSFileEntry> {
    self
      .conn
      .query_row(
        "
SELECT content_length, modification_date
FROM events
WHERE event_id = ?
LIMIT 1
",
        params![event_id],
        |row| {
          Ok(ICSFileEntry {
            size: row.get(0)?,
            modified_at: from_unix_timestamp(row.get(1)?),
          })
        },
      )
      .ok()
  }

  fn delete_event_entry(&self, event_id: &EventId) -> Option<()> {
    self
      .conn
      .execute("DELETE FROM events WHERE event_id = ?", params![event_id])
      .ok()?;

    Some(())
  }

  fn all_event_entry_ids(&self) -> Option<Vec<EventId>> {
    let mut stmt = self.conn.prepare("SELECT event_id FROM events").ok()?;
    let event_ids = stmt
      .query_map([], |row| row.get::<_, EventId>(0))
      .ok()?
      .into_iter()
      .filter_map(|x| x.ok())
      .collect();
    Some(event_ids)
  }

  pub fn create_table(&self) -> Option<()> {
    self
      .conn
      .execute(
        "
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id TEXT PRIMARY KEY NOT NULL,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  content_length INTEGER,
  modification_date INTEGER NOT NULL
);
",
        [],
      )
      .ok()?;

    Some(())
  }

  fn upsert(&self, event: &Event, metadata: &Metadata) -> Option<()> {
    let event_id = &event.id;
    let start = event.start.timestamp();
    let end = &event.end.timestamp();
    let length = metadata.len() as usize;
    let modification_date = metadata.modified().ok()?;
    let modification_timestamp = modification_date
      .duration_since(std::time::SystemTime::UNIX_EPOCH)
      .ok()?
      .as_secs();

    self
      .conn
      .execute(
        "
INSERT INTO events (event_id, start, end, content_length, modification_date)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(event_id)
DO UPDATE SET start=?2, end=?3, content_length=?4, modification_date=?5
",
        params![event_id, start, end, length, modification_timestamp],
      )
      .ok()?;

    Some(())
  }

  fn refresh(&self) {
    if Instant::now() < *self.next_refresh_at.borrow() {
      return;
    }

    self.force_refresh();

    *self.next_refresh_at.borrow_mut() = Instant::now() + self.refresh_interval;
  }

  fn force_refresh(&self) {
    self.refresh_updated_files();
    self.refresh_deleted_files();
  }

  fn refresh_updated_files(&self) {
    for file_entry in self.backend.all_event_file_entries() {
      let path = file_entry.path();
      let metadata = file_entry.metadata().unwrap();
      let file_stem = path.file_stem().unwrap();
      let event_id = file_stem.to_str().unwrap();
      if let Some(event_entry) = self.get_single_event_entry(event_id) {
        let file_size = metadata.len() as usize;
        let mod_time: DateTime<Utc> = metadata.modified().unwrap().into();

        if event_entry.size != file_size || event_entry.modified_at < mod_time {
          self.update_event_entry(path);
        }
      } else {
        self.create_event_entry(path);
      }
    }
  }

  fn refresh_deleted_files(&self) -> Option<()> {
    for event_id in self.all_event_entry_ids()? {
      let path = self.backend.event_path(&event_id);
      if !path.exists() {
        self.delete_event_entry(&event_id)?;
      }
    }

    Some(())
  }

  fn create_event_entry<P: AsRef<Path>>(&self, file: P) -> Option<()> {
    self.update_event_entry(file)
  }

  fn update_event_entry<P: AsRef<Path>>(&self, file: P) -> Option<()> {
    let metadata = file.as_ref().metadata().unwrap();
    let event = self.backend.parse_event(file)?;
    self.upsert(&event, &metadata)
  }

  fn all_event_entry_ids_between(
    &self,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
  ) -> Option<Vec<EventId>> {
    let start = from.timestamp();
    let end = to.timestamp();

    let mut stmt = self
      .conn
      .prepare("SELECT event_id FROM events WHERE start >= ? AND end <= ?")
      .ok()?;
    let event_ids = stmt
      .query_map([start, end], |row| row.get::<_, EventId>(0))
      .ok()?
      .into_iter()
      .filter_map(|x| x.ok())
      .collect();
    Some(event_ids)
  }
}

impl Backend for IndexedLocalDir {
  fn get_events(
    &self,
    from: chrono::DateTime<chrono::Local>,
    to: chrono::DateTime<chrono::Local>,
  ) -> Option<Vec<Event>> {
    self.refresh();

    let event_ids = self.all_event_entry_ids_between(
      from.with_timezone(&Utc),
      to.with_timezone(&Utc),
    )?;

    let events = event_ids.into_iter().filter_map(|id| {
      let path = self.backend.event_path(&id);
      self.backend.parse_event(path)
    });

    Some(events.collect())
  }

  fn delete_event(&mut self, event_id: &EventId) -> Option<()> {
    self.backend.delete_event(event_id);
    self.delete_event_entry(event_id);
    Some(())
  }

  fn update_event(&mut self, event: &Event) -> Option<()> {
    self.backend.update_event(event)?;
    let path = self.backend.event_path(&event.id);
    self.update_event_entry(path)
  }

  fn create_event(&mut self, event: &Event) -> Option<()> {
    self.backend.create_event(event)?;
    let path = self.backend.event_path(&event.id);
    self.create_event_entry(path)
  }

  fn get_event(&self, event_id: &EventId) -> Option<Event> {
    self.backend.get_event(event_id)
  }
}

fn from_unix_timestamp(i: i64) -> DateTime<Utc> {
  use std::time::UNIX_EPOCH;
  let d = UNIX_EPOCH + Duration::from_secs(i as u64);
  DateTime::<Utc>::from(d)
}
