use walkdir::{DirEntry, WalkDir, WalkDirIterator};

fn is_hidden(entry: &DirEntry) -> bool {
  entry.file_name()
    .to_str()
    .map(|s| s.starts_with("."))
    .unwrap_or(false)
}

pub fn scan_dir(path: &str) {
  let iter = WalkDir::new(path)
    .into_iter()
    .filter_entry(|e| !is_hidden(e));

  for i in iter {
    match i {
      Ok(_) => (),
      Err(err) => println!("err: {:?}", err),
    }
  }

/*
    .filter_map(|e| e.ok())
    .map(|e| e.path().to_str().unwrap().to_owned())
    .collect()
*/
}
