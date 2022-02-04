use chrono::Local;
use eframe::{egui, epi};

use crate::widget::{self, EventBlock};

pub struct App {
  events: Vec<EventBlock>,
  schedule_ui: widget::ScheduleUi,
}

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    egui::CentralPanel::default().show(ctx, |ui| {
      ui.heading("Hello!");

      egui::ScrollArea::both()
        .show(ui, |ui| self.schedule_ui.show(ui, &mut self.events))
    });
  }
}

impl App {
  pub fn new() -> Self {
    let events = vec![];
    let schedule_ui = widget::ScheduleUiBuilder::default().build().unwrap();

    Self {
      events,
      schedule_ui,
    }
  }

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
      color: egui::Color32::RED,
      title: "C: feynman 2".into(),
      description: None,
      start: Local::today().and_hms(15, 30, 0),
      end: Local::today().and_hms(16, 0, 0),
    });
  }
}
