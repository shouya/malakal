use anyhow::Context;
use derive_builder::Builder;
use filetime::FileTime;
use std::{
  ffi::OsStr,
  fs::DirEntry,
  path::{Path, PathBuf},
};

use crate::{
  backend::Backend,
  event::{Event, EventId},
  ical::ICal,
  util::{DateTime, Result},
};

#[derive(Builder)]
#[builder(try_setter, setter(into))]
pub struct LocalDir {
  dir: PathBuf,
  calendar: String,
}

impl LocalDir {
  pub(crate) fn all_event_file_entries(
    &self,
  ) -> impl Iterator<Item = DirEntry> + '_ {
    let entries = self.dir.read_dir().expect("read_dir failed");
    entries
      .into_iter()
      .filter_map(|entry| entry.ok())
      .filter(|entry| entry.file_type().unwrap().is_file())
      .filter(|entry| {
        entry.path().extension().and_then(OsStr::to_str) == Some("ics")
      })
  }

  pub(crate) fn parse_event<P: AsRef<Path>>(&self, path: P) -> Result<Event> {
    let path = path.as_ref().to_owned();
    let content = std::fs::read(&path)?;
    let string = String::from_utf8(content)?;

    ICal
      .parse(&self.calendar, &string)
      .with_context(|| format!("parse ics file: {}", path.display()))
  }

  fn all_events(&self) -> impl Iterator<Item = Event> + '_ {
    self
      .all_event_file_entries()
      .filter_map(|entry| self.parse_event(entry.path()).ok())
  }

  pub(crate) fn event_path(&self, event_id: &EventId) -> PathBuf {
    let mut path = self.dir.clone();
    path.push(format!("{event_id}.ics"));
    path
  }
}

impl Backend for LocalDir {
  fn get_events(&mut self, from: DateTime, to: DateTime) -> Result<Vec<Event>> {
    let mut events = vec![];
    for event in self.all_events() {
      if event_visible_in_range(&event, from, to) {
        events.push(event);
      }
    }

    Ok(events)
  }

  fn delete_event(&mut self, event_id: &EventId) -> Result<()> {
    let path = self.event_path(event_id);
    if path.exists() {
      log::debug!("Removing event {:?}", path);
      std::fs::remove_file(path)?;
    } else {
      // TODO: log
    }

    Ok(())
  }

  fn update_event(&mut self, updated_event: &Event) -> Result<()> {
    let ics_content = ICal.generate(updated_event)?;
    let path = self.event_path(&updated_event.id);

    if !path.exists() {
      // TODO: show warning
    }

    log::debug!("Updating event {:?}", path);
    std::fs::write(path, ics_content)?;
    touch_dir(&self.dir);

    Ok(())
  }

  fn create_event(&mut self, event: &Event) -> Result<()> {
    let ics_content = ICal.generate(event)?;
    let path = self.event_path(&event.id);

    log::debug!("Creating event {:?}", path);
    std::fs::write(path, ics_content)?;

    Ok(())
  }

  fn get_event(&mut self, event_id: &EventId) -> Result<Event> {
    let path = self.event_path(event_id);
    let buffer = std::fs::read(path)?;
    let string = String::from_utf8(buffer)?;

    ICal.parse(&self.calendar, &string)
  }
}

fn event_visible_in_range(e: &Event, start: DateTime, end: DateTime) -> bool {
  e.start.max(start) <= e.end.min(end)
}

fn touch_dir(path: &Path) {
  let mtime = FileTime::now();

  // Failing is acceptable.
  match filetime::set_file_mtime(path, mtime) {
    Ok(()) => (),
    Err(e) => log::warn!("Failed updating directory mtime {path:?}: #{e:?}"),
  }
}
