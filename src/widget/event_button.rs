#![allow(unused)]
use eframe::egui::{self, Button, Id, PointerButton, Rect, TextEdit, Ui};

use crate::widget::EventBlock;

#[derive(Clone, Copy, Default, Debug)]
pub struct EventButton;

#[derive(Clone, Copy)]
struct EditingEvent;

impl EventButton {
  pub fn show(&self, ui: &mut Ui, rect: Rect, event: &mut EventBlock) {
    let id = Id::new("event").with(&event.id).with("edit");

    let is_editing = ui.memory().data.get_temp::<EditingEvent>(id).is_some();

    if is_editing {
      self.show_text_box(ui, rect, event, id);
    } else {
      self.show_label(ui, rect, event, id);
    }
  }

  fn show_text_box(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &mut EventBlock,
    id: Id,
  ) {
    let text_edit = TextEdit::singleline(&mut event.title).id(id.with("edit"));
    let response = ui.put(rect, text_edit);
    if response.lost_focus() || response.clicked_elsewhere() {
      ui.memory().data.remove::<EditingEvent>(id);
    }
  }

  fn show_label(
    &self,
    ui: &mut Ui,
    rect: Rect,
    event: &mut EventBlock,
    id: Id,
  ) {
    let button = Button::new(&event.title).sense(egui::Sense::hover());
    let response = ui.put(rect, button);
    if response.clicked_by(PointerButton::Primary) {
      ui.memory().data.insert_temp(id, EditingEvent);
    }
  }
}
