use std::sync::Arc;

use num_cpus;

use futures::{Future, Stream};
use futures::stream;
use futures_cpupool::{Builder as CpuPoolBuilder, CpuPool};
use tokio_core::reactor::Core;

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use elasticsearch::ElasticSearch;
use file_scanner;
use file_processor::FileProcessor;
use models::MediaFileInfo;

use basic_types::*;

pub struct Processor<'a> {
  paths: &'a Vec<String>,

  core: Core,
  thread_pool: CpuPool,

  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
  search: Arc<ElasticSearch>,
}

impl<'a> Processor<'a> {
  pub fn new(config: &'a Config) -> Result<Self, ProcessorError> {
    let api_key = match config.api_keys.get("acoustid") {
      Some(v) => v,
      None => return Err(ProcessorError::ApiKeyError),
    };

    let mut core = Core::new().unwrap();

    let cores = num_cpus::get();

    let thread_pool = CpuPoolBuilder::new()
      .pool_size(cores)
      .name_prefix("pool_thread")
      .create();

    let acoustid = Arc::new(AcoustId::new(api_key.clone(), thread_pool.clone(), core.handle()));
    let conn = Arc::new(DatabaseConnection::new(thread_pool.clone()));
    let search = Arc::new(ElasticSearch::new(thread_pool.clone(), core.handle()));

    debug!("Database Connection: {:?}", conn);

    core.run(search.ensure_index_exists()).unwrap();

    Ok(Self {
      paths: &config.paths,

      core,
      thread_pool,

      acoustid,
      conn,
      search,
    })
  }

  pub fn scan_dirs(&mut self) -> Result<Box<i32>, ProcessorError> {
    for path in self.paths {
      println!("Scanning {}", path);

      let dir_walk = file_scanner::scan_dir(&path);
      let files: Vec<String> = dir_walk.iter().map(|e| e.clone()).collect();

      debug!("files length: {}", files.len());

      let thread_pool = self.thread_pool.clone();

      let acoustid = self.acoustid.clone();
      let conn = self.conn.clone();
      let search = self.search.clone();

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

        let worker = FileProcessor::new(acoustid, conn);
        worker.call(info)
      }).and_then(move |info| {
        let doc = info.to_document();

        search.insert_document(doc)
          .map_err(|e| {
            error!("elastic error: {:#?}", e);
            ProcessorError::NothingUseful
          })
          .and_then(|res| {
            trace!("elastic insert res: {:?}", res);
            Ok(())
          })
      }).for_each(|_| {
        Ok(())
      }).map_err(|e| {
        error!("err: {:#?}", e);
      });

      println!("did we get here?");

      self.core.run(handler).unwrap();
    }

    Ok(Box::new(9000))
  }
}
