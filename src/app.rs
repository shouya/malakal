use chrono::Local;
use eframe::{egui, epi};

use crate::widget::{self, EventBlock};

#[derive(Default)]
pub struct App {
  events: Vec<EventBlock>,
}

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
        scheduler.show(ui, &mut self.events)
      })
    });
  }
}

impl App {
  pub fn seed_sample_events(&mut self) {
    self.events.push(EventBlock {
      id: "1".into(),
      color: egui::Color32::GREEN,
      title: "C: vocab".into(),
      description: None,
      start: Local::today().and_hms(14, 0, 0),
      end: Local::today().and_hms(15, 0, 0),
    });
    self.events.push(EventBlock {
      id: "2".into(),
      color: egui::Color32::GREEN,
      title: "C: feynman".into(),
      description: None,
      start: Local::today().and_hms(14, 30, 0),
      end: Local::today().and_hms(16, 0, 0),
    });
    self.events.push(EventBlock {
      id: "3".into(),
      color: egui::Color32::GREEN,
      title: "C: feynman".into(),
      description: None,
      start: Local::today().and_hms(15, 30, 0),
      end: Local::today().and_hms(16, 0, 0),
    });
  }
}
