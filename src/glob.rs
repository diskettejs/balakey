use glob::glob;

// This is essentially a workaround for the fact that `glob` doesn't support a pattern like `**/*.{png,ico,md}`
pub fn expand(patterns: Vec<String>) -> Vec<String> {
  let paths: Vec<_> = patterns
    .iter()
    .map(|p| {
      glob(p)
        .unwrap()
        .flat_map(std::result::Result::ok)
        .map(|pb| pb.display().to_string())
    })
    .flatten()
    .collect();

  paths
}
