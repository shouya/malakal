use anyhow::{anyhow, Context};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
  pub calendar_name: String,
  pub calendar_location: String,
  pub timezone: Option<String>,
  pub notifier_switch: Option<bool>,
  pub notifier_blacklist_processes: Vec<String>,
}

pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const DEFAULT_CONFIG: &str = "calendar_name = \"time-blocking\"
calendar_location = \"~/.calendar/time-blocking\"
";

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
      std::fs::write(&config_file, DEFAULT_CONFIG)?;
    }

    Ok(toml::from_slice(&std::fs::read(config_file)?)?)
  }
}
