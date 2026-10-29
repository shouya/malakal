mod interaction;
mod layout;

use derive_builder::Builder;
use eframe::egui::{
  self, Color32, Pos2, Rect, Response, Sense, Ui, Vec2, pos2, vec2,
};
use jiff::{
  RoundMode, SignedDuration, Timestamp, ToSpan, Unit,
  civil::{Date, DateTime, DateTimeRound, Time},
  tz::TimeZone,
};
use uuid::Uuid;

use self::{
  interaction::History,
  layout::{Layout, LayoutAlgorithm},
};

use crate::{
  event::{Event, EventBuilder},
  util::{on_the_same_day, seconds_since_midnight, today},
  widget::CalendarBuilder,
};

use super::Calendar;

#[derive(Builder, Clone, Debug, PartialEq)]
#[builder(try_setter, setter(into))]
pub struct ScheduleUi {
  #[builder(default = "3")]
  day_count: usize,
  #[builder(default = "260.0")]
  day_width: f32,
  #[builder(default = "100.0")]
  day_min_width: f32,
  #[builder(default = "260.0")]
  day_max_width: f32,
  #[builder(default = "24")]
  segment_count: usize,
  #[builder(default = "90.0")]
  segment_height: f32,
  #[builder(default = "80.0")]
  time_marker_margin_width: f32,
  #[builder(default = "60.0")]
  day_header_margin_height: f32,
  #[builder(default = "\"%H:%M\"")]
  time_marker_format: &'static str,
  #[builder(default = "\"%F %a\"")]
  day_header_format: &'static str,

  first_day: Date,

  // a small margin on the right of day columns reserved for creating
  // new events
  #[builder(default = "20.0")]
  new_event_margin: f32,

  // used to render current time indicator
  current_time: Option<Timestamp>,

  // used to refresh every second
  #[builder(default = "std::time::Instant::now()", setter(skip))]
  last_update: std::time::Instant,

  #[builder(default = "5.0")]
  resizer_height: f32,
  #[builder(default = "20.0")]
  resizer_width_margin: f32,

  #[builder(default = "SignedDuration::from_mins(15)")]
  min_event_duration: SignedDuration,

  #[builder(default = "SignedDuration::from_mins(15)")]
  snapping_duration: SignedDuration,

  #[builder(default = "\"%H:%M\"")]
  event_resizing_hint_format: &'static str,

  #[builder(default = "Color32::LIGHT_BLUE")]
  new_event_color: Color32,

  timezone: TimeZone,

  new_event_calendar: String,

  #[builder(default = "false")]
  pub scope_updated: bool,

  #[builder(default = "false")]
  pub refresh_requested: bool,

  #[builder(default = "vec![]")]
  events: Vec<Event>,

  #[builder(default, setter(skip))]
  history: History,

  #[builder(default)]
  calendar: Option<Calendar>,
}

type EventId = String;

#[derive(Debug)]
enum EventLayoutType {
  // start, end
  Single(f32, f32),
  #[allow(unused)]
  AllDay([Date; 2]),
}

const SECS_PER_DAY: i64 = 24 * 3600;

impl ScheduleUi {
  // the caller must ensure the events are all within the correct days
  fn layout_events(&self, events: &[&Event]) -> Layout {
    let mut layout = Layout::default();

    for day in 0..self.day_count {
      // layout for each day
      let events: Vec<layout::Ev> = events
        .iter()
        .filter(|&e| !e.deleted)
        .filter(|&e| {
          self.date_to_day(self.to_local(e.start).date()) == Some(day)
        })
        .filter(|&e| matches!(self.layout_type(e), EventLayoutType::Single(..)))
        .map(|e| {
          let end = e.end.max(e.start + self.min_event_duration);
          (&e.id, e.start.as_second(), end.as_second()).into()
        })
        .collect();

      layout.merge(layout::MarkusAlgorithm::compute(events))
    }

    layout
  }

