use std::path::PathBuf;

use derive_builder::Builder;

#[derive(Builder)]
#[builder(try_setter, setter(into))]
struct Local {
  _dir: PathBuf,
}
