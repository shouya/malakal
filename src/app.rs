use chrono::Local;
use eframe::{egui, epi};

use crate::event::{Event, EventBuilder};
use crate::widget;

#[derive(Default)]
pub struct App {
  events: Vec<Event>,
}

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    egui::CentralPanel::default().show(ctx, |ui| {
      ui.heading("Hello!");

      egui::ScrollArea::both().show(ui, |ui| {
        let mut scheduler = widget::ScheduleUiBuilder::default()
          .new_event_calendar("time-blocking")
          .build()
          .unwrap();
        scheduler.show(ui, &mut self.events)
      })
    });
  }
}

impl App {
  pub fn seed_sample_events(&mut self) {
    let e1 = EventBuilder::default()
      .id("1")
      .title("C: vocab")
      .calendar("time-blocking")
      .start(Local::today().and_hms(14, 0, 0))
      .end(Local::today().and_hms(15, 0, 0))
      .build()
      .unwrap();
    let e2 = EventBuilder::default()
      .id("2")
      .title("C: reading")
      .calendar("time-blocking")
      .start(Local::today().and_hms(14, 30, 0))
      .end(Local::today().and_hms(16, 0, 0))
      .build()
      .unwrap();
    let e3 = EventBuilder::default()
      .id("3")
      .title("C: gaming")
      .calendar("time-blocking")
      .start(Local::today().and_hms(15, 30, 0))
      .end(Local::today().and_hms(16, 23, 49))
      .build()
      .unwrap();

    self.events.extend_from_slice(&[e1, e2, e3]);
  }
}
