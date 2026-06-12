use crate::glob;
use crate::shared::*;
use blake3::Hasher;
use napi::bindgen_prelude::*;
use rayon::prelude::*;
use std::future::Future;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::sync::mpsc;

#[napi]
pub struct FileSet {
  // Snapshot of the glob expansion taken at construction; every hash() pass
  // covers this same set, so `paths` always describes exactly what a pass
  // will yield.
  paths: Vec<String>,
}

#[napi]
impl FileSet {
  #[napi(constructor)]
  pub fn new(patterns: Vec<String>) -> Self {
    FileSet {
      paths: glob::expand(patterns),
    }
  }

  #[napi(getter)]
  pub fn paths(&self) -> Vec<String> {
    self.paths.clone()
  }

  #[napi]
  pub fn hash(&self) -> HashStream {
    HashStream::start(self.paths.clone())
  }
}

#[napi(async_iterator)]
pub struct HashStream {
  // `next()` must return a `'static + Send` future, so it can't borrow the
  // receiver from `&mut self` — it takes a clone of the Arc instead.
  rx: Arc<Mutex<mpsc::UnboundedReceiver<Either<HashResult, HashError>>>>,
}

impl HashStream {
  // Files are hashed in parallel across the rayon pool and yielded in
  // completion order, so consumption from JS never gates hashing throughput.
  fn start(paths: Vec<String>) -> Self {
    let (tx, rx) = mpsc::unbounded_channel();

    rayon::spawn(move || {
      // `send` fails only when the receiver is gone (the stream was dropped
      // before being drained) — short-circuit instead of hashing the rest.
      let _ = paths
        .into_par_iter()
        .try_for_each_with(tx, |tx, path| tx.send(hash_file(path)).map_err(|_| ()));
    });

    HashStream {
      rx: Arc::new(Mutex::new(rx)),
    }
  }
}

#[napi]
impl AsyncGenerator for HashStream {
  type Yield = Either<HashResult, HashError>;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let rx = self.rx.clone();

    async move { Ok(rx.lock().await.recv().await) }
  }
}

// I/O failures are yielded as `HashError` entries rather than rejecting the
// iterator — files can disappear between glob expansion and hashing, and a
// single bad file must not abort the rest of the stream.
fn hash_file(path: String) -> Either<HashResult, HashError> {
  let mut hasher = Hasher::new();
  let start = Instant::now();

  if let Err(e) = hasher.update_mmap_rayon(&path) {
    return Either::B(HashError {
      path,
      error: e.to_string(),
    });
  }

  let hash = hasher.finalize();

  Either::A(HashResult {
    path,
    hash: hash.to_hex().to_string(),
    duration: start.elapsed().as_secs_f64(),
  })
}
