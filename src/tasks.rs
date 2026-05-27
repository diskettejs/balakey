use crate::glob;
use crate::shared::*;
use blake3::Hasher;
use napi::bindgen_prelude::*;
use rayon::prelude::*;
use std::time::Instant;

pub struct HashFilesTask {
  paths: Vec<String>,
}

#[napi]
impl Task for HashFilesTask {
  type Output = Vec<HashResult>;
  type JsValue = Vec<HashResult>;

  fn compute(&mut self) -> Result<Self::Output> {
    self
      .paths
      .par_iter()
      .map(|path| {
        let mut hasher = Hasher::new();
        let start = Instant::now();

        // Propagate I/O errors up as napi Errors rather than panicking.
        // Files could realistically disappear between
        // glob expansion and hashing given that we process them all at once.
        hasher
          .update_mmap(path)
          .map_err(|e| Error::from_reason(format!("Failed to hash {path}: {e}",)))?;

        let hash = hasher.finalize();
        let result = HashResult {
          path: path.clone(),
          hash: hash.to_hex().to_string(),
          duration: start.elapsed().as_secs_f64(),
        };

        Ok(result)
      })
      .collect()
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
    Ok(output)
  }
}

#[napi]
pub fn hash_files(patterns: Vec<String>, signal: Option<AbortSignal>) -> AsyncTask<HashFilesTask> {
  let paths = glob::expand(patterns);

  AsyncTask::with_optional_signal(HashFilesTask { paths }, signal)
}
