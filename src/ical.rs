use anyhow::{bail, ensure};
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
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
    let parse_time = |p: Property| -> Result<DateTime<Utc>> {
      let s = value(p.clone())?;
      let tzid = p.params.and_then(|params| {
        params.into_iter().find_map(|(n, v)| {
          (n == "TZID")
            .then_some(())
            .and_then(|_| v.into_iter().next())
        })
      });
      from_timestamp(&s, tzid.as_deref())
    };

    event.calendar(calendar_name);

    let mut start = None;

    for p in ical_event.properties {
      match p.name.as_str() {
        "UID" => event.id(value(p)?),
        "SUMMARY" => event.title(value(p)?),
        "DTSTAMP" => event.created_at(parse_time(p)?),
        "DTSTART" => {
          start = Some(parse_time(p)?);
          event.start(start.unwrap())
        }
        "DTEND" => event.end(parse_time(p)?),
        "DURATION" => {
          let value = value(p)?;
          let start =
            start.ok_or_else(|| anyhow!("duration: start not defined yet"))?;
          let end = start + parse_duration(&value)?;
          event.end(end)
        }
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
  use chrono_tz::Tz;
  use std::str::FromStr;

  if let Ok(t) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ") {
    return Ok(t.and_utc());
  }

  if let Some(tz) = tzid.and_then(|tz| Tz::from_str(tz).ok()) {
    if let Ok(t) = NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
      return Ok(t.and_local_timezone(tz).unwrap().with_timezone(&Utc));
    }
  }

  bail!("failed to parse timestamp {}", s)
}

fn parse_duration(s: &str) -> Result<Duration> {
  let reg = regex::Regex::new(r"PT((?P<h>\d+)H)?((?P<m>\d+)M)?")?;
  let cap = reg
    .captures(s)
    .ok_or_else(|| anyhow!("Invalid duration parsed {}", s))?;

  let mut dur = Duration::zero();
  if let Some(m) = cap.name("h") {
    let hours = m.as_str().parse::<i64>()?;
    dur += Duration::hours(hours);
  }
  if let Some(m) = cap.name("m") {
    let mins = m.as_str().parse::<i64>()?;
    dur += Duration::minutes(mins);
  }

  Ok(dur)
}
