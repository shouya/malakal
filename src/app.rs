use chrono::{Date, Local};
use eframe::{egui, epi};

use crate::event::Event;
use crate::{backend::Backend, widget};

pub struct App {
  date: Date<Local>,
  day_count: usize,
  calendar: String,
  events: Vec<Event>,
  backend: Box<dyn Backend>,
}

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    egui::CentralPanel::default().show(ctx, |ui| {
      egui::ScrollArea::both().show(ui, |ui| {
        let mut scheduler = widget::ScheduleUiBuilder::default()
          .new_event_calendar(&self.calendar)
          .build()
          .unwrap();
        scheduler.show(ui, &mut self.events)
      })
    });

    self.apply_changes();
  }
}

impl App {
  pub fn new(
    calendar: String,
    date: Date<Local>,
    backend: impl Backend + 'static,
  ) -> Self {
    Self {
      calendar,
      date,
      backend: Box::new(backend),
      events: vec![],
      day_count: 3,
    }
  }

  pub fn load_events(&mut self) {
    let start = self.date.and_hms(0, 0, 0);
    let end = start + chrono::Duration::days(self.day_count as i64);
    let events = self.backend.get_events(start, end).expect("load events");
    self.events = events;
  }

  fn apply_changes(&mut self) {
    for event in self.events.iter_mut() {
      if event.deleted {
        self.backend.delete_event(&event.id);
      } else if event.changed {
        dbg!(self.backend.update_event(event));
      }
    }

    self.events.retain(|e| !e.deleted);

    for event in self.events.iter_mut() {
      event.reset_dirty_flags();
    }
  }
}
