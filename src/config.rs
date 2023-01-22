use std::path::PathBuf;

use anyhow::{anyhow, Context};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_with::{formats::Flexible, serde_as};
use toml::ser::to_string_pretty;

#[serde_as]
#[derive(Deserialize, Serialize, Debug)]
#[serde(default)]
pub struct Config {
  pub calendar_name: String,
  pub calendar_location: String,
  pub timezone: Option<String>,
  pub notifier_switch: bool,
  pub notifier_blacklist_processes: Vec<String>,
  #[serde_as(as = "serde_with::DurationMilliSeconds<i64, Flexible>")]
  pub notification_timeout: Duration,
  pub post_update_hook: Option<Vec<String>>,
  #[serde_as(as = "serde_with::DurationMilliSeconds<i64, Flexible>")]
  pub post_update_hook_delay: Duration,
}

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

impl Default for Config {
  fn default() -> Self {
    Self {
      calendar_name: "malakal".into(),
      calendar_location: format!("~/.calendar/{APP_NAME}"),
      timezone: None,
      notifier_switch: true,
      notification_timeout: Duration::seconds(5),
      notifier_blacklist_processes: vec![],
      post_update_hook: None,
      post_update_hook_delay: Duration::seconds(30),
    }
  }
}

impl Config {
  pub fn normalize(&mut self) -> anyhow::Result<()> {
    self.calendar_location =
      self.calendar_location.replace('~', &std::env::var("HOME")?);

    Ok(())
  }

  pub fn read_or_initialize() -> anyhow::Result<Config> {
    let config_file = {
      let mut dir = dirs::config_dir()
        .with_context(|| "Cannot find a directory to store config")?;
      dir.push(format!("{APP_NAME}/config.toml"));
      dir
    };

    log::info!("Loading config from {}", config_file.display());

    let dir = config_file.parent().ok_or_else(|| {
      anyhow!("Invalid config_file location: {config_file:?}")
    })?;

    if !dir.exists() {
      std::fs::create_dir_all(dir)?;
    }

    if !config_file.exists() {
      log::info!("Creating default config at {config_file:?}");
      let default_conf = to_string_pretty(&Config::default())?;
      std::fs::write(&config_file, default_conf)?;
    }

    let mut config: Config = toml::from_slice(&std::fs::read(config_file)?)?;
    config.normalize()?;

    let calendar_location = PathBuf::from(config.calendar_location.as_str());
    if !calendar_location.exists() {
      log::info!("Creating calendar directory at {calendar_location:?}");
      std::fs::create_dir_all(&calendar_location)?;
    }

    Ok(config)
  }
}
