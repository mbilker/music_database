use std::sync::Arc;

use num_cpus;

use futures::{Future, Stream};
use futures::future;
use futures::stream;
use futures_cpupool::{Builder as CpuPoolBuilder, CpuPool};
use tokio_core::reactor::Core;

use chrono::prelude::*;

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use elasticsearch::ElasticSearch;
use file_scanner;
use models::MediaFileInfo;

use basic_types::*;

macro_rules! wrap_err {
  ($x:expr) => {
    $x.map_err(|e| ProcessorError::from(e))
  }
}

struct WorkUnit {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
  info: Arc<MediaFileInfo>,
}

struct DatabaseThread {
}

pub struct Processor<'a> {
  paths: &'a Vec<String>,

  thread_pool: CpuPool,

  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
}

impl DatabaseThread {
  fn insert_path_entry(work: WorkUnit) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> {
    info!("path: {}", work.info.path);

    let conn1 = work.conn.clone();
    let conn2 = work.conn.clone();
    let acoustid = work.acoustid.clone();

    let info = work.info.clone();

    let add_future = wrap_err!(work.conn.insert_file(&work.info.clone()));

    let future = add_future.and_then(move |_| {
      let acoustid_future = acoustid.parse_file(info.path.clone());
      let info_future = wrap_err!(conn1.fetch_file(info.path.clone())).and_then(|info| {
        // If a database row is not returned after adding it, there is an issue and the
        // error is appropriate here
        match info {
          Some(v) => Ok(v),
             None => Err(ProcessorError::NothingUseful),
        }
      });

      acoustid_future.join(info_future)
    }).and_then(move |(mbid, info)| {
      let last_check = conn2.add_acoustid_last_check(info.id, Utc::now());
      let uuid = conn2.update_file_uuid(info.id, mbid);

      wrap_err!(last_check
        .join(uuid))
	.and_then(|(_, _)| Ok(info))
    });

    Box::new(future)
  }

  fn update_path_entry(work: WorkUnit, db_info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> {
    if work.info.mbid != None {
      return Box::new(future::ok(db_info));
    }

    debug!("path does not have associated mbid: {}", work.info.path);

    let acoustid = work.acoustid.clone();
    let conn = work.conn.clone();

    let last_check = wrap_err!(work.conn.get_acoustid_last_check(db_info.id));

    // Must use trait object or rust will not detect the correct boxing
    let future = last_check.and_then(move |last_check| -> Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> {
      let now: DateTime<Utc> = Utc::now();
      let difference = now.timestamp() - last_check.unwrap_or(Utc.timestamp(0, 0)).timestamp();

      // 2 weeks = 1,209,600 seconds
      if difference < 1_209_600 {
        debug!("id: {}, last check within 2 weeks, not re-checking", db_info.id);
        return Box::new(future::ok(db_info));
      }

      let id = db_info.id;
      info!("id: {}, path: {}", id, db_info.path);

      debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);
      let conn2 = conn.clone();
      let fetch_fingerprint = acoustid.parse_file(db_info.path.clone())
        .and_then(move |mbid|
          wrap_err!(conn2.update_file_uuid(id, mbid))
        );

      let last_check = match last_check {
        Some(_) => conn.update_acoustid_last_check(id, now),
           None => conn.add_acoustid_last_check(id, now),
      };

      let future = last_check
        .map_err(|e| ProcessorError::from(e))
        .join(fetch_fingerprint)
        .inspect(|&(arg1, _)| debug!("last_check add/update: {:?}", arg1))
        .and_then(|(_, _)| Ok(db_info));

      Box::new(future)
    });

    Box::new(future)
  }

  fn call(work: WorkUnit) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> {
    // Get the previous value from the database if it exists
    let fetch_future = wrap_err!(work.conn.fetch_file(work.info.path.clone()));

    let future = fetch_future.and_then(move |db_info| {
      match db_info {
        Some(v) => Self::update_path_entry(work, v),
           None => Self::insert_path_entry(work),
      }
    });

    Box::new(future)
  }
}

impl<'a> Processor<'a> {
  pub fn new(config: &'a Config) -> Result<Self, ProcessorError> {
    let api_key = match config.api_keys.get("acoustid") {
      Some(v) => v,
      None => return Err(ProcessorError::ApiKeyError),
    };

    let cores = num_cpus::get();

    let thread_pool = CpuPoolBuilder::new()
      .pool_size(cores)
      .name_prefix("pool_thread")
      .create();

    let acoustid = Arc::new(AcoustId::new(api_key.clone(), thread_pool.clone()));

    let conn = Arc::new(DatabaseConnection::new(thread_pool.clone()));
    info!("Database Future: {:?}", conn);

    Ok(Self {
      paths: &config.paths,

      thread_pool,

      acoustid,
      conn,
    })
  }

  pub fn scan_dirs(&self) -> Result<Box<i32>, ProcessorError> {
    let mut core = Core::new().unwrap();

    let search = Arc::new(ElasticSearch::new(self.thread_pool.clone(), core.handle()));
    let index_exists_future = search.ensure_index_exists();
    core.run(index_exists_future).unwrap();

    let test = stream::iter_ok(vec!["test1", "foo", "bar", "foobar"]).and_then(|item| {
      if false {
        return Err(());
      }

      info!("stream test, item: {:?}", item);
      Ok(())
    }).for_each(|_| {
      Ok(())
    });
    core.run(test).unwrap();

    for path in self.paths {
      println!("Scanning {}", path);

      let dir_walk = file_scanner::scan_dir(&path);
      let files: Vec<String> = dir_walk.iter().map(|e| e.clone()).collect();

      debug!("files length: {}", files.len());

      let thread_pool = self.thread_pool.clone();

      let acoustid = self.acoustid.clone();
      let conn = self.conn.clone();
      let search = search.clone();

      let handler = stream::iter_ok(files).and_then(|file| {
        thread_pool.spawn_fn(move || {
          // A None value indicates a non-valid file instead of an error
          Ok(MediaFileInfo::read_file(&file))
        })
      }).filter_map(|info| {
        info
      }).and_then(move |info| {
        let acoustid = acoustid.clone();
        let conn = conn.clone();

        let work = WorkUnit {
          acoustid,
          conn,

          info: Arc::new(info),
        };

        DatabaseThread::call(work)
      }).for_each(move |info| {
        info!("info: {:?}", info);

        let doc = info.to_document();

        search.insert_document(doc)
          .map_err(|e| {
            error!("elastic error: {:#?}", e);
            ProcessorError::NothingUseful
          })
          .and_then(|res| {
            debug!("elastic insert res: {:?}", res);
            Ok(())
          })
      }).map_err(|e| {
        error!("err: {:#?}", e);
      });

      println!("did we get here?");

      core.run(handler).unwrap();
    }

    Ok(Box::new(9000))
  }
}
