use std::io;
use std::sync::Arc;

use num_cpus;

use futures::Future;
use futures::future;
use futures_cpupool::Builder as CpuPoolBuilder;
use tokio_core::reactor::Core;

use chrono::prelude::*;

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use elasticsearch::ElasticSearch;
use file_scanner;
use models::MediaFileInfo;

use basic_types::*;

struct WorkUnit {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
  info: Arc<MediaFileInfo>,
}

struct ProcessorThread {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
}

pub struct Processor {
  config: Config,
}

impl ProcessorThread {
  fn insert_path_entry(work: WorkUnit) -> Box<Future<Item = (), Error = ProcessorError> + Send> {
    info!("path: {}", work.info.path);

    let conn1 = work.conn.clone();
    let conn2 = work.conn.clone();
    let acoustid = work.acoustid.clone();

    let info1 = work.info.clone();
    let info2 = work.info.clone();

    let add_future = work.conn.insert_file(&work.info.clone());
    let future = add_future.and_then(move |_| {
      conn1.get_id(&info1)
    }).and_then(move |id| {
      let acoustid_future = conn2.add_acoustid_last_check(id, Utc::now());

      let uuid_future = match acoustid.parse_file(&info2.path) {
	Ok(mbid) => conn2.update_file_uuid(info2.path.clone(), mbid),
	  Err(_) => Box::new(future::ok(())),
      };

      acoustid_future.join(uuid_future)
	.and_then(|(_, _)| Ok(()))
    }).map_err(|e| {
      error!("add_future.map_err: {:#?}", e);
      ProcessorError::NothingUseful
    });

    Box::new(future)
  }

  fn update_path_entry(work: WorkUnit, db_info: MediaFileInfo) -> Box<Future<Item = (), Error = ProcessorError> + Send> {
    if work.info.mbid != None {
      return Box::new(future::ok(()));
    }

    debug!("path does not have associated mbid: {}", work.info.path);

    let acoustid = work.acoustid.clone();
    let conn = work.conn.clone();

    let last_check = work.conn.get_acoustid_last_check(db_info.id);

    // Must use trait object or rust will not detect the correct boxing
    let future = last_check.and_then(move |last_check| -> Box<Future<Item = (), Error = io::Error> + Send> {
      let now: DateTime<Utc> = Utc::now();
      let difference = now.timestamp() - last_check.unwrap_or(Utc.timestamp(0, 0)).timestamp();

      // 2 weeks = 1,209,600 seconds
      if difference <= 1_209_600 {
        return Box::new(future::ok(()));
      }

      info!("path: {}", db_info.path);

      debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);
      let fetch_fingerprint = match acoustid.parse_file(&db_info.path) {
        Ok(mbid) => conn.update_file_uuid(db_info.path, mbid),
        Err(_) => Box::new(future::ok(())),
      };

      let last_check = match last_check {
        Some(_) => conn.update_acoustid_last_check(db_info.id, now),
        None => conn.add_acoustid_last_check(db_info.id, now),
      };

      let future = last_check.join(fetch_fingerprint)
        .and_then(|(_, _)| Ok(()));

      Box::new(future)
    }).map_err(|e| {
      error!("last_check.map_err: {:#?}", e);
      ProcessorError::NothingUseful
    });

    Box::new(future)
  }

  fn call(&self, req: MediaFileInfo) -> Box<Future<Item = (), Error = ProcessorError> + Send> {
    let work = WorkUnit {
      conn:	self.conn.clone(),
      acoustid:	self.acoustid.clone(),
      info:	Arc::new(req),
    };

    // Get the previous value from the database if it exists
    let fetch_future = work.conn.fetch_file(work.info.path.clone());

    let future = fetch_future.map_err(|e| {
      error!("process_path err: {:#?}", e);
      ProcessorError::NothingUseful
    }).and_then(move |db_info| {
      match db_info {
        Some(v) => Self::update_path_entry(work, v),
           None => Self::insert_path_entry(work),
      }
    });

    Box::new(future)
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
      None => return Err(ProcessorError::ApiKeyError),
    };

    let cores = num_cpus::get();

    let mut core = Core::new().unwrap();
    let thread_pool = CpuPoolBuilder::new()
      .pool_size(cores)
      .name_prefix("pool_thread")
      .create();

    let conn = Arc::new(DatabaseConnection::new(thread_pool.clone()));
    info!("Database Future: {:?}", conn);

    let search = ElasticSearch::new(thread_pool.clone(), core.handle());
    let index_exists_future = search.ensure_index_exists();
    core.run(index_exists_future).unwrap();

    let acoustid = Arc::new(AcoustId::new(api_key.clone()));

    let paths = &self.config.paths;
    for path in paths {
      println!("Scanning {}", path);

      let dir_walk = file_scanner::scan_dir(&path);
      let files = dir_walk.iter();

      let futures = files.map(|file| {
        let file = file.clone();
	let conn = conn.clone();
	let acoustid = acoustid.clone();

        let processing = ProcessorThread {
          conn,
          acoustid,
        };

        thread_pool.spawn_fn(move || {
          // A None value indicates a non-valid file instead of an error
          match MediaFileInfo::read_file(&file) {
            Some(v) => Ok(v),
            None => Err(ProcessorError::NothingUseful),
          }
        }).and_then(move |info| {
          processing.call(info)
        })
      });

      for future in futures {
        let res = future.wait();
        match res {
          Ok(v) => debug!("future: {:?}", v),

          Err(ProcessorError::NothingUseful) => (),
          Err(err) => error!("future error: {:?}", err),
        }
      }
    }

    Ok(Box::new(9000))
  }
}