  fn event_rect(
    &self,
    ui: &Ui,
    layout: &Layout,
    event: &Event,
  ) -> Option<Rect> {
    let widget_rect = ui.max_rect();
    match self.layout_type(event) {
      EventLayoutType::Single(start, end) => {
        let rel_x = layout.query(&event.id)?;
        let day = start as usize as f32;
        let y = [(start - day).clamp(0.0, 1.0), (end - day).clamp(0.0, 1.0)];
        let rect = self.layout_event(widget_rect, day as usize, y, rel_x);
        let margin = ui.style().spacing.item_spacing / 2.0;

        Some(rect.shrink2(margin))
      }
      _ => unimplemented!(),
    }
  }

  fn layout_event(
    &self,
    widget_rect: Rect,
    day: usize,
    y: [f32; 2],
    x: [f32; 2],
  ) -> Rect {
    let mut rect = self.day_column(day);

    // leave a margin for creating new events
    rect.set_right(rect.right() - self.new_event_margin);

    let w = rect.width();
    let h = rect.height();

    rect.set_right(rect.left() + x[1] * w);
    rect.set_left(rect.left() + x[0] * w);

    rect.set_bottom(rect.top() + y[1] * h);
    rect.set_top(rect.top() + y[0] * h);

    rect.translate(self.content_offset(widget_rect))
  }

  // These move_event_{start,end} functions aim to enforce few constraints:
  //
  // 1. event end must be later than event start
  // 2. event duration must be at least self.min_event_duration long
  // 3. event date can't be changed

  fn to_local(&self, time: Timestamp) -> DateTime {
    self.timezone.to_datetime(time)
  }

  fn to_timestamp(&self, local: DateTime) -> Option<Timestamp> {
    self
      .timezone
      .to_ambiguous_timestamp(local)
      .compatible()
      .ok()
  }

  fn pointer_pos_to_datetime(&self, rel_pos: Pos2) -> Option<Timestamp> {
    let day = (rel_pos.x / self.day_width) as i64;
    if !(day >= 0 && day < self.day_count as i64) {
      return None;
    }

    let vert_pos = rel_pos.y / self.content_height();
    if !(vert_pos > 0.0 && vert_pos < 1.0) {
      return None;
    }

    let seconds = SECS_PER_DAY as f32 * vert_pos;
    let seconds = ((seconds / 60.0).round() * 60.0) as i64;

    let date = self.first_day + day.days();
    let local =
      date.to_datetime(Time::midnight()) + SignedDuration::from_secs(seconds);
    self.to_timestamp(local)
  }

  fn pointer_pos_to_datetime_snapping(
    &self,
    rel_pos: Pos2,
  ) -> Option<Timestamp> {
    let day = (rel_pos.x / self.day_width) as i64;
    if !(day >= 0 && day < self.day_count as i64) {
      return None;
    }

    let vert_pos = rel_pos.y / self.content_height();
    if vert_pos < 0.0 {
      // Note: we must allow vert_pos to exceed 1.0, otherwise we
      // can't snap to the end of day.
      return None;
    }

    let seconds = SECS_PER_DAY as f32 * vert_pos;
    let local = self.first_day.to_datetime(Time::midnight())
      + day.days()
      + SignedDuration::from_secs(seconds as i64);
    let snapped = self.snap(local, RoundMode::Floor);
    let end_of_day =
      (self.first_day + (day + 1).days()).to_datetime(Time::midnight());
    self.to_timestamp(snapped.min(end_of_day))
  }

  fn snap_to_nearest(&self, time: Timestamp) -> Timestamp {
    let snapped = self.snap(self.to_local(time), RoundMode::HalfExpand);
    self.to_timestamp(snapped).expect("date overflow")
  }

  fn snap(&self, local: DateTime, mode: RoundMode) -> DateTime {
    let rounding = DateTimeRound::new()
      .smallest(Unit::Minute)
      .increment(self.snapping_duration.as_mins())
      .mode(mode);
    local
      .round(rounding)
      .expect("snapping_duration must divide an hour")
  }

