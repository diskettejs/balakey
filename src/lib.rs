#[macro_use]
extern crate napi_derive;

use glob::glob;

mod balakey;

#[napi]
pub fn hasher(patterns: Vec<String>) -> balakey::Balakey {
  // This is essentially a workaround for `glob` not accepting a pattern like `**/*.{png,ico,md}`
  let paths: Vec<_> = patterns
    .iter()
    .map(|p| {
      glob(p)
        .unwrap()
        .flat_map(std::result::Result::ok)
        .map(|pb| pb.display().to_string())
    })
    .flatten()
    .collect();

  balakey::Balakey::new(paths)
}
