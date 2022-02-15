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

impl epi::App for App {
  fn name(&self) -> &str {
    "Malakal"
  }

  fn update(&mut self, ctx: &egui::CtxRef, _frame: &epi::Frame) {
    let first_launch_flag_id = egui::Id::new("first_launch");
    let first_launch: Option<()> =
      ctx.memory().data.get_temp(first_launch_flag_id);
    ctx.memory().data.insert_temp(first_launch_flag_id, ());

    egui::CentralPanel::default().show(ctx, |ui| {
      let mut scroll_area = egui::ScrollArea::both();
      let roughly_8am_y = 640.0;
      if first_launch.is_none() {
        scroll_area = scroll_area.vertical_scroll_offset(roughly_8am_y);
      }

      scroll_area.show(ui, |ui| {
        let mut scheduler = widget::ScheduleUiBuilder::default()
          .new_event_calendar(&self.calendar)
          .first_day(self.state.first_day)
          .day_count(self.state.day_count)
          .build()
          .unwrap();
        scheduler.show(ui, &mut self.state)
      })
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
      request_refresh_events: true,
      events: vec![],
    };

    Self {
      calendar,
      state,
      backend: Box::new(backend),
    }
  }

  pub fn load_events(&mut self) {
    if !self.state.request_refresh_events {
      return;
    }

    let start = self.state.first_day.and_hms(0, 0, 0);
    let end = start + chrono::Duration::days(self.state.day_count as i64);
    let events = self.backend.get_events(start, end).expect("load events");
    self.state.events = events;
    self.state.request_refresh_events = false;
  }

  fn apply_event_changes(&mut self) -> Result<()> {
    for event in self.state.events.iter() {
      if event.is_editing() {
        // we do not create events that are still been edited
        continue;
      }

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
