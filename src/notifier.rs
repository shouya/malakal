use std::sync::{Arc, Mutex};

use chrono::Duration;
use timer::Timer;

use crate::backend::Backend;
use crate::util::{Result, Shared};
use crate::Config;

pub struct Notifier {
  reschedule_guard: timer::Guard,
  state: Shared<SwitchState>,
  timer: Shared<NotifierTimer>,
  backend: Shared<dyn Backend>,
}

struct SwitchState {
  switch: bool,
  disabled: bool,
}

struct NotifierTimer {
  timer: Timer,
  guards: Vec<timer::Guard>,
}

impl NotifierTimer {
  fn new() -> Result<Self> {
    Ok(Self {
      timer: Timer::new(),
      guards: vec![],
    })
  }
}

impl Notifier {
  fn start(config: &Config, backend: Shared<dyn Backend>) -> Result<Self> {
    let state = Arc::new(Mutex::new({
      SwitchState {
        switch: config.notifier_switch.unwrap_or(true),
        disabled: false,
      }
    }));
    let timer = Arc::new(Mutex::new(NotifierTimer::new()?));

    let reschedule_guard = {
      let backend = backend.clone();
      let locked_timer = timer.lock().unwrap();
      let timer = timer.clone();
      locked_timer.timer.schedule_repeating(
        Self::reschedule_interval(),
        move || {
          let backend = backend.lock().unwrap();
          let mut timer = timer.lock().unwrap();
          Self::reschedule_events(&mut timer, &*backend);
        },
      )
    };

    Ok(Self {
      state,
      backend,
      timer,
      reschedule_guard,
    })
  }

  fn reschedule_events(_timer: &mut NotifierTimer, _backend: &dyn Backend) {}

  // should be const but chrono::from_std is not declared as const
  fn reschedule_interval() -> Duration {
    Duration::from_std(std::time::Duration::from_secs(3600 * 24)).unwrap()
  }
}
