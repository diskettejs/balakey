#[napi(object)]
pub struct HashResult {
  // The absolute path of the file
  pub path: String,
  // The lowercase hexadecimal encoded string
  pub hash: String,
  // The amount of time it took to hash the file in seconds, includes the fractional (nanosecond).
  pub duration: f64,
}
