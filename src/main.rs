mod app;
mod widget;

fn main() {
  let options = eframe::NativeOptions::default();
  let mut app = app::App::new();
  app.seed_sample_events();
  eframe::run_native(Box::new(app), options);
}
