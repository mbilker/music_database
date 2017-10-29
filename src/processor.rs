use num_cpus;

use crossbeam::sync::MsQueue;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use config::Config;
use database::DatabaseConnection;
use file_scanner;
use models::MediaFileInfo;

pub struct Processor {
  config: Config,
}

impl Processor {
  pub fn new(config: Config) -> Self {
    Self {
      config: config,
    }
  }

  pub fn scan_dirs(&self) {
    let cores = num_cpus::get();

    let is_done_processing = Arc::new(AtomicBool::new(false));
    let work_queue: Arc<MsQueue<String>> = Arc::new(MsQueue::new());
    let mut threads = Vec::new();

    for i in 0..cores {
      let is_done_processing = is_done_processing.clone();
      let work_queue = work_queue.clone();

      let builder = thread::Builder::new().name(format!("processor {}", i).into());
      let handler = builder.spawn(move || {
        let conn = DatabaseConnection::new().unwrap();
        info!("Database Connection: {:?}", conn);

        loop {
          let path = work_queue.try_pop();
          if let Some(path) = path {
            info!("path: {}", path);
            if let Some(info) = MediaFileInfo::read_file(&path) {
              if !info.is_default_values() {
                conn.insert_file(info);
              }
            }
          } else {
            if is_done_processing.load(Ordering::Relaxed) {
              // Break the loop if there is the signal to indicate no more items
              // are being added to queue
              info!("No more work");
              break;
            }
          }
        }
      }).unwrap();
      
      threads.push(handler);
    }

    let paths = &self.config.paths;
    for path in paths {
      println!("Scanning {}", path);

      let dir_walk = file_scanner::scan_dir(&path);
      let files = dir_walk.iter();

      for file in files {
        work_queue.push(file.clone());
      }
    }

    is_done_processing.store(true, Ordering::Relaxed);

    for thread in threads {
      thread.join().unwrap();
    }
  }
}
