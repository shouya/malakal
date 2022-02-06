use icalendar::{self, Component};

use crate::event::Event;

trait ICalLike {
  fn generate(&self, event: &Event) -> Option<String>;
  fn parse(&self, content: &str) -> Option<Event>;
}

struct ICal;

impl ICalLike for ICal {
  fn generate(&self, event: &Event) -> Option<String> {
    let mut ical_event = icalendar::Event::new();

    ical_event
      .uid(&event.id)
      .summary(&event.title)
      .starts(event.start.naive_local())
      .ends(event.end.naive_local());

    if let Some(description) = event.description.as_ref() {
      ical_event.description(description);
    }

    let ical_calendar = icalendar::Calendar::new()
      .name(&event.calendar)
      .push(ical_event.done())
      .done();

    Some(format!("{}", ical_calendar))
  }

  fn parse(&self, _content: &str) -> Option<Event> {
    // TODO
    None
  }
}
