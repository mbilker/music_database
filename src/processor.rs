use num_cpus;

use crossbeam::sync::MsQueue;
use uuid::Uuid;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use file_scanner;
use fingerprint;
use models::MediaFileInfo;

use basic_types::*;

fn fetch_fingerprint_result(acoustid: &AcoustId, path: &String) -> Result<Uuid, ProcessorError> {
  // Eat up fingerprinting errors, I mostly see them when a file is not easily
  // parsed like WAV files
  let (duration, fingerprint) = try!(fingerprint::get(&path));
  
  let result = try!(acoustid.lookup(duration, &fingerprint));
  if let Some(result) = result {
    if let Some(recordings) = result.recordings {
      let first = recordings.first().unwrap();
      return Ok(first.id.clone());
    }
  }

  Err(ProcessorError::NoFingerprintMatch())
}

pub struct Processor {
  config: Config,
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
            // A None value indicates a non-valid file instead of an error
            if let Some(info) = MediaFileInfo::read_file(&path) {
              // Get the previous value from the database if it exists
              if let Some(db_info) = conn.fetch_file(&path) {
                // TODO(mbilker): handle cases where the uuid is missing
                if db_info.mbid == None {
                  debug!("path does not have associated mbid: {}", path);

                  let last_check = conn.get_acoustid_last_check(db_info.id);

                  let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::new(0, 0)).as_secs();
                  let difference = now - last_check.unwrap_or(0);

                  // 2 weeks = 1,209,600 seconds
                  if difference > 1_209_600 {
                    info!("path: {}", path);

                    debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);
                    if let Ok(mbid) = fetch_fingerprint_result(&acoustid, &path) {
                      conn.update_file_uuid(&path, &mbid);
                    }
                    if last_check == None {
                      conn.add_acoustid_last_check(db_info.id);
                    } else {
                      conn.update_acoustid_last_check(db_info.id);
                    }
                  }
                }
              } else {
                info!("path: {}", path);

                conn.insert_file(&info);
                let id = conn.get_id(&info);

                if let Ok(mbid) = fetch_fingerprint_result(&acoustid, &path) {
                  conn.update_file_uuid(&path, &mbid);
                }
                conn.add_acoustid_last_check(id);
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
