use std::str::FromStr;

use anyhow::Context;
use chrono::{Offset, TimeZone, Utc};
use eframe::egui::ViewportBuilder;

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

  let backend = backend::IndexedLocalDir::new(local_backend, db_path)?;

  let mut app = app::App::new(&config, 3, timezone, backend)?;

  app.load_events();

  let viewport = ViewportBuilder {
    title: Some(APP_NAME.to_owned()),
    app_id: Some(APP_NAME.to_owned()),
    ..Default::default()
  };
  let options = eframe::NativeOptions {
    viewport,
    ..Default::default()
  };

  let eframe_res = eframe::run_native(
    APP_NAME,
    options,
    Box::new(|ctx| Box::new(app.setup(ctx))),
  );
  match eframe_res {
    Ok(o) => o,
    Err(e) => {
      log::error!("Error running gui: {:?}", e);
      std::process::exit(1);
    }
  };

  Ok(())
}
