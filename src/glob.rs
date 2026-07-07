use napi::bindgen_prelude::{Error, Result, Status};
use std::collections::HashSet;
use std::path::Path;
use wax::Glob;
use wax::walk::{Entry, FileIterator};

pub fn walk(root: &Path, patterns: &[String], ignore: &[String]) -> Result<Vec<String>> {
  if !root.is_dir() {
    return Err(Error::new(
      Status::InvalidArg,
      format!("root is not a directory: {}", root.display()),
    ));
  }

  let mut paths: HashSet<String> = HashSet::new();

  for pattern in patterns {
    let glob = Glob::new(pattern)
      .map_err(|e| Error::new(Status::InvalidArg, format!("invalid glob `{pattern}`: {e}")))?;

    let walker = glob
      .walk(root)
      .not(wax::any(ignore.iter().map(String::as_str)))
      .map_err(|e| Error::new(Status::InvalidArg, format!("invalid ignore pattern: {e}")))?;

    for entry in walker.flatten() {
      if entry.file_type().is_file() {
        paths.insert(entry.into_path().display().to_string());
      }
    }
  }

  Ok(paths.into_iter().collect())
}