  fn event_resizer_regions(&self, rect: Rect) -> [Rect; 2] {
    let corner_area = if rect.width() > self.resizer_width_margin * 4.0 {
      vec2(self.resizer_width_margin, 0.0)
    } else {
      vec2(rect.width() / 4.0, 0.0)
    };
    let mut upper_resizer = rect.shrink2(corner_area);
    upper_resizer.set_height(self.resizer_height);

    let mut lower_resizer = rect.shrink2(corner_area);
    lower_resizer.set_top(rect.bottom() - self.resizer_height);

    if upper_resizer.intersects(lower_resizer) {
      // overlaps, then we keep only the lower resizer
      upper_resizer.set_height(0.0);
    }

    [upper_resizer, lower_resizer]
  }

  fn date_to_day(&self, date: Date) -> Option<usize> {
    let diff_days = (date - self.first_day).get_days();
    if diff_days < 0 || diff_days as usize >= self.day_count {
      return None;
    }

    Some(diff_days as usize)
  }

  fn draw_ticks(&self, ui: &mut Ui, rect: Rect) {
    self.draw_grid(ui, rect);
  }

  fn draw_grid(&self, ui: &mut Ui, rect: Rect) {
    let widget_visuals = ui.style().noninteractive();

    let offset = self.content_offset(rect);
    let painter = ui.painter_at(rect);

    // vertical lines
    for day in 0..=self.day_count {
      let x = self.day_width * day as f32;
      let y0 = 0.0;
      let y1 = self.segment_height * self.segment_count as f32;
      let ends = [pos2(x, y0) + offset, pos2(x, y1) + offset];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }

    // horizontal lines
    for seg in 0..=self.segment_count {
      let y = self.segment_height * seg as f32;
      let x0 = 0.0;
      let x1 = self.day_width * self.day_count as f32;
      let ends = [pos2(x0, y) + offset, pos2(x1, y) + offset];

      painter.line_segment(ends, widget_visuals.bg_stroke);
    }
  }

  fn scroll_horizontally(&mut self, days: i64) {
    self.first_day += days.days();
    self.mark_scope_updated();
  }

  fn draw_current_time_indicator(&self, ui: &mut Ui, rect: Rect, alpha: f32) {
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);
    let offset = self.content_offset(rect);

