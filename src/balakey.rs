use blake3::Hasher;
use napi::bindgen_prelude::*;
use std::future::Future;
use std::time::Instant;

#[napi(object)]
pub struct HashResult {
  // The absolute path of the file
  pub path: String,
  // The lowercase hexadecimal encoded string
  pub hash: String,
  // The amount of time it took to hash the file in seconds, includes the fractional (nanosecond).
  pub duration: f64,
}

#[napi(async_iterator)]
pub struct Balakey {
  paths: Vec<String>,
  index: usize,
  hasher: Hasher,
}

#[napi]
impl AsyncGenerator for Balakey {
  type Yield = HashResult;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let item = if self.index < self.paths.len() {
      let path = self.paths[self.index].clone();
      let start = Instant::now();

      // `update_mmap_rayon` could possibly error if `path` can't be opened
      //  but unreadable paths should've been filtered out by the constructor
      //  So if the path can't be opened at this point, something changed that's out of our control
      self
        .hasher
        .update_mmap_rayon(&path)
        .expect(&format!("Unable to open file: {}", path).to_string());

      let hash = self.hasher.finalize();
      let duration = start.elapsed();

      self.hasher.reset();

      let result = HashResult {
        path,
        duration: duration.as_secs_f64(),
        hash: hash.to_hex().to_string(),
      };

      Some(result)
    } else {
      None
    };
    self.index += 1;

    async move { Ok(item) }
  }
}

#[napi]
impl Balakey {
  #[napi(constructor)]
  pub fn new(paths: Vec<String>) -> Self {
    Balakey {
      paths,
      index: 0,
      hasher: Hasher::new(),
    }
  }
}
