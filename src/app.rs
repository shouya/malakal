use std::sync::atomic::AtomicBool;

use chrono::{Duration, Local};
use eframe::{egui, epi};

use crate::util::Result;
use crate::widget::ScheduleUiState;
use crate::{backend::Backend, widget};

pub struct App {
  calendar: String,
  state: widget::ScheduleUiState,
  backend: Box<dyn Backend>,
}

static SCROLL: AtomicBool = AtomicBool::new(true);

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    self.refresh_events();
    self.load_events();

    let mut scheduler = widget::ScheduleUiBuilder::default()
      .new_event_calendar(&self.calendar)
      .first_day(self.state.first_day)
      .day_count(self.state.day_count)
      .build()
      .unwrap();

    egui::CentralPanel::default().show(ctx, |ui| {
      let mut scroll_area = egui::ScrollArea::both();

      if SCROLL.fetch_and(false, std::sync::atomic::Ordering::SeqCst) {
        let now = scheduler.scroll_position(&Local::now());
        scroll_area = scroll_area.vertical_scroll_offset(now);
      }

      scroll_area.show(ui, |ui| scheduler.show(ui, &mut self.state))
    });

    self.apply_event_changes().expect("Failed applying changes");
  }
}

impl App {
  pub fn new(
    calendar: String,
    day_count: usize,
    backend: impl Backend + 'static,
  ) -> Self {
    let first_day = Local::today() - Duration::days(day_count as i64 / 2);

    let state = ScheduleUiState {
      day_count,
      first_day,
      scope_updated: true,
      refresh_requested: false,
      events: vec![],
    };

    Self {
      calendar,
      state,
      backend: Box::new(backend),
    }
  }

  pub fn refresh_events(&mut self) {
    if !self.state.refresh_requested {
      return;
    }

    self
      .backend
      .force_refresh()
      .expect("failed to reload event");

    self.load_events();

    self.state.refresh_requested = false;
  }

  pub fn load_events(&mut self) {
    if !self.state.scope_updated {
      return;
    }

    let start = self.state.first_day.and_hms(0, 0, 0);
    let end = start + chrono::Duration::days(self.state.day_count as i64);
    let events = self.backend.get_events(start, end).expect("load events");
    self.state.events = events;

    self.state.scope_updated = false;
  }

  fn apply_event_changes(&mut self) -> Result<()> {
    for event in self.state.events.iter() {
      if event.deleted {
        self.backend.delete_event(&event.id)?;
      } else if event.changed {
        self.backend.update_event(event)?;
      }
    }

    self.state.events.retain(|e| !e.deleted);

    for event in self.state.events.iter_mut() {
      event.reset_dirty_flags();
    }

    Ok(())
  }
}