    if let Some(now) = self.current_time {
      let y = self.day_progress(now) * self.content_height();
      let x0 = 0.0;
      let x1 = rect.width();

      let p0 = pos2(x0, y) + offset;
      let p1 = pos2(x1, y) + offset;
      let mut indicator_stroke = widget_visuals.bg_stroke;
      indicator_stroke.color = Color32::RED.linear_multiply(alpha);
      painter.line_segment([p0, p1], indicator_stroke);
    }
  }

  fn time_mark_region(&self) -> Rect {
    Rect::from_min_size(
      pos2(0.0, self.day_header_margin_height),
      vec2(
        self.time_marker_margin_width,
        self.segment_height * self.segment_count as f32,
      ),
    )
  }

  fn draw_time_marks(&self, ui: &mut Ui, rect: Rect) {
    let offset = self.content_offset(rect);

    let spacing = ui.style().spacing.item_spacing;
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);

    let mut time_mark_region =
      self.time_mark_region().translate(rect.left_top().to_vec2());

    let mut alpha = 1.0;

    if time_mark_region.center().x <= ui.clip_rect().left() {
      // floating time mark region
      time_mark_region.set_left(ui.clip_rect().left());
      time_mark_region.set_width(self.time_marker_margin_width);

      alpha = ui.ctx().animate_bool(
        egui::Id::unique("time_mark"),
        !ui.rect_contains_pointer(time_mark_region),
      );
    }

    painter.rect_filled(
      time_mark_region.shrink2(spacing),
      widget_visuals.corner_radius.ne,
      widget_visuals.bg_fill.linear_multiply(alpha * 0.8),
    );

    for seg in 0..=self.segment_count {
      let y = offset.y + seg as f32 * self.segment_height;
      let x = time_mark_region.center().x;

      let text = self.time_marker_text(seg).expect("segment out of bound");
      painter.text(
        pos2(x, y),
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace.resolve(ui.style()),
        widget_visuals.text_color().linear_multiply(alpha),
      );
    }
  }

  fn day_mark_region(&self) -> Rect {
    Rect::from_min_size(
      pos2(self.time_marker_margin_width, 0.0),
      vec2(
        self.day_width * self.day_count as f32,
        self.day_header_margin_height,
      ),
    )
  }

  fn draw_day_marks(&self, ui: &mut Ui, rect: Rect) {
    let spacing = ui.style().spacing.item_spacing;
    let widget_visuals = ui.style().noninteractive();
    let painter = ui.painter_at(rect);

    let today_index = self
      .current_time
      .map(|t| (self.to_local(t).date() - self.first_day).get_days() as i64);

    let mut day_mark_region =
      self.day_mark_region().translate(rect.left_top().to_vec2());

    let mut alpha = 1.0;

    if day_mark_region.center().y <= ui.clip_rect().top() {
      // floating day mark region
      day_mark_region.set_top(ui.clip_rect().top());
      day_mark_region.set_height(self.day_header_margin_height);
      alpha = ui.ctx().animate_bool(
        egui::Id::unique("day_mark"),
        !ui.rect_contains_pointer(day_mark_region),
      );
    }

    painter.rect_filled(
      day_mark_region.shrink2(spacing),
      widget_visuals.corner_radius.ne,
      widget_visuals.bg_fill.linear_multiply(alpha * 0.8),
    );

    for nth_day in 0..self.day_count {
      let x = day_mark_region.left() + (nth_day as f32 + 0.5) * self.day_width;

      let text = self.day_header_text(nth_day).expect("day out of bound");

      let text_rect = painter.text(
        pos2(x, day_mark_region.center().y),
        egui::Align2::CENTER_CENTER,
        text,
        egui::TextStyle::Monospace.resolve(ui.style()),
        widget_visuals.text_color().linear_multiply(alpha),
      );

      if Some(nth_day as i64) == today_index {
        // current day indicator
        let mut stroke = widget_visuals.bg_stroke;
        stroke.color = stroke.color.linear_multiply(alpha);

        painter.circle(
          text_rect.center_bottom() + vec2(0.0, 6.0),
          2.0,
          Color32::RED.linear_multiply(alpha),
          stroke,
        );
      }
    }
  }

  fn content_height(&self) -> f32 {
    self.segment_height * self.segment_count as f32
  }

  #[allow(unused)]
  fn content_width(&self) -> f32 {
    self.day_width * self.day_count as f32
  }

  fn content_offset(&self, widget_rect: Rect) -> Vec2 {
    widget_rect.min.to_vec2() + self.content_offset0()
  }

  fn content_offset0(&self) -> Vec2 {
    vec2(self.time_marker_margin_width, self.day_header_margin_height)
  }

  fn day_column(&self, day: usize) -> Rect {
    let x0 = self.day_width * day as f32;
    let y0 = 0.0;

    let w = self.day_width;
    let h = self.content_height();

    Rect::from_min_size(pos2(x0, y0), vec2(w, h))
  }

  fn day_header_text(&self, nth_day: usize) -> Option<String> {
    if nth_day >= self.day_count {
      return None;
    }

    let day = self.first_day + (nth_day as i64).days();
    Some(day.strftime(self.day_header_format).to_string())
  }

  fn time_marker_text(&self, segment: usize) -> Option<String> {
    if segment > self.segment_count {
      return None;
    }

    let time = self.time_marker_time(segment, 0)?;
    Some(time.strftime(self.time_marker_format).to_string())
  }

  fn time_marker_time(&self, segment: usize, day: usize) -> Option<DateTime> {
    if segment > self.segment_count {
      return None;
    }
    let day = self.first_day + (day as i64).days();
    let seconds = SECS_PER_DAY / self.segment_count as i64 * segment as i64;
    Some(day.to_datetime(Time::midnight()) + SignedDuration::from_secs(seconds))
  }

  fn desired_size(&self) -> Vec2 {
    // give a bit more vertical space to display the last time mark
    let text_safe_margin = 10.0;

    vec2(
      self.time_marker_margin_width + self.day_width * self.day_count as f32,
      self.day_header_margin_height
        + self.segment_height * self.segment_count as f32
        + text_safe_margin,
    )
  }

  pub(crate) fn show_ui(&mut self, ui: &mut Ui) {
    let rect = ui.max_rect();
    let interacting_event = self.get_interacting_event(ui);
    let combined_events: Vec<CombinedEvent> =
      combine_events(&self.events, interacting_event);

    // get response at empty area first (other widgets will steal it)
    let response_on_empty_area = ui.interact(
      ui.max_rect(),
      ui.scope_id().with("empty_area"),
      Sense::click_and_drag(),
    );

    // background: ticks and current time indicator
    self.draw_ticks(ui, rect);
    self.draw_current_time_indicator(ui, rect, 1.0);

    let layout = self.layout_events(
      combined_events
        .iter()
        .map(|x| x.event())
        .collect::<Vec<_>>()
        .as_slice(),
    );

    // main: event buttons
    for combined_event in combined_events {
      match combined_event {
        CombinedEvent::ExistingEvent(event) => {
          self.put_non_interacting_event_block(ui, &layout, &event);
        }
        CombinedEvent::InteractingEvent(_event) => {
          self.put_interacting_event_block(ui, &layout);
        }
      }
    }

    // floating: time and day headers
    self.draw_day_marks(ui, rect);
    self.draw_time_marks(ui, rect);

    // interact with blank area for context menu and new event creation
    self.handle_new_event(ui, &response_on_empty_area);
    self.handle_context_menu(&response_on_empty_area);

    self.refocus_edited_event(ui);
    self.handle_hotkeys(ui);
    self.handle_undo(ui);
  }

  pub(crate) fn show(&mut self, ui: &mut Ui) {
    let (_id, rect) = ui.allocate_space(self.desired_size());

    if !ui.is_rect_visible(rect) {
      return;
    }

    // regularize timezone & enforce minimal duration
    self.regularize_events();

    // draw the event ui
    let child_ui_builder = egui::UiBuilder::new()
      .layout(egui::Layout::left_to_right(egui::Align::default()))
      .max_rect(rect);
    let mut child_ui = ui.new_child(child_ui_builder);
    self.show_ui(&mut child_ui);

    // commit any event changes
    self.apply_interacting_events(ui);
    remove_empty_events(&mut self.events);
  }

  pub fn time_range(&self) -> (Timestamp, Timestamp) {
    let last_day = self.first_day + (self.day_count as i64).days();
    let start = self.to_timestamp(self.first_day.to_datetime(Time::midnight()));
    let end = self.to_timestamp(last_day.to_datetime(Time::midnight()));

    (start.expect("date overflow"), end.expect("date overflow"))
  }

  pub fn visible_dates(&self) -> Vec<Date> {
    self
      .first_day
      .series(1.day())
      .take(self.day_count)
      .collect()
  }

  pub fn is_visible(&self, time: Timestamp) -> bool {
    self.date_to_day(self.to_local(time).date()).is_some()
  }

  pub fn load_events(&mut self, events: Vec<Event>) {
    // avoid new events interfering with history
    self.history.clear();
    self.events = events;
  }

  pub fn events_mut(&mut self) -> &mut Vec<Event> {
    &mut self.events
  }

  fn mark_scope_updated(&mut self) {
    self.scope_updated = true;

    // reset calendar dates
    self.calendar = None;
  }

  fn handle_context_menu(&mut self, response: &Response) {
    response.context_menu(|ui| {
      if ui.button("Refresh").clicked() {
        self.refresh_requested = true;
        self.mark_scope_updated();
        ui.label("Refreshing events...");
        ui.close();
      }
      ui.separator();

      ui.horizontal(|ui| {
        if ui.button("<<").clicked() {
          self.scroll_horizontally(-(self.day_count as i64));
        }
        if ui.button("<").clicked() {
          self.scroll_horizontally(-1);
        }
        if ui.button("Today").clicked() {
          self.first_day =
            today(&self.timezone) - (self.day_count as i64 / 2).days();
          self.mark_scope_updated();
        }
        if ui.button(">").clicked() {
          self.scroll_horizontally(1);
        }
        if ui.button(">>").clicked() {
          self.scroll_horizontally(self.day_count as i64);
        }
      });
      ui.separator();

      self.show_calendar(ui);
      ui.separator();

      if ui.button("Close menu").clicked() {
        ui.close();
      }
    });
  }

  fn show_calendar(&mut self, ui: &mut Ui) {
    use super::CalendarAction::*;

    let visible_dates = self.visible_dates();
    let default_date = self.current_time.map(|t| self.to_local(t).date());

    let calendar = self.calendar.get_or_insert_with(|| {
      CalendarBuilder::default()
        .date(self.first_day + (self.day_count as i64 / 2).days())
        .current_date(default_date)
        .weekday_offset(1)
        .highlight_dates(visible_dates)
        .build()
        .unwrap()
    });

    match calendar.show_ui(ui) {
      None => (),
      Some(DateClicked(date)) => {
        self.first_day = date - (self.day_count as i64 / 2).days();
        self.mark_scope_updated();
      }
    }
  }

  fn new_event(&self) -> Event {
    let color = egui::Rgba::from(self.new_event_color);
    let start = self
      .to_timestamp(self.first_day.to_datetime(Time::midnight()))
      .expect("date overflow");
    let end = start + self.min_event_duration;
    let mut event = EventBuilder::default()
      .id(new_event_id())
      .calendar(self.new_event_calendar.as_str())
      .title("")
      .description(None)
      .start(start)
      .end(end)
      .timestamp(Timestamp::now())
      .created_at(Timestamp::now())
      .modified_at(Timestamp::now())
      .color([color.r(), color.g(), color.b()])
      .build()
      .unwrap();

    event.mark_changed();
    event
  }

  fn clone_to_new_event(&self, event: &Event) -> Event {
    let mut new_event = event.clone();
    new_event.id = new_event_id();
    new_event.mark_changed();
    new_event
  }

  fn pointer_to_datetime_auto(&self, ui: &Ui, pos: Pos2) -> Option<Timestamp> {
    if ui.input(|input| input.modifiers.shift_only()) {
      // no snapping when shift is held down
      self.pointer_pos_to_datetime(pos)
    } else {
      // enable snapping otherwise
      self.pointer_pos_to_datetime_snapping(pos)
    }
  }

  // Need to ensure the ui's max_rect is the rect allocated for the
  // whole widget
  fn relative_pointer_pos(&self, ui: &Ui) -> Option<Pos2> {
    let mut pointer_pos = None
      .or_else(|| ui.input(|input| input.pointer.interact_pos()))
      .or_else(|| ui.input(|input| input.pointer.hover_pos()))?;
    pointer_pos -= self.content_offset(ui.max_rect());
    Some(pointer_pos)
  }

  fn regularize_events(&mut self) {
    remove_empty_events(&mut self.events);

    for event in self.events.iter_mut() {
      if event.start.duration_until(event.end) < self.min_event_duration {
        move_event_end(
          event,
          event.end + self.min_event_duration,
          self.min_event_duration,
          &self.timezone,
        );
      }
    }
  }

  pub fn scroll_position(&self, time: Timestamp) -> f32 {
    self.date_time_to_pos(time).y
  }

  pub fn scroll_position_for_now(&self) -> f32 {
    self.scroll_position(Timestamp::now())
  }

  fn date_time_to_pos(&self, time: Timestamp) -> Pos2 {
    let x = (self.to_local(time).date() - self.first_day).get_days() as f32
      / self.day_width
      + self.time_marker_margin_width;
    let y = self.day_progress(time) * self.content_height()
      + self.day_header_margin_height;
    pos2(x, y)
  }

  fn day_progress(&self, time: Timestamp) -> f32 {
    let seconds = seconds_since_midnight(self.to_local(time));
    (seconds as f32 / SECS_PER_DAY as f32).clamp(0.0, 1.0)
  }

  fn to_normalized_time(&self, time: Timestamp) -> f32 {
    let local = self.to_local(time);
    let integer_part = (local.date() - self.first_day).get_days() as f32;
    let fraction_part =
      seconds_since_midnight(local) as f32 / SECS_PER_DAY as f32;

    integer_part + fraction_part
  }

  fn layout_type(&self, event: &Event) -> EventLayoutType {
    let start = self.to_normalized_time(event.start);
    let end = self.to_normalized_time(event.end);
    EventLayoutType::Single(start, end)
  }

  pub fn update_current_time(&mut self) {
    self.current_time = Some(Timestamp::now());
  }

  pub fn refit_into_ui(&mut self, ui: &Ui) {
    let day_space_width = ui.max_rect().width() - self.time_marker_margin_width;

    let day_count_min = day_space_width / self.day_max_width;
    let day_count_max = day_space_width / self.day_min_width;
    let optimal_day_count =
      ((day_count_max + day_count_min) / 2.0).round() as usize;

    match optimal_day_count {
      0 => self.day_count = 1,
      n => self.day_count = n,
    }

    self.day_width = match day_space_width / self.day_count as f32 {
      width if width < self.day_min_width => self.day_min_width,
      width => width,
    };

    self.mark_scope_updated()
  }
}

