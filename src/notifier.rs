use chrono::Duration;
use notify_rust::Notification;
use sysinfo::{ProcessExt, System, SystemExt};
use timer::Timer;

use crate::backend::Backend;
use crate::event::Event;
use crate::util::{self, shared, utc_now, Result, Shared};
use crate::Config;

pub struct Notifier {
  // this guard is only used to bind the lifetime of the guard to a
  // notifier, not to be read.
  #[allow(dead_code)]
  reschedule_guard: timer::Guard,
  context: Shared<NotifierContext>,
}

struct NotifierContext {
  timer: Timer,
  guards: Vec<timer::Guard>,
  switch: bool,
  blacklist_processes: Vec<String>,
  reschedule_interval: Duration,
  backend: Shared<dyn Backend>,
}

impl NotifierContext {
  fn new(config: &Config, backend: &Shared<dyn Backend>) -> Result<Self> {
    Ok(Self {
      timer: Timer::new(),
      guards: vec![],
      switch: config.notifier_switch.unwrap_or(true),
      blacklist_processes: config.notifier_blacklist_processes.clone(),
      backend: backend.clone(),
      reschedule_interval: Self::reschedule_interval(),
    })
  }

  // should be const but chrono::from_std is not declared as const
  fn reschedule_interval() -> Duration {
    Duration::from_std(std::time::Duration::from_secs(3600 * 24)).unwrap()
  }

  fn start_rescheduler(shared_context: Shared<Self>) -> timer::Guard {
    let cloned_context = shared_context.clone();
    let context_locked = shared_context.lock().unwrap();
    context_locked
      .timer
      .schedule_repeating(context_locked.reschedule_interval, move || {
        Self::reschedule_events(cloned_context.clone())
      })
  }

  fn reschedule_events(shared_context: Shared<Self>) {
    let mut context = shared_context.lock().unwrap();
    let shared_backend = context.backend.clone();
    let mut backend = shared_backend.lock().unwrap();

    let now = util::utc_now();
    let until = now + context.reschedule_interval;
    let events: Vec<_> = backend
      .get_events(now, until)
      .into_iter()
      .flatten()
      .collect();
    drop(backend);

    context.guards.clear();
    for event in events {
      if event.start < utc_now() {
        continue;
      }

      let notify_at = event.start;
      let shared_context = shared_context.clone();
      let guard = context.timer.schedule_with_date(notify_at, move || {
        Self::notify(shared_context.clone(), event.clone())
      });

      context.guards.push(guard);
    }
  }

  fn notify(context: Shared<Self>, event: Event) {
    let context = context.lock().unwrap();
    if !context.switch {
      return;
    }

    if blacklist_process_running(&context.blacklist_processes) {
      return;
    }

    Notification::new()
      .summary(&event.title)
      .appname("malakal")
      .show()
      .unwrap();
  }
}

impl Notifier {
  pub fn start(config: &Config, backend: &Shared<dyn Backend>) -> Result<Self> {
    let context = shared(NotifierContext::new(config, backend)?);
    let reschedule_guard = NotifierContext::start_rescheduler(context.clone());
    let notifier = Self {
      reschedule_guard,
      context,
    };
    notifier.events_updated();
    Ok(notifier)
  }

  pub fn events_updated(&self) {
    NotifierContext::reschedule_events(self.context.clone());
  }
}

fn blacklist_process_running(blacklist: &[String]) -> bool {
  let system = System::default();
  for process in system.processes().values() {
    let name = process.name();
    if blacklist.iter().any(|black| black == name) {
      return true;
    }
  }

  false
}
