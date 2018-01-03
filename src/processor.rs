use std::path::Path;
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
  pub fn new(config: &'a Config) -> Self {
    let api_key = config.api_keys.get("acoustid").expect("Failed to get Acoustid API key");

    let cores = num_cpus::get();

    let mut core = Core::new().unwrap();
    let thread_pool = CpuPoolBuilder::new()
      .pool_size(cores)
      .name_prefix("pool_thread")
      .create();

    let acoustid = Arc::new(AcoustId::new(api_key.clone(), thread_pool.clone(), core.handle()));
    let conn = Arc::new(DatabaseConnection::new(thread_pool.clone()));
    let search = Arc::new(ElasticSearch::new(thread_pool.clone(), core.handle()));

    debug!("Database Connection: {:?}", conn);

    let future = search.ensure_index_exists();
    core.run(future).expect("Failed to create Elasticsearch index");

    Self {
      paths: &config.paths,

      core,
      thread_pool,

      acoustid,
      conn,
      search,
    }
  }

  pub fn prune_db(&mut self) -> Result<(), ProcessorError> {
    let conn = self.conn.clone();
    let remote = self.core.remote();

    let future = self.conn.path_iter(move |id, path| {
      let path = Path::new(&path);
      if !path.exists() {
        println!("id: {}, path: {:?}", id, path);

        let conn2 = conn.clone();

        let future = conn.delete_acoustid_last_check(id)
          .and_then(move |_| conn2.delete_file(id))
          .and_then(move |_| {
            info!("id: {} deleted", id);
            Ok(())
          })
          .map_err(move |e| {
            error!("error deleting id = {}: {:#?}", id, e);
          });

        remote.spawn(move |_| future);
      }
    })
    .map_err(|e| e.into());

    self.core.run(future)
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
      });

      self.core.run(handler).unwrap();
    }

    Ok(Box::new(9000))
  }
}
