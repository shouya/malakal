use anyhow::{bail, ensure};
use ical::property::Property;
use jiff::{SignedDuration, Timestamp, civil::DateTime, tz::TimeZone};

use crate::event::{Event, EventBuilder};
use crate::util::{Result, anyhow};

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
        .ok_or_else(|| anyhow!("property {} doesn't have value", p.name))
    };
    let parse_time = |p: Property| -> Result<Timestamp> {
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
          let end = start + value.parse::<SignedDuration>()?;
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

fn to_timestamp(time: Timestamp) -> String {
  time.strftime("%Y%m%dT%H%M%SZ").to_string()
}

fn from_timestamp(s: &str, tzid: Option<&str>) -> Result<Timestamp> {
  if let Ok(t) = DateTime::strptime("%Y%m%dT%H%M%SZ", s) {
    return Ok(TimeZone::UTC.to_ambiguous_timestamp(t).compatible()?);
  }

  if let Some(tz) = tzid.and_then(|tz| TimeZone::get(tz).ok())
    && let Ok(t) = DateTime::strptime("%Y%m%dT%H%M%S", s)
  {
    return Ok(tz.to_ambiguous_timestamp(t).compatible()?);
  }

  bail!("failed to parse timestamp {}", s)
}
