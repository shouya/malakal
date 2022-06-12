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
  pub notifier_switch: Option<bool>,
  pub notifier_blacklist_processes: Vec<String>,
  #[serde_as(as = "serde_with::DurationMilliSeconds<i64, Flexible>")]
  pub notification_timeout: Duration,
}

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");

impl Default for Config {
  fn default() -> Self {
    Self {
      calendar_name: "malakal".into(),
      calendar_location: "~/.calendar/malakal".into(),
      timezone: None,
      notifier_switch: Some(true),
      notification_timeout: Duration::seconds(2000),
      notifier_blacklist_processes: vec![],
    }
  }
}

impl Config {
  pub fn read_or_initialize() -> anyhow::Result<Config> {
    let xdg = xdg::BaseDirectories::new()?;
    let config_file = xdg
      .place_config_file(format!("{APP_NAME}/config.toml"))
      .with_context(|| "cannot find xdg config directory")?;

    log::info!("Loading config from {}", config_file.display());

    let dir = config_file.parent().ok_or_else(|| {
      anyhow!("Invalid config_file location: {config_file:?}")
    })?;

    if !dir.exists() {
      std::fs::create_dir_all(dir)?;
      let default_conf = to_string_pretty(&Config::default())?;
      std::fs::write(&config_file, default_conf)?;
    }

    Ok(toml::from_slice(&std::fs::read(config_file)?)?)
  }
}
