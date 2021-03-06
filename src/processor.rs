use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use futures::{Future, Stream};
use futures::{future, stream};
use futures_cpupool::{Builder as CpuPoolBuilder, CpuPool};
use tokio_core::reactor::Core;

use acoustid::AcoustId;
use config::Config;
use database::DatabaseConnection;
use elasticsearch::ElasticSearch;
use scanner;
use file_processor::FileProcessor;

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

    let mut core = Core::new().unwrap();
    let thread_pool = CpuPoolBuilder::new()
      .name_prefix("pool_thread")
      .create();

    let acoustid = Arc::new(AcoustId::new(api_key.clone(), thread_pool.clone(), &core.handle()));
    let conn = Arc::new(DatabaseConnection::new(thread_pool.clone()));
    let search = Arc::new(ElasticSearch::new(thread_pool.clone(), &core.handle()));

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
    let conn = Arc::clone(&self.conn);
    let futures = Rc::new(Mutex::new(Vec::new()));

    let futures2 = Rc::clone(&futures);

    let cb = move |id, path| {
      let path = Path::new(&path);
      if !path.exists() {
        println!("id: {}, path: {:?}", id, path);

        let conn2 = Arc::clone(&conn);

        let future = conn.delete_acoustid_last_check(id)
          .and_then(move |_| conn2.delete_file(id))
          .and_then(move |_| {
            info!("id: {} deleted", id);
            Ok(())
          })
          .map_err(move |e| {
            error!("error deleting id = {}: {:#?}", id, e);
            ProcessorError::NothingUseful
          });

        futures2.lock().unwrap().push(future);
      }
    };

    try!(self.conn.path_iter(cb));

    let mut futures = futures.lock().unwrap();
    let futures: Vec<_> = futures.drain(..).collect();
    try!(self.core.run(future::join_all(futures)));

    Ok(())
  }

  pub fn scan_dirs(&mut self) -> Result<Box<i32>, ProcessorError> {
    for path in self.paths {
      println!("Scanning {}", path);

      let dir_walk = scanner::scan_dir(path);
      let files: Vec<String> = dir_walk.to_vec();

      debug!("files length: {}", files.len());

      let thread_pool = self.thread_pool.clone();

      let acoustid = Arc::clone(&self.acoustid);
      let conn = Arc::clone(&self.conn);
      let search = Arc::clone(&self.search);

      let handler = stream::iter_ok(files).and_then(move |file| {
        let thread_pool = thread_pool.clone();
        let worker = FileProcessor::new(&acoustid, &conn, thread_pool);
        worker.call(file)
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
      }).or_else(|err| match err {
        ProcessorError::NothingUseful => Ok(()),
        _ => Err(err),
      }).for_each(|_| {
        Ok(())
      });

      self.core.run(handler).unwrap();
    }

    Ok(Box::new(9000))
  }
}
