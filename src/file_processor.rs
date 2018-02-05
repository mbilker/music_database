use std::sync::Arc;

use futures::Future;
use futures::future;
use futures_cpupool::CpuPool;

use chrono::prelude::*;

use acoustid::AcoustId;
use database::DatabaseConnection;
use models::{MediaFileInfo, NewMediaFileInfo};

use basic_types::*;

macro_rules! wrap_err {
  ($x:expr) => {
    $x.map_err(ProcessorError::from)
  }
}

pub struct FileProcessor {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,

  thread_pool: CpuPool,
}

impl FileProcessor {
  pub fn new(acoustid: &Arc<AcoustId>, conn: &Arc<DatabaseConnection>, thread_pool: CpuPool) -> Self {
    let acoustid = Arc::clone(acoustid);
    let conn = Arc::clone(conn);

    Self {
      acoustid,
      conn,

      thread_pool,
    }
  }

  pub fn call(self, path: String) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    // Get the previous value from the database if it exists
    let fetch_future = wrap_err!(self.conn.fetch_file(path.clone()));

    // If there is an entry in the database corresponding to the provided file-
    // path, then check if the mtime has changed.
    //
    // If there is no entry in the database, then check if this is a valid file
    // by checking if `NewMediaFileInfo::read_file(path)` returns a Some value
    let future = fetch_future.and_then(move |db_info| match db_info {
      Some(v) => self.check_if_update_needed(path, v),
         None => self.insert_path_entry(path),
    });

    Box::new(future)
  }

  fn insert_path_entry(self, path: String) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let conn = Arc::clone(&self.conn);

    // Only insert entry into database if it is a valid file
    let future = self.read_file_info(&path)
      .and_then(move |info| {
        // Log the path after reading the file so invalid files are not printed
        info!("new file: {}", info.path);

        wrap_err!(conn.insert_file(&info))
      })
      .and_then(move |info| {
        let id = info.id;
        let conn = Arc::clone(&self.conn);

        let last_check = wrap_err!(self.conn.add_acoustid_last_check(id, Utc::now()));
        let acoustid = self.acoustid.parse_file(&path)
          .and_then(move |mbid| {
            wrap_err!(conn.update_file_uuid(id, mbid))
          })
          .or_else(|err| match err {
            ProcessorError::NoFingerprintMatch => Ok(()),
            _ => Err(err),
          });

        last_check
          .join(acoustid)
          .and_then(|(_, _)| Ok(info))
      });

    Box::new(future)
  }

  fn read_file_info(&self, path: &str) -> impl Future<Item = NewMediaFileInfo, Error = ProcessorError> {
    let path = path.to_string();

    self.thread_pool.spawn_fn(move || {
      // A None value indicates a non-valid file
      NewMediaFileInfo::read_file(&path).ok_or(ProcessorError::NothingUseful)
    })
  }

  fn check_if_update_needed(self, path: String, db_info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let mtime = NewMediaFileInfo::get_mtime(&path);
    if mtime != db_info.mtime {
      Box::new(
        self.read_file_info(&path)
          .and_then(move |info| self.update_path_entry(info, db_info))
      )
    } else if db_info.mbid == None {
      Box::new(self.handle_acoustid(db_info))
    } else {
      Box::new(future::ok(db_info))
    }
  }

  fn update_path_entry(self, info: NewMediaFileInfo, db_info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let id = db_info.id;

    macro_rules! check_fields {
      ( $($x:ident),* ) => {
        $((info.$x != db_info.$x) || )* false
      }
    }

    macro_rules! is_field_not_equal {
      ($x:ident) => {
        if info.$x != db_info.$x {
          let name = stringify!($x);
          debug!("id: {}, field not equal: info.{} = {:?}, db_info.{} = {:?}", id, name, info.$x, name, db_info.$x);
        }
      }
    }

    // Update the database with the file metadata read from the actual file
    // if the database entry differs from the read file metadata. The
    // modification time field is skipped since it was checked in
    // `check_if_update_needed(path, db_info)`.
    let needs_update = check_fields!(title, artist, album, track, track_number, duration);
    let update_future: Box<Future<Item = MediaFileInfo, Error = ProcessorError>> = if needs_update {
      info!("not equal, info: {:#?}, db_info: {:#?}", info, db_info);

      is_field_not_equal!(title);
      is_field_not_equal!(artist);
      is_field_not_equal!(album);
      is_field_not_equal!(track);
      is_field_not_equal!(track_number);
      is_field_not_equal!(duration);

      let info = info.clone();
      Box::new(
        wrap_err!(self.conn.update_file(id, info))
      )
    } else {
      Box::new(future::ok(db_info.clone()))
    };

    // Return early if the entry already has a MusicBrainz ID associated with it
    if let Some(mbid) = db_info.mbid {
      debug!("id: {}, path: {}, associated mbid: {:?}", db_info.id, db_info.path, mbid);
      return update_future;
    }

    debug!("id: {}, path: {}, no associated mbid", db_info.id, db_info.path);

    let future = update_future.and_then(move |db_info| self.handle_acoustid(db_info));
    Box::new(future)
  }

  fn handle_acoustid(self, db_info: MediaFileInfo) -> impl Future<Item = MediaFileInfo, Error = ProcessorError> {
    let id = db_info.id;

    let conn = Arc::clone(&self.conn);

    wrap_err!(self.conn.get_acoustid_last_check(db_info.clone()))
      .and_then(move |last_check| -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
        let now = Utc::now();
        let difference = now.timestamp() - last_check.unwrap_or_else(|| Utc.timestamp(0, 0)).timestamp();

        // 2 weeks = 1,209,600 seconds
        if difference < 1_209_600 {
          debug!("id: {}, path: {}, last check within 2 weeks, not re-checking", id, db_info.path);
          return Box::new(future::ok(db_info));
        }

        info!("id: {}, path: {}, checking for mbid match", id, db_info.path);
        debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);

        let fetch_fingerprint = self.acoustid.parse_file(&db_info.path)
          .and_then(move |mbid| {
            debug!("id: {}, new mbid: {}", id, mbid);
            wrap_err!(conn.update_file_uuid(id, mbid))
          })
          .or_else(|err| match err {
            ProcessorError::NoFingerprintMatch => Ok(()),
            _ => Err(err),
          });

        let last_check = wrap_err!(match last_check {
          Some(_) => self.conn.update_acoustid_last_check(id, now),
             None => self.conn.add_acoustid_last_check(id, now),
        });

        let future = last_check
          .join(fetch_fingerprint)
          .and_then(|(_, _)| Ok(db_info));

        Box::new(future)
      })
  }
}
