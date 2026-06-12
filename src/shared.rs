#[napi(object)]
pub struct HashResult {
  // The absolute path of the file
  pub path: String,
  // The lowercase hexadecimal encoded string
  pub hash: String,
  // The amount of time it took to hash the file in seconds, includes the fractional (nanosecond).
  pub duration: f64,
}

#[napi(object)]
pub struct HashError {
  // The absolute path of the file that could not be hashed
  pub path: String,
  // The underlying I/O error message
  pub error: String,
}