fn new_event_id() -> EventId {
  format!("{}", Uuid::new_v4().hyphenated())
}

enum CombinedEvent {
  ExistingEvent(Event),
  InteractingEvent(Event),
}

impl CombinedEvent {
  fn event(&self) -> &Event {
    match self {
      CombinedEvent::ExistingEvent(ev) => ev,
      CombinedEvent::InteractingEvent(ev) => ev,
    }
  }

  fn event_id(&self) -> &EventId {
    &self.event().id
  }
}

fn combine_events(
  events: &[Event],
  interacting_event: Option<Event>,
) -> Vec<CombinedEvent> {
  use CombinedEvent::*;

  let mut out_events: Vec<_> =
    events.iter().map(|x| ExistingEvent(x.clone())).collect();

  match interacting_event {
    None => (),
    Some(interacting_event) => {
      match out_events
        .iter_mut()
        .find(|ev| ev.event_id() == &interacting_event.id)
      {
        None => out_events.push(InteractingEvent(interacting_event)),
        Some(e) => *e = InteractingEvent(interacting_event),
      }
    }
  }

  out_events
}

fn move_event_end(
  event: &mut Event,
  new_end: Timestamp,
  min_event_duration: SignedDuration,
  tz: &TimeZone,
) {
  if new_end < event.start + min_event_duration {
    return;
  }

  if !on_the_same_day(event.start, new_end, tz) {
    return;
  }

  if event.end != new_end {
    event.mark_changed();
    event.end = new_end;
  }
}

fn move_event_start(
  event: &mut Event,
  new_start: Timestamp,
  min_event_duration: SignedDuration,
  tz: &TimeZone,
) {
  if event.end < new_start + min_event_duration {
    return;
  }

  if !on_the_same_day(new_start, event.end, tz) {
    return;
  }

  if event.start != new_start {
    event.mark_changed();
    event.start = new_start;
  }
}

fn move_event(event: &mut Event, new_start: Timestamp, tz: &TimeZone) {
  let new_end = new_start + event.start.duration_until(event.end);

  if !on_the_same_day(new_start, new_end, tz) {
    return;
  }

  if event.start != new_start || event.end != new_end {
    event.mark_changed();
    event.start = new_start;
    event.end = new_end;
  }
}

fn remove_empty_events(events: &mut [Event]) {
  for event in events.iter_mut() {
    if event.title.is_empty() {
      event.mark_deleted();
    }
  }
}
