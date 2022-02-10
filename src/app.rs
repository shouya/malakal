use chrono::{Date, Local};
use eframe::{egui, epi};

use crate::widget::ScheduleUiState;
use crate::{backend::Backend, widget};

pub struct App {
  day_count: usize,
  calendar: String,
  state: widget::ScheduleUiState,
  backend: Box<dyn Backend>,
}

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    self.load_events();

    egui::CentralPanel::default().show(ctx, |ui| {
      egui::ScrollArea::both().show(ui, |ui| {
        let mut scheduler = widget::ScheduleUiBuilder::default()
          .new_event_calendar(&self.calendar)
          .first_day(self.state.date)
          .build()
          .unwrap();
        scheduler.show(ui, &mut self.state)
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
    let state = ScheduleUiState {
      date,
      request_refresh_events: true,
      events: vec![],
    };

    Self {
      calendar,
      backend: Box::new(backend),
      day_count: 3,
      state,
    }
  }

  pub fn load_events(&mut self) {
    if !self.state.request_refresh_events {
      return;
    }

    let start = self.state.date.and_hms(0, 0, 0);
    let end = start + chrono::Duration::days(self.day_count as i64);
    let events = self.backend.get_events(start, end).expect("load events");
    self.state.events = events;
    self.state.request_refresh_events = false;
  }

  fn apply_changes(&mut self) {
    for event in self.state.events.iter() {
      if event.deleted {
        self.backend.delete_event(&event.id);
      } else if event.changed {
        self.backend.update_event(event);
      }
    }

    self.state.events.retain(|e| !e.deleted);

    for event in self.state.events.iter_mut() {
      event.reset_dirty_flags();
    }
  }
}
