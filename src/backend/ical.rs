use chrono::DateTime;

use crate::event::{Event, EventBuilder};

pub(crate) struct ICal;

impl ICal {
  fn generate(&self, event: &Event) -> Option<String> {
    use ics::{properties::*, *};

    let mut ical_cal = ICalendar::new("2.0", "-//malakal/malakal//EN");
    ical_cal.add_timezone(TimeZone::standard(
      "UTC",
      Standard::new("19700329T020000", "+0000", "+0000"),
    ));
    ical_cal.push(CalScale::new("GREGORIAN"));
    ical_cal.push(Method::new("PUBLISH"));

    let mut ical_event =
      ics::Event::new(&event.id, to_timestamp(event.created_at));
    ical_event.push(DtStart::new(to_timestamp(event.start)));
    ical_event.push(DtEnd::new(to_timestamp(event.end)));

    ical_cal.add_event(ical_event);

    Some(ical_cal.to_string())
  }

  fn parse(&self, _content: &str) -> Option<Event> {
    // TODO: show warning
    // let ical_calendar = icalendar::Calendar::from_str(content).ok()?;
    // for component in ical_calendar.iter() {
    //   if let Some(ical_event) = component.as_event() {
    //     let mut ev = EventBuilder::default();
    //     ev.calendar = ical_calendar.prope
    //     return ev.build().ok();
    //   }
    None
  }
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_event_generation() {
    // let s = include_str!("/home/shou/.calendar/time-blocking/8ab34a78-559a-421c-9314-1fa03929ae25.ics");

    // println!("{:?}", ICal.parse(&s));

    let e = EventBuilder::default()
      .id(";1")
      .title("E: gaming")
      .calendar("time-blocking")
      .start(DateTime::parse_from_rfc3339("2022-02-26T23:00:00+08:00").unwrap())
      .end(DateTime::parse_from_rfc3339("2022-02-26T23:59:59+08:00").unwrap())
      .build()
      .unwrap();

    println!("{}", ICal.generate(&e).unwrap());

    assert!(false);
  }
}

fn to_timestamp<Tz: chrono::TimeZone>(time: DateTime<Tz>) -> String {
  time.naive_utc().format("%Y%m%dT%H%M%SZ").to_string()
}
