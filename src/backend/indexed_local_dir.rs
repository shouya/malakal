use chrono::{DateTime, Timelike, Utc};
use rusqlite::{params, Connection};
use std::time::Duration;
use std::{cell::RefCell, fs::Metadata, path::Path, time::Instant};

use crate::{
  backend::Backend,
  event::{Event, EventId},
  util::Result,
};

use super::LocalDir;

pub struct IndexedLocalDir {
  backend: LocalDir,
  conn: RefCell<Connection>,
  refresh_interval: Duration,
  next_refresh_at: RefCell<Instant>,
}

struct ICSFileEntry {
  size: usize,
  modified_at: chrono::DateTime<Utc>,
}

impl IndexedLocalDir {
  pub fn new<P: AsRef<Path>>(backend: LocalDir, index_path: P) -> Result<Self> {
    let conn = Connection::open(index_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "temp_store", "memory")?;
    conn.pragma_update(None, "synchronous", "normal")?;
    conn.pragma_update(None, "mmap_size", 30_000_000)?;

    let conn = RefCell::new(conn);
    let refresh_interval = Duration::from_secs(60);
    let next_refresh_at = RefCell::new(Instant::now() + refresh_interval);
    let new_self = Self {
      backend,
      conn,
      refresh_interval,
      next_refresh_at,
    };

    new_self.create_table()?;
    new_self.force_refresh()?;

    Ok(new_self)
  }

  fn get_single_event_entry(
    &self,
    conn: &Connection,
    event_id: &str,
  ) -> Result<ICSFileEntry> {
    let mut stmt = conn.prepare_cached(
      "
SELECT content_length, modification_date
FROM events
WHERE event_id = ?
LIMIT 1
",
    )?;

    stmt
      .query_row(params![event_id], |row| {
        Ok(ICSFileEntry {
          size: row.get(0)?,
          modified_at: from_unix_timestamp(row.get(1)?),
        })
      })
      .map_err(Into::into)
  }

  fn delete_event_entry(
    &self,
    conn: &Connection,
    event_id: &EventId,
  ) -> Result<()> {
    conn.execute("DELETE FROM events WHERE event_id = ?", params![event_id])?;

    Ok(())
  }

  fn all_event_entry_ids(&self, conn: &Connection) -> Result<Vec<EventId>> {
    let mut stmt = conn.prepare_cached("SELECT event_id FROM events")?;
    let event_ids = stmt
      .query_map([], |row| row.get::<_, EventId>(0))?
      .into_iter()
      .filter_map(|x| x.ok())
      .collect();
    Ok(event_ids)
  }

  pub fn create_table(&self) -> Result<()> {
    log::debug!("Creating index table");
    self.conn.borrow().execute_batch(
      "
BEGIN;
CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  event_id TEXT NOT NULL UNIQUE,
  start INTEGER NOT NULL,
  end INTEGER NOT NULL,
  content_length INTEGER NOT NULL,
  modification_date INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS events_id ON events (event_id);
CREATE INDEX IF NOT EXISTS events_start ON events (start);
COMMIT;
",
    )?;

    Ok(())
  }

  fn upsert(
    &self,
    conn: &Connection,
    event: &Event,
    metadata: &Metadata,
  ) -> Result<()> {
    let event_id = &event.id;
    let start = event.start.timestamp();
    let end = &event.end.timestamp();
    let length = metadata.len() as usize;
    let modification_date = metadata.modified()?;
    let modification_timestamp = modification_date
      .duration_since(std::time::SystemTime::UNIX_EPOCH)?
      .as_secs();

    let mut stmt = conn.prepare_cached(
      "
INSERT INTO events (event_id, start, end, content_length, modification_date)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(event_id)
DO UPDATE SET start=?2, end=?3, content_length=?4, modification_date=?5
",
    )?;

    stmt.execute(params![
      event_id,
      start,
      end,
      length,
      modification_timestamp
    ])?;

    Ok(())
  }

  fn refresh(&self) {
    if Instant::now() < *self.next_refresh_at.borrow() {
      return;
    }

    if let Err(e) = self.force_refresh() {
      log::error!("Failed refreshing {:?}", e);
    }

    *self.next_refresh_at.borrow_mut() = Instant::now() + self.refresh_interval;
  }

  fn force_refresh(&self) -> Result<()> {
    self.refresh_updated_files()?;
    self.refresh_deleted_files()?;
    Ok(())
  }

