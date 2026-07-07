use crate::glob;
use blake3::Hasher;
use napi::bindgen_prelude::*;
use napi::tokio::sync::Mutex;
use napi::tokio::sync::mpsc;
use rayon::prelude::*;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

#[napi(object)]
pub struct Hashed {
  #[napi(ts_type = "true")]
  pub hashed: bool,
  // The absolute path of the file
  pub path: String,
  // Total number of files in the set
  pub total: u32,
  // Number of files hashed so far, including this one
  pub succeeded: u32,
  // Number of files that failed so far
  pub failed: u32,
  // The lowercase hexadecimal encoded string
  pub hash: String,
  // The amount of time it took to hash the file in seconds, includes the fractional (nanosecond).
  pub duration: f64,
}

#[napi(object)]
pub struct Failed {
  #[napi(ts_type = "false")]
  pub hashed: bool,
  // The absolute path of the file that could not be hashed
  pub path: String,
  // Total number of files in the set
  pub total: u32,
  // Number of files hashed so far
  pub succeeded: u32,
  // Number of files that failed so far, including this one
  pub failed: u32,
  // The underlying I/O error message
  pub error: String,
}

#[napi]
pub type Progress = Either<Hashed, Failed>;

enum Outcome {
  Hashed {
    path: String,
    hash: String,
    duration: f64,
  },
  Failed {
    path: String,
    error: String,
  },
}

#[napi]
pub struct FileSet {
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

struct State {
  rx: Mutex<mpsc::UnboundedReceiver<Outcome>>,
  total: u32,
  succeeded: AtomicU32,
  failed: AtomicU32,
}

#[napi(async_iterator)]
pub struct HashStream {
  state: Arc<State>,
}

impl HashStream {
  fn start(paths: Vec<String>) -> Self {
    let total = paths.len() as u32;
    let (tx, rx) = mpsc::unbounded_channel();

    rayon::spawn(move || {
      let _ = paths
        .into_par_iter()
        .try_for_each_with(tx, |tx, path| tx.send(hash_file(path)).map_err(|_| ()));
    });

    HashStream {
      state: Arc::new(State {
        rx: Mutex::new(rx),
        total,
        succeeded: AtomicU32::new(0),
        failed: AtomicU32::new(0),
      }),
    }
  }
}

#[napi]
impl AsyncGenerator for HashStream {
  type Yield = Progress;
  type Next = ();
  type Return = ();

  fn next(
    &mut self,
    _value: Option<Self::Next>,
  ) -> impl Future<Output = Result<Option<Self::Yield>>> + Send + 'static {
    let state = self.state.clone();

    async move {
      let mut rx = state.rx.lock().await;
      let Some(outcome) = rx.recv().await else {
        return Ok(None);
      };

      let (succeeded, failed) = match &outcome {
        Outcome::Hashed { .. } => {
          let succeeded = state.succeeded.fetch_add(1, Ordering::Relaxed) + 1;
          (succeeded, state.failed.load(Ordering::Relaxed))
        }
        Outcome::Failed { .. } => {
          let failed = state.failed.fetch_add(1, Ordering::Relaxed) + 1;
          (state.succeeded.load(Ordering::Relaxed), failed)
        }
      };
      drop(rx);

      let total = state.total;
      let progress = match outcome {
        Outcome::Hashed {
          path,
          hash,
          duration,
        } => Either::A(Hashed {
          hashed: true,
          path,
          total,
          succeeded,
          failed,
          hash,
          duration,
        }),
        Outcome::Failed { path, error } => Either::B(Failed {
          hashed: false,
          path,
          total,
          succeeded,
          failed,
          error,
        }),
      };

      Ok(Some(progress))
    }
  }
}

fn hash_file(path: String) -> Outcome {
  let mut hasher = Hasher::new();
  let start = Instant::now();

  if let Err(e) = hasher.update_mmap_rayon(&path) {
    return Outcome::Failed {
      path,
      error: e.to_string(),
    };
  }

  let hash = hasher.finalize();

  Outcome::Hashed {
    path,
    hash: hash.to_hex().to_string(),
    duration: start.elapsed().as_secs_f64(),
  }
}
