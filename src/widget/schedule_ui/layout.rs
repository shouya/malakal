use std::collections::HashMap;

// Layout algorithms for the schedule ui

type EventId = String;

pub struct Ev<'a> {
  id: &'a EventId,
  // span
  start: i64,
  end: i64,
}

impl<'a> From<(&'a EventId, i64, i64)> for Ev<'a> {
  fn from((id, start, end): (&'a EventId, i64, i64)) -> Self {
    Ev { id, start, end }
  }
}

#[derive(Default)]
pub struct Layout {
  // EventId => [left, right]
  layout: HashMap<EventId, [f32; 2]>,
}

impl Layout {
  fn from_map(layout: HashMap<EventId, [f32; 2]>) -> Self {
    Self { layout }
  }

  pub fn query(&self, id: &EventId) -> Option<[f32; 2]> {
    self.layout.get(id).cloned()
  }

  pub fn merge(&mut self, other: Layout) {
    self.layout.extend(other.layout)
  }
}

pub trait LayoutAlgorithm {
  fn compute(events: Vec<Ev<'_>>) -> Layout;
}

pub struct NaiveAlgorithm;

impl LayoutAlgorithm for NaiveAlgorithm {
  fn compute(mut events: Vec<Ev<'_>>) -> Layout {
    use intervaltree::{Element, IntervalTree};

    // EventId => (col, total_cols)
    let mut columns: HashMap<&EventId, (usize, usize)> = HashMap::new();

    events.sort_by_key(|e| e.start - e.end);

    let tree: IntervalTree<i64, &String> =
      events.iter().map(|e| (e.start..e.end, e.id)).collect();

    for e in events {
      if columns.contains_key(e.id) {
        continue;
      }

      let range = e.start..e.end;
      let mut siblings: Vec<_> = tree.query(range).collect();
      siblings.sort_by_key(|e| e.range.start);

      let num_cols = siblings.len();
      for (n, Element { value: id, .. }) in siblings.into_iter().enumerate() {
        columns.entry(*id).or_insert((n, num_cols));
      }
    }

    let mut layout = HashMap::new();
    for Element { value: id, .. } in tree {
      let (col0, tcol) = columns.get(&id).unwrap();
      let t0 = *col0 as f32 / *tcol as f32;
      let t1 = (*col0 + 1) as f32 / *tcol as f32;

      layout.insert(id.clone(), [t0, t1]);
    }

    Layout::from_map(layout)
  }
}

// https://stackoverflow.com/a/11323909
pub struct MarkusAlgorithm;

impl LayoutAlgorithm for MarkusAlgorithm {
  fn compute(mut events: Vec<Ev<'_>>) -> Layout {
    events.sort_by_key(|e| e.start);

    let ev_map: HashMap<_, _> = events.iter().map(|e| (e.id, e)).collect();

    let mut groups: Vec<(HashMap<&EventId, usize>, usize)> = vec![];

    let mut group: HashMap<&EventId, usize> = Default::default();
    let mut group_width: usize = 1;

    for event in events.iter() {
      if group.is_empty() {
        group.insert(event.id, 0);
        continue;
      }

      let overlapped = group.iter().any(|(id, _c)| overlaps(event, ev_map[id]));

      if !overlapped {
        // start a new group
        groups.push((group, group_width));
        group = Default::default();
        group_width = 1;

        group.insert(event.id, 0);
        continue;
      }

      // there is an overlap with existing group members, find a slot to put in
      for col in 0..=group_width {
        let slot_used = group
          .iter()
          .filter(|(_, &c)| c == col)
          .any(|(id, _c)| overlaps(event, ev_map[id]));

        if slot_used {
          continue;
        }

        group.insert(event.id, col);
        group_width = group_width.max(col + 1);
        break;
      }
    }

    if !group.is_empty() {
      groups.push((group, group_width));
    }

    let mut layout = HashMap::new();
    for (group, width) in groups {
      for (id, col) in group {
        let x0 = col as f32 / width as f32;
        let x1 = (col + 1) as f32 / width as f32;
        layout.insert(id.clone(), [x0, x1]);
      }
    }

    Layout::from_map(layout)
  }
}

fn overlaps(e1: &Ev, e2: &Ev) -> bool {
  e1.start.max(e2.start) < e1.end.min(e2.end)
}