  fn refresh_updated_files(&self) -> Result<()> {
    let mut conn = self.conn.borrow_mut();
    let tx = conn.transaction()?;

    for file_entry in self.backend.all_event_file_entries() {
      let path = file_entry.path();
      let metadata = file_entry.metadata().unwrap();
      let file_stem = path.file_stem().unwrap();
      let event_id = file_stem.to_str().unwrap();
      if let Ok(event_entry) = self.get_single_event_entry(&tx, event_id) {
        let file_size = metadata.len() as usize;
        let mut mod_time: DateTime<Utc> = metadata
          .modified()
          .expect("modification date not available")
          .into();

        // we only care about second-level modification time
        mod_time = mod_time
          .with_nanosecond(0)
          .expect("failed trimming sub-second units");

        if event_entry.size != file_size || event_entry.modified_at < mod_time {
          log::debug!("Updating existing event {:?}", path);
          self.update_event_entry(&tx, path)?;
        }
      } else {
        log::debug!("Creating new event {:?}", path);
        self.create_event_entry(&tx, path)?;
      }
    }

    tx.commit()?;

    Ok(())
  }

  fn refresh_deleted_files(&self) -> Result<()> {
    let mut conn = self.conn.borrow_mut();
    let tx = conn.transaction()?;
    for event_id in self.all_event_entry_ids(&tx)? {
      let path = self.backend.event_path(&event_id);
      if !path.exists() {
        log::debug!("Deleting event {:?}", event_id);
        self.delete_event_entry(&tx, &event_id)?;
      }
    }

    tx.commit()?;

    Ok(())
  }

  fn create_event_entry<P: AsRef<Path>>(
    &self,
    conn: &Connection,
    file: P,
  ) -> Result<()> {
    self.update_event_entry(conn, file)
  }

  fn update_event_entry<P: AsRef<Path>>(
    &self,
    conn: &Connection,
    file: P,
  ) -> Result<()> {
    let metadata = file.as_ref().metadata().unwrap();
    let event = self.backend.parse_event(file)?;
    self.upsert(conn, &event, &metadata)
  }

  fn all_event_entry_ids_between(
    &self,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
  ) -> Result<Vec<EventId>> {
    let start = from.timestamp();
    let end = to.timestamp();

    let conn = self.conn.borrow();
    let mut stmt = conn.prepare_cached(
      "SELECT event_id FROM events WHERE start >= ? AND end <= ?",
    )?;
    let event_ids = stmt
      .query_map([start, end], |row| row.get::<_, EventId>(0))?
      .into_iter()
      .filter_map(|x| x.ok())
      .collect();
    Ok(event_ids)
  }
}

impl Backend for IndexedLocalDir {
  fn get_events(
    &self,
    from: chrono::DateTime<chrono::Local>,
    to: chrono::DateTime<chrono::Local>,
  ) -> Result<Vec<Event>> {
    self.refresh();

    let event_ids = self.all_event_entry_ids_between(
      from.with_timezone(&Utc),
      to.with_timezone(&Utc),
    )?;

    let events = event_ids.into_iter().filter_map(|id| {
      let path = self.backend.event_path(&id);
      self.backend.parse_event(path).ok()
    });

    Ok(events.collect())
  }

  fn delete_event(&mut self, event_id: &EventId) -> Result<()> {
    self.backend.delete_event(event_id)?;
    self.delete_event_entry(&self.conn.borrow(), event_id)?;
    Ok(())
  }

  fn update_event(&mut self, event: &Event) -> Result<()> {
    self.backend.update_event(event)?;
    let path = self.backend.event_path(&event.id);
    self.update_event_entry(&self.conn.borrow(), path)?;
    Ok(())
  }

  fn create_event(&mut self, event: &Event) -> Result<()> {
    self.backend.create_event(event)?;
    let path = self.backend.event_path(&event.id);
    self.create_event_entry(&self.conn.borrow(), path)
  }

  fn get_event(&self, event_id: &EventId) -> Result<Event> {
    self.backend.get_event(event_id)
  }
}

fn from_unix_timestamp(i: i64) -> DateTime<Utc> {
  use std::time::UNIX_EPOCH;
  let d = UNIX_EPOCH + Duration::from_secs(i as u64);
  DateTime::<Utc>::from(d)
}
