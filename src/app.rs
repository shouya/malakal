use eframe::{egui, epi};

use crate::widget;

#[derive(Default)]
pub struct App {}

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    egui::CentralPanel::default().show(ctx, |ui| {
      ui.heading("Hello!");

      egui::ScrollArea::both().show(ui, |ui| {
        let mut scheduler =
          widget::ScheduleUiBuilder::default().build().unwrap();
        ui.add(&mut scheduler);
      })
    });
  }
}
