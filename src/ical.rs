use anyhow::{bail, ensure};
use chrono::{DateTime, Local, Utc};
use ical::property::Property;

use crate::event::{Event, EventBuilder};
use crate::util::{anyhow, Result};

pub(crate) struct ICal;

impl ICal {
  pub fn generate(&self, event: &Event) -> Result<String> {
    use ics::{properties::*, *};

    let mut ical_cal = ICalendar::new("2.0", "malakal");
    ical_cal.add_timezone(TimeZone::standard(
      "UTC",
      Standard::new("19700329T020000", "+0000", "+0000"),
    ));
    ical_cal.push(CalScale::new("GREGORIAN"));

    let mut ical_event =
      ics::Event::new(&event.id, to_timestamp(event.timestamp));
    ical_event.push(DtStart::new(to_timestamp(event.start)));
    ical_event.push(DtEnd::new(to_timestamp(event.end)));
    ical_event.push(LastModified::new(to_timestamp(event.modified_at)));
    ical_event.push(Created::new(to_timestamp(event.created_at)));

    ical_event.push(Summary::new(&event.title));
    if let Some(desc) = &event.description {
      ical_event.push(Description::new(desc));
    }

    ical_cal.add_event(ical_event);

    Ok(ical_cal.to_string())
  }

  pub fn parse(&self, calendar_name: &str, content: &str) -> Result<Event> {
    use ical::parser::ical::IcalParser;

    let ical_cal = IcalParser::new(content.as_bytes())
      .next()
      .ok_or_else(|| anyhow!("ics file contains only no calendar"))??;

    ensure!(!ical_cal.events.is_empty(), "ics file contains no events");
    ensure!(
      ical_cal.events.len() == 1,
      "ics file contains more than one events"
    );

    let ical_event = ical_cal.events.into_iter().next().unwrap();
    let mut event = EventBuilder::default();

    let value = |p: Property| -> Result<String> {
      p.value
        .ok_or_else(|| anyhow!("property {} doesn't have value", &p.name))
    };
    let parse_time = |p: Property| -> Result<DateTime<Local>> {
      let s = value(p.clone())?;
      let tzid = p.params.and_then(|params| {
        params.into_iter().find_map(|(n, v)| {
          (n == "TZID").then(|| ()).and_then(|_| v.into_iter().next())
        })
      });
      let t = from_timestamp(&s, tzid.as_deref())?;
      Ok(t.with_timezone(&Local))
    };

    event.calendar(calendar_name);

    for p in ical_event.properties {
      match p.name.as_str() {
        "UID" => event.id(value(p)?),
        "SUMMARY" => event.title(value(p)?),
        "DTSTAMP" => event.created_at(parse_time(p)?),
        "DTSTART" => event.start(parse_time(p)?),
        "DTEND" => event.end(parse_time(p)?),
        "CREATED" => event.created_at(parse_time(p)?),
        "LAST-MODIFIED" => event.modified_at(parse_time(p)?),
        _ => &mut event,
      };
    }

    Ok(event.build()?)
  }
}

fn to_timestamp<Tz: chrono::TimeZone>(time: DateTime<Tz>) -> String {
  time.naive_utc().format("%Y%m%dT%H%M%SZ").to_string()
}

fn from_timestamp(s: &str, tzid: Option<&str>) -> Result<DateTime<Utc>> {
  use chrono::offset::TimeZone;
  use chrono_tz::Tz;
  use std::str::FromStr;

  if let Ok(t) = Utc.datetime_from_str(s, "%Y%m%dT%H%M%SZ") {
    return Ok(t);
  }

  if let Some(tz) = tzid.and_then(|tz| Tz::from_str(tz).ok()) {
    if let Ok(t) = tz.datetime_from_str(s, "%Y%m%dT%H%M%S") {
      return Ok(t.with_timezone(&Utc));
    }
  }

  bail!("failed to parse timestamp {}", s)
}
