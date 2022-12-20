use std::{
  process::Command,
  sync::{Arc, Mutex},
};

use chrono::Duration;
use timer::{Guard, Timer};

use crate::{config::Config, util::Shared};

#[derive(Clone)]
pub struct HookExecutor {
  timer: Arc<Timer>,
  guard: Shared<Option<Guard>>,
  command: Option<Vec<String>>,
}

impl HookExecutor {
  pub fn new(config: &Config) -> Self {
    let post_update_hook = config.post_update_hook.clone();
    Self {
      timer: Arc::new(Timer::new()),
      guard: Arc::new(Mutex::new(None)),
      command: post_update_hook,
    }
  }

  pub fn report_updated(&self) {
    if let Some(cmd_and_args) = self.command.as_ref() {
      let one_min = Duration::seconds(1);
      let cmd_and_args = cmd_and_args.clone();
      let mut guard = self.guard.lock().unwrap();

      // cancel previous timer
      drop(guard.take());

      let schedule_guard = self.timer.schedule_with_delay(one_min, move || {
        let mut iter = cmd_and_args.iter();
        Command::new(iter.next().expect("Empty command"))
          .args(iter)
          .spawn()
          .expect("failed to spawn")
          .wait()
          .expect("failed to wait");
      });

      *guard = Some(schedule_guard)
    }
  }
}
