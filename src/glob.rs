use napi::bindgen_prelude::{Error, Result, Status, spawn_blocking};
use std::collections::HashSet;
use std::path::Path;
use wax::{
  Glob,
  walk::{Entry, FileIterator},
};

#[napi(object, object_to_js = false)]
pub struct FileSetOptions {
  pub ignore: Option<Vec<String>>,
}

pub async fn expand(
  root: String,
  pattern: napi::Either<String, Vec<String>>,
  options: Option<FileSetOptions>,
) -> Result<Vec<String>> {
  let patterns = match pattern {
    napi::Either::A(single) => vec![single],
    napi::Either::B(many) => many,
  };
  let ignore = options.and_then(|o| o.ignore).unwrap_or_default();

  let paths = spawn_blocking(move || walk(std::path::Path::new(&root), &patterns, &ignore))
    .await
    .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))??;

  Ok(paths)
}

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
