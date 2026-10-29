use std::sync::atomic::AtomicBool;
use std::thread;

use eframe::egui::Ui;
use eframe::{CreationContext, egui};
use jiff::{Timestamp, ToSpan, tz::TimeZone};

use crate::config::Config;
use crate::hook::HookExecutor;
use crate::util::shared;
use crate::{
  backend::Backend,
  notifier::Notifier,
  util::{Result, Shared, today},
  widget,
};

pub struct App {
  scheduler_ui: widget::ScheduleUi,
  backend: Shared<dyn Backend>,
  notifier: Notifier,
  refresh_timer: Option<thread::JoinHandle<()>>,
  hook: HookExecutor,
  last_rect: Option<egui::Rect>,
}

static SCROLL: AtomicBool = AtomicBool::new(true);

impl eframe::App for App {
  fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
    self.refresh_events();
    self.load_events();

    self.scheduler_ui.update_current_time();

    egui::CentralPanel::default().show(ui, |ui| {
      let mut scroll_area = egui::ScrollArea::both();

      if SCROLL.fetch_and(false, std::sync::atomic::Ordering::SeqCst) {
        let now = self.scheduler_ui.scroll_position_for_now();
        scroll_area = scroll_area.vertical_scroll_offset(now);
      }

      scroll_area.show(ui, |ui| {
        apply_schedule_style(ui.style_mut());

        let just_resized = match self.last_rect {
          None => true,
          Some(rect) => rect != ui.max_rect(),
        };
        if just_resized {
          self.last_rect = Some(ui.max_rect());
          self.scheduler_ui.refit_into_ui(ui);
        }

        self.scheduler_ui.show(ui)
      });
    });

    self.apply_event_changes().expect("Failed applying changes");
  }
}

impl App {
  pub fn setup(mut self, ctx: &CreationContext) -> Self {
    let ctx = ctx.egui_ctx.clone();
    self.refresh_timer = Some(thread::spawn(move || {
      loop {
        thread::sleep(std::time::Duration::from_millis(1000));
        ctx.request_repaint();
      }
    }));
    self
  }

  pub fn new(
    config: &Config,
    day_count: usize,
    timezone: TimeZone,
    backend: impl Backend + 'static,
  ) -> Result<Self> {
    let first_day = today(&timezone) - (day_count as i64 / 2).days();
    let backend: Shared<dyn Backend> = shared(backend);
    let notifier = Notifier::start(config, &backend);

    let min_width = config.day_column_width.unwrap_or(260.0);

    let scheduler_ui = widget::ScheduleUiBuilder::default()
      .new_event_calendar(config.calendar_name.clone())
      .first_day(first_day)
      .current_time(Timestamp::now())
      .timezone(timezone)
      .day_count(day_count)
      .refresh_requested(true)
      .scope_updated(true)
      .day_width(min_width)
      .day_min_width((min_width - 100.0).min(200.0))
      .build()
      .expect("failed to build scheduler");

    let hook = HookExecutor::new(config);

    Ok(Self {
      scheduler_ui,
      backend,
      notifier,
      hook,
      last_rect: None,
      refresh_timer: None,
    })
  }

  pub fn refresh_events(&mut self) {
    if !self.scheduler_ui.refresh_requested {
      return;
    }

    self
      .backend
      .lock()
      .unwrap()
      .force_refresh()
      .expect("failed to reload event");

    self.load_events();

    self.scheduler_ui.refresh_requested = false;
  }

  pub fn load_events(&mut self) {
    if !self.scheduler_ui.scope_updated {
      return;
    }

    let (start, end) = self.scheduler_ui.time_range();
    let events = self
      .backend
      .lock()
      .unwrap()
      .get_events(start, end)
      .expect("load events");

    self.scheduler_ui.load_events(events);
    self.scheduler_ui.scope_updated = false;
  }

  fn apply_event_changes(&mut self) -> Result<()> {
    let mut anything_changed = false;
    let mut backend = self.backend.lock().unwrap();
    let events = self.scheduler_ui.events_mut();
    for event in events.iter() {
      if event.deleted {
        backend.delete_event(&event.id)?;
        anything_changed = true;
      } else if event.changed {
        backend.update_event(event)?;
        anything_changed = true;
      }
    }

    events.retain(|e| !e.deleted);

    for event in events.iter_mut() {
      event.reset_dirty_flags();
    }

    drop(backend);

    if anything_changed {
      self.notifier.events_updated();
      self.hook.report_updated();
    }

    Ok(())
  }
}

fn apply_schedule_style(style: &mut egui::Style) {
  style.spacing.item_spacing = egui::vec2(3.0, 3.0);

  let widgets = &mut style.visuals.widgets;
  for visuals in [
    &mut widgets.noninteractive,
    &mut widgets.inactive,
    &mut widgets.active,
    &mut widgets.open,
  ] {
    visuals.corner_radius = egui::CornerRadius::same(2);
  }
  widgets.hovered.corner_radius = egui::CornerRadius::same(3);
}
