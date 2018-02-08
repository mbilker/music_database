use walkdir::{DirEntry, WalkDir};

fn is_hidden(entry: &DirEntry) -> bool {
  entry.file_name()
    .to_str()
    .map(|s| s.starts_with('.'))
    .unwrap_or(false)
}

fn is_dir(entry: &DirEntry) -> bool {
  entry.file_type()
    .is_dir()
}

pub fn scan_dir(path: &str) -> Vec<String> {
  let iter = WalkDir::new(path)
    .into_iter()
    .filter_entry(|e| !is_hidden(e))
    .filter_map(|e| e.ok())
    // Filtering directories cannot be done earlier as it will
    // prevent walkdir from recursing into directories and
    // cause there to be no output
    .filter(|e| !is_dir(e))
    .map(|e| e.path().to_str().unwrap().to_owned());

  let items: Vec<String> = iter.collect();

  items
}
