use std::str::FromStr;

use anyhow::Context;
use chrono::{Offset, TimeZone, Utc};
use serde_derive::Deserialize;

mod app;
mod backend;
mod event;
mod ical;
mod util;
mod widget;

#[derive(Deserialize, Debug)]
struct Config {
  calendar_name: String,
  calendar_location: String,
  timezone: Option<String>,
}

const APP_NAME: &str = env!("CARGO_PKG_NAME");

fn main() -> anyhow::Result<()> {
  env_logger::init();

  let xdg = xdg::BaseDirectories::new()?;
  let config_file = xdg
    .place_config_file(format!("{APP_NAME}/config.toml"))
    .with_context(|| "cannot find xdg config directory")?;

  log::info!("Loading config from {}", config_file.display());

  let mut config: Config = toml::from_slice(&std::fs::read(config_file)?)?;

  config.calendar_location = config
    .calendar_location
    .replace("~", &std::env::var("HOME")?);

  log::info!("Config loaded {:?}", &config);

  let timezone = if let Some(tz) = config.timezone {
    chrono_tz::Tz::from_str(&tz)
      .map_err(|x| anyhow::anyhow!("{}", x))?
      .offset_from_utc_datetime(&Utc::now().naive_utc())
      .fix()
  } else {
    util::now().offset().fix()
  };

  let local_backend = backend::LocalDirBuilder::default()
    .calendar(&config.calendar_name)
    .dir(&config.calendar_location)
    .build()?;
  let backend = backend::IndexedLocalDir::new(
    local_backend,
    xdg.place_data_file(format!("{APP_NAME}/{APP_NAME}.db"))?,
  )?;

  let mut app = app::App::new(config.calendar_name, 3, timezone, backend);

  app.load_events();

  let options = eframe::NativeOptions::default();
  eframe::run_native(Box::new(app), options);
}
