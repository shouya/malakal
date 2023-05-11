use std::fs;
use std::str::FromStr;

use anyhow::Context;
use chrono::{Offset, TimeZone, Utc};

use crate::config::{Config, APP_NAME};

mod app;
mod backend;
mod config;
mod event;
mod hook;
mod ical;
mod notifier;
mod util;
mod widget;

fn main() -> anyhow::Result<()> {
  // default to log info
  env_logger::builder()
    .filter_level(log::LevelFilter::Info)
    .parse_default_env()
    .init();

  let config = Config::read_or_initialize()?;
  log::info!("Config loaded {:?}", &config);

  let timezone = if let Some(ref tz) = config.timezone {
    chrono_tz::Tz::from_str(tz)
      .map_err(|x| anyhow::anyhow!("{}", x))?
      .offset_from_utc_datetime(&Utc::now().naive_utc())
      .fix()
  } else {
    util::local_tz()
  };

  let local_backend = backend::LocalDirBuilder::default()
    .calendar(&config.calendar_name)
    .dir(&config.calendar_location)
    .build()?;

  let db_path = {
    let mut path = dirs::data_dir()
      .with_context(|| "Cannot find a directory to store data")?;
    path.push(format!("{APP_NAME}/{APP_NAME}.db"));
    path
  };

  if fs::metadata(&db_path).is_err() {
    fs::create_dir_all(db_path.parent().unwrap())?;
  }

  let backend = backend::IndexedLocalDir::new(local_backend, db_path)?;

  let mut app = app::App::new(&config, 3, timezone, backend)?;

  app.load_events();

  let options = eframe::NativeOptions::default();

  eframe::run_native(
    APP_NAME,
    options,
    Box::new(|ctx| Box::new(app.setup(ctx))),
  );

  Ok(())
}
