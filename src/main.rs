mod app;
mod backend;
mod event;
mod ical;
mod widget;

fn main() {
  let options = eframe::NativeOptions::default();
  let backend = backend::LocalDirBuilder::default()
    .calendar("time-blocking")
    .dir(format!("{}/.calendar/time-blocking-malakal", env!("HOME")))
    .build()
    .expect("build backend");

  let mut app = app::App::new("time-blocking".into(), today_plus(-1), backend);

  app.load_events();

  eframe::run_native(Box::new(app), options);
}

fn today_plus(offset: i64) -> chrono::Date<chrono::Local> {
  chrono::Local::today() + chrono::Duration::days(offset)
}
