use crate::glob;
use blake3::Hasher;
use memmap2::Mmap;
use napi::Status;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::tokio::sync::{Mutex, mpsc};
use rayon::prelude::*;
use std::fs::File;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

const CHUNK_SIZE: usize = 128 * 1024 * 1024;

#[napi(object)]
pub struct SetStats {
  // Number of files hashed so far, including this one
  pub hashed: u32,
  // Number of files that failed so far
  pub failed: u32,
  // Total number of files in the set
  pub total: u32,
}

#[napi(object)]
pub struct Hashed {
  #[napi(ts_type = "true")]
  pub hashed: bool,
  // The absolute path of the file
  pub path: String,
  // The lowercase hexadecimal encoded string
  pub hash: String,
  // The amount of time it took to hash the file in seconds, includes the fractional (nanosecond).
  pub duration: f64,
  // Progress across the whole file set
  pub stats: SetStats,
}

#[napi(object)]
pub struct Failed {
  #[napi(ts_type = "false")]
  pub hashed: bool,
  // The absolute path of the file that could not be hashed
  pub path: String,
  // The underlying I/O error message
  pub error: String,
  // Progress across the whole file set
  pub stats: SetStats,
}

#[napi]
pub type Progress = Either<Hashed, Failed>;

#[napi(object)]
pub struct StartEvent {
  pub path: String,
  pub size: f64,
}

#[napi(object)]
pub struct ProgressEvent {
  pub path: String,
  pub bytes: f64,
  pub size: f64,
}

#[napi]
pub type StartCallback = ThreadsafeFunction<StartEvent, (), StartEvent, Status, false>;
#[napi]
pub type ProgressCallback = ThreadsafeFunction<ProgressEvent, (), ProgressEvent, Status, false>;

#[napi(object, object_to_js = false)]
pub struct HashOptions {
  pub on_start: Option<StartCallback>,
  pub on_progress: Option<ProgressCallback>,
}

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

#[napi(object, object_to_js = false)]
pub struct FileSetOptions {
  pub ignore: Option<Vec<String>>,
}

#[napi]
pub struct FileSet {
  root: String,
  paths: Vec<String>,
}

#[napi]
impl FileSet {
  #[napi(constructor)]
  pub fn new(root: String, paths: Vec<String>) -> Self {
    FileSet { root, paths }
  }

  #[napi(factory)]
  pub async fn from(
    root: String,
    pattern: Either<String, Vec<String>>,
    options: Option<FileSetOptions>,
  ) -> Result<Self> {
    let patterns = match pattern {
      Either::A(single) => vec![single],
      Either::B(many) => many,
    };
    let ignore = options.and_then(|o| o.ignore).unwrap_or_default();
    let root_dir = root.clone();
    let paths = spawn_blocking(move || glob::walk(Path::new(&root), &patterns, &ignore))
      .await
      .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))??;

    Ok(FileSet {
      root: root_dir,
      paths,
    })
  }

  #[napi(getter)]
  pub fn root(&self) -> String {
    self.root.clone()
  }

  #[napi(getter)]
  pub fn paths(&self) -> Vec<String> {
    self.paths.clone()
  }

  #[napi]
  pub fn hash(&self, options: Option<HashOptions>) -> HashStream {
    HashStream::start(self.paths.clone(), options)
  }
}

struct State {
  inner: Mutex<Inner>,
  total: u32,
}

struct Inner {
  rx: mpsc::UnboundedReceiver<Outcome>,
  succeeded: u32,
  failed: u32,
}

#[napi(async_iterator)]
pub struct HashStream {
  state: Arc<State>,
}

impl HashStream {
  fn start(paths: Vec<String>, options: Option<HashOptions>) -> Self {
    let total = paths.len() as u32;
    let (tx, rx) = mpsc::unbounded_channel();

    rayon::spawn(move || {
      let on_start = options.as_ref().and_then(|o| o.on_start.as_ref());
      let on_progress = options.as_ref().and_then(|o| o.on_progress.as_ref());

      let _ = paths.into_par_iter().try_for_each_with(tx, |tx, path| {
        tx.send(hash_file(path, on_start, on_progress))
          .map_err(|_| ())
      });
    });

    HashStream {
      state: Arc::new(State {
        inner: Mutex::new(Inner {
          rx,
          succeeded: 0,
          failed: 0,
        }),
        total,
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
      let mut inner = state.inner.lock().await;
      let Some(outcome) = inner.rx.recv().await else {
        return Ok(None);
      };

      match &outcome {
        Outcome::Hashed { .. } => inner.succeeded += 1,
        Outcome::Failed { .. } => inner.failed += 1,
      }
      let (succeeded, failed) = (inner.succeeded, inner.failed);
      drop(inner);

      let stats = SetStats {
        hashed: succeeded,
        failed,
        total: state.total,
      };
      let progress = match outcome {
        Outcome::Hashed {
          path,
          hash,
          duration,
        } => Either::A(Hashed {
          hashed: true,
          path,
          hash,
          duration,
          stats,
        }),
        Outcome::Failed { path, error } => Either::B(Failed {
          hashed: false,
          path,
          error,
          stats,
        }),
      };

      Ok(Some(progress))
    }
  }
}

fn hash_file(
  path: String,
  on_start: Option<&StartCallback>,
  on_progress: Option<&ProgressCallback>,
) -> Outcome {
  let start = Instant::now();
  let mut hasher = Hasher::new();

  // Fast path: nothing needs the file size, so hash it in one shot.
  if on_start.is_none() && on_progress.is_none() {
    if let Err(e) = hasher.update_mmap_rayon(&path) {
      return Outcome::Failed {
        path,
        error: e.to_string(),
      };
    }
    return Outcome::Hashed {
      path,
      hash: hasher.finalize().to_hex().to_string(),
      duration: start.elapsed().as_secs_f64(),
    };
  }

  let file = match File::open(&path) {
    Ok(file) => file,
    Err(e) => {
      return Outcome::Failed {
        path,
        error: e.to_string(),
      };
    }
  };

  let size = match file.metadata() {
    Ok(metadata) => metadata.len(),
    Err(e) => {
      return Outcome::Failed {
        path,
        error: e.to_string(),
      };
    }
  };

  if let Some(on_start) = on_start {
    on_start.call(
      StartEvent {
        path: path.clone(),
        size: size as f64,
      },
      ThreadsafeFunctionCallMode::NonBlocking,
    );
  }

  let result = match on_progress {
    Some(on_progress) => hash_chunked(&mut hasher, &file, &path, size, on_progress),
    None => hasher.update_mmap_rayon(&path).map(|_| ()),
  };

  match result {
    Ok(()) => Outcome::Hashed {
      path,
      hash: hasher.finalize().to_hex().to_string(),
      duration: start.elapsed().as_secs_f64(),
    },
    Err(e) => Outcome::Failed {
      path,
      error: e.to_string(),
    },
  }
}

fn hash_chunked(
  hasher: &mut Hasher,
  file: &File,
  path: &str,
  size: u64,
  on_progress: &ProgressCallback,
) -> std::io::Result<()> {
  let mmap = unsafe { Mmap::map(file)? };

  let len = mmap.len();
  let mut offset = 0;
  while offset < len {
    let end = (offset + CHUNK_SIZE).min(len);
    hasher.update_rayon(&mmap[offset..end]);
    offset = end;

    on_progress.call(
      ProgressEvent {
        path: path.to_string(),
        bytes: offset as f64,
        size: size as f64,
      },
      ThreadsafeFunctionCallMode::NonBlocking,
    );
  }

  Ok(())
}
