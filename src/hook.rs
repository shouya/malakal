use std::{
  process::Command,
  sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
  thread,
  time::Duration,
};

use crate::config::Config;

pub struct HookExecutor {
  updated: Sender<()>,
}

impl HookExecutor {
  pub fn new(config: &Config) -> Self {
    let (updated, rx) = mpsc::channel();

    if let Some(command) = config.post_update_hook.clone() {
      let delay = config.post_update_hook_delay;
      thread::spawn(move || run_debounced(&rx, delay, &command));
    }

    Self { updated }
  }

  pub fn report_updated(&self) {
    let _ = self.updated.send(());
  }
}

fn run_debounced(rx: &Receiver<()>, delay: Duration, command: &[String]) {
  while rx.recv().is_ok() {
    if !wait_until_quiet(rx, delay) {
      return;
    }
    run_command(command);
  }
}

fn wait_until_quiet(rx: &Receiver<()>, delay: Duration) -> bool {
  loop {
    match rx.recv_timeout(delay) {
      Ok(()) => continue,
      Err(RecvTimeoutError::Timeout) => return true,
      Err(RecvTimeoutError::Disconnected) => return false,
    }
  }
}

fn run_command(command: &[String]) {
  let Some((program, args)) = command.split_first() else {
    log::warn!("post_update_hook is empty");
    return;
  };

  let result = Command::new(program).args(args).status();
  if let Err(e) = result {
    log::warn!("failed running post_update_hook: {e:?}");
  }
}
