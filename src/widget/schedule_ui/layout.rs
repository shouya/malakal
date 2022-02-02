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

    // First step: assign each event a column
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
