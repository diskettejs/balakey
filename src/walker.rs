use crate::glob;
use crate::shared::*;
use blake3::Hasher;
use napi::bindgen_prelude::*;
use std::future::Future;
use std::time::Instant;

#[napi(async_iterator)]
pub struct Walker {
  paths: Vec<String>,
  index: usize,
}

#[napi]
impl AsyncGenerator for Walker {
  type Yield = HashResult;
  type Next = ();
  type Return = ();

  // TODO: refactor this to not panic if `update_mmap_rayon` returns an error
  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let item = if self.index < self.paths.len() {
      let mut hasher = Hasher::new();
      let path = self.paths[self.index].clone();
      let start = Instant::now();

      // `update_mmap_rayon` could possibly error if `path` can't be opened
      //  but unreadable paths should've been filtered out by the constructor
      //  So if the path can't be opened at this point, something changed that's out of our control
      hasher
        .update_mmap_rayon(&path)
        .expect(&format!("Unable to open file: {}", path).to_string());

      let hash = hasher.finalize();

      self.index += 1;

      Some(HashResult {
        path,
        duration: start.elapsed().as_secs_f64(),
        hash: hash.to_hex().to_string(),
      })
    } else {
      None
    };

    async move { Ok(item) }
  }
}

#[napi]
impl Walker {
  #[napi(constructor)]
  pub fn new(paths: Vec<String>) -> Self {
    Walker { paths, index: 0 }
  }
}

#[napi]
pub fn hasher(patterns: Vec<String>) -> Walker {
  let paths = glob::expand(patterns);

  Walker::new(paths)
}
