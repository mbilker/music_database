use num_cpus;

use crossbeam::sync::MsQueue;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use std::io;
use std::thread;

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use file_scanner;
use fingerprint;
use models::MediaFileInfo;

pub struct Processor {
  config: Config,
}

#[derive(Debug)]
pub enum ProcessorError {
  ApiKeyError(),

  IoError(io::Error),
  ThreadError(String),
  MutexError(String),
  FingerprintError(fingerprint::FingerprintError),
}

impl From<io::Error> for ProcessorError {
  fn from(value: io::Error) -> ProcessorError {
    ProcessorError::IoError(value)
  }
}

impl From<fingerprint::FingerprintError> for ProcessorError {
  fn from(value: fingerprint::FingerprintError) -> ProcessorError {
    ProcessorError::FingerprintError(value)
  }
}

impl Processor {
  pub fn new(config: Config) -> Self {
    Self {
      config: config,
    }
  }

  pub fn scan_dirs(&self) -> Result<Box<i32>, ProcessorError> {
    let api_key = match self.config.api_keys.get("acoustid") {
      Some(v) => v,
      None => return Err(ProcessorError::ApiKeyError()),
    };

    let cores = num_cpus::get();

    let is_done_processing = Arc::new(AtomicBool::new(false));
    let work_queue: Arc<MsQueue<String>> = Arc::new(MsQueue::new());
    let mut threads = Vec::new();

    let acoustid = Arc::new(AcoustId::new(api_key));

    // Spawn a worker thread for each core
    for i in 0..cores {
      // Clone variables for the thread
      let is_done_processing = is_done_processing.clone();
      let work_queue = work_queue.clone();

      let acoustid = acoustid.clone();

      // Construct the thread
      let builder = thread::Builder::new().name(format!("processor {}", i).into());
      let handler = try!(builder.spawn(move || {
        let conn = DatabaseConnection::new().unwrap();
        info!("Database Connection: {:?}", conn);

        loop {
          let path = work_queue.try_pop();
          if let Some(path) = path {
            info!("path: {}", path);

            // A None value indicates a non-valid file instead of an error
            if let Some(info) = MediaFileInfo::read_file(&path) {
              if !info.is_default_values() {
                conn.insert_file(&info);

                let (duration, fingerprint) = fingerprint::get(&path).unwrap();
                let result = acoustid.lookup(duration, &fingerprint);
                if let Some(result) = result {
                  if let Some(recordings) = result.recordings {
                    let first = recordings.first().unwrap();
                    conn.update_file_uuid(&path, &first.id);
                  }
                } else {
                  error!("error fetching fingerprint for: {:#?}", info);
                }
              }
            }
          } else {
            // Break the loop if there is the signal to indicate no more items
            // are being added to queue
            if is_done_processing.load(Ordering::Relaxed) {
              info!("No more work");
              break;
            }
          }
        }
      }));

      // Save the thread handle so the thread can be joined later      
      threads.push(handler);
    }

    let paths = &self.config.paths;
    for path in paths {
      println!("Scanning {}", path);

      let dir_walk = file_scanner::scan_dir(&path);
      let files = dir_walk.iter();

      for file in files {
        // Push every file onto the queue so the workers can process the files
        work_queue.push(file.clone());
      }
    }

    // Signal the threads to exit
    is_done_processing.store(true, Ordering::Relaxed);

    // Join the thread handles
    for thread in threads {
      if let Err(err) = thread.join() {
        return Err(ProcessorError::ThreadError(format!("{:?}", err)));
      }
    }

    Ok(Box::new(9000))
  }
}
