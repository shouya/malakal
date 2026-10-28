use std::{
  sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
  thread,
};

use chrono::Duration;
use log::warn;
use notify_rust::Notification;
use sysinfo::System;

use crate::Config;
use crate::backend::Backend;
use crate::event::Event;
use crate::util::{Result, Shared, utc_now};

// Upcoming events are reloaded at least this often.
const LOOKAHEAD_HOURS: i64 = 24;

pub struct Notifier {
  events_updated: Sender<()>,
}

impl Notifier {
  pub fn start(config: &Config, backend: &Shared<dyn Backend>) -> Self {
    let (events_updated, rx) = mpsc::channel();
    let mut alarm = Alarm {
      switch: config.notifier_switch,
      blacklist_processes: config.notifier_blacklist_processes.clone(),
      notification_timeout: config.notification_timeout,
      backend: backend.clone(),
      upcoming: vec![],
      reload_at: utc_now(),
    };
    thread::spawn(move || alarm.run(&rx));

    Self { events_updated }
  }

  pub fn events_updated(&self) {
    let _ = self.events_updated.send(());
  }
}

struct Alarm {
  switch: bool,
  blacklist_processes: Vec<String>,
  notification_timeout: Duration,
  backend: Shared<dyn Backend>,

  // sorted by start time, latest first
  upcoming: Vec<Event>,
  reload_at: crate::util::DateTime,
}

impl Alarm {
  fn run(&mut self, events_updated: &Receiver<()>) {
    self.reload();

    loop {
      match events_updated.recv_timeout(self.time_until_wake_up()) {
        Ok(()) => self.reload(),
        Err(RecvTimeoutError::Timeout) => self.wake_up(),
        Err(RecvTimeoutError::Disconnected) => return,
      }
    }
  }

  fn time_until_wake_up(&self) -> std::time::Duration {
    let next_start = self.upcoming.last().map(|event| event.start);
    let wake_up_at =
      next_start.map_or(self.reload_at, |t| t.min(self.reload_at));
    (wake_up_at - utc_now()).to_std().unwrap_or_default()
  }

  fn wake_up(&mut self) {
    let now = utc_now();
    while let Some(event) = self.upcoming.pop_if(|event| event.start <= now) {
      if let Err(e) = self.notify(&event) {
        warn!("failed sending notification {e:?}");
      }
    }

    if self.reload_at <= now {
      self.reload();
    }
  }

  fn reload(&mut self) {
    let now = utc_now();
    let until = now + Duration::hours(LOOKAHEAD_HOURS);
    let events = self.backend.lock().unwrap().get_events(now, until);

    let mut upcoming = match events {
      Ok(events) => events,
      Err(e) => {
        warn!("failed loading events for notification {e:?}");
        vec![]
      }
    };
    upcoming.retain(|event| event.start >= now);
    upcoming.sort_by_key(|event| std::cmp::Reverse(event.start));

    self.upcoming = upcoming;
    self.reload_at = until;
  }

  fn notify(&self, event: &Event) -> Result<()> {
    if !self.switch {
      return Ok(());
    }

    if blacklist_process_running(&self.blacklist_processes) {
      return Ok(());
    }

    Notification::new()
      .summary(&event.title)
      .appname("malakal")
      .auto_icon()
      .timeout(self.notification_timeout.to_std()?)
      .show()?;

    Ok(())
  }
}

fn blacklist_process_running(blacklist: &[String]) -> bool {
  let system = System::default();
  for process in system.processes().values() {
    let name = process.name();
    if blacklist.iter().any(|black| **black == *name) {
      return true;
    }
  }

  false
}
