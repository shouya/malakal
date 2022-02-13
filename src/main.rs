mod app;
mod backend;
mod event;
mod ical;
mod util;
mod widget;

fn main() {
  env_logger::init();

  let options = eframe::NativeOptions::default();
  let local_backend = backend::LocalDirBuilder::default()
    .calendar("time-blocking")
    .dir(format!("{}/.calendar/time-blocking", env!("HOME")))
    .build()
    .expect("build backend");
  let backend = backend::IndexedLocalDir::new(
    local_backend,
    format!("{}/.calendar/malakal.db", env!("HOME")),
  )
  .expect("load event index file");

  backend.create_table().expect("create sqlite table");

  let mut app = app::App::new("time-blocking".into(), today_plus(-1), backend);

  app.load_events();

  eframe::run_native(Box::new(app), options);
}

fn today_plus(offset: i64) -> chrono::Date<chrono::Local> {
  chrono::Local::today() + chrono::Duration::days(offset)
}
