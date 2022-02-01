mod app;
mod widget;

fn main() {
  let options = eframe::NativeOptions::default();
  eframe::run_native(Box::new(app::App::default()), options);
}
