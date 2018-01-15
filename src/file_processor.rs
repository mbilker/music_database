use std::io;
use std::sync::Arc;

use futures::Future;
use futures::future;

use chrono::prelude::*;

use acoustid::AcoustId;
use database::DatabaseConnection;
use models::MediaFileInfo;

use basic_types::*;

macro_rules! wrap_err {
  ($x:expr) => {
    $x.map_err(ProcessorError::from)
  }
}

struct WorkUnit {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
  info: Arc<MediaFileInfo>,
}

pub struct FileProcessor {
  acoustid: Arc<AcoustId>,
  conn: Arc<DatabaseConnection>,
}

impl FileProcessor {
  pub fn new(acoustid: &Arc<AcoustId>, conn: &Arc<DatabaseConnection>) -> Self {
    let acoustid = Arc::clone(acoustid);
    let conn = Arc::clone(conn);

    Self {
      acoustid,
      conn,
    }
  }

  pub fn call(self, info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let acoustid = Arc::clone(&self.acoustid);
    let conn = Arc::clone(&self.conn);

    let path = info.path.clone();

    let work = WorkUnit {
      acoustid,
      conn,

      info: Arc::new(info),
    };

    // Get the previous value from the database if it exists
    let fetch_future = wrap_err!(self.conn.fetch_file(&path));

    let future = fetch_future.and_then(move |db_info| {
      match db_info {
        Some(v) => Self::update_path_entry(&work, v),
           None => Self::insert_path_entry(&work),
      }
    });

    Box::new(future)
  }

  fn insert_path_entry(work: &WorkUnit) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    info!("path: {}", work.info.path);

    let conn1 = Arc::clone(&work.conn);
    let conn2 = Arc::clone(&work.conn);
    let acoustid = Arc::clone(&work.acoustid);

    let info = Arc::clone(&work.info);

    let add_future = wrap_err!(work.conn.insert_file(&Arc::clone(&work.info)));

    let future = add_future.and_then(move |_| {
      let path = info.path.clone();

      let acoustid_future = acoustid.parse_file(info.path.clone());
      let info_future = wrap_err!(conn1.fetch_file(&path)).and_then(|info| {
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
      let uuid: Box<Future<Item = (), Error = io::Error>> = match mbid {
        Some(mbid) => Box::new(conn2.update_file_uuid(info.id, mbid)),
        None => Box::new(future::ok(())),
      };

      wrap_err!(last_check
        .join(uuid))
        .and_then(|(_, _)| Ok(info))
    });

    Box::new(future)
  }

  fn update_path_entry(work: &WorkUnit, db_info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let id = db_info.id;

    macro_rules! check_fields {
      ( $($x:ident),* ) => {
        $((work.info.$x != db_info.$x) || )* false
      }
    }

    // Update the database with the file metadata read from the actual file
    // if the database entry differs from the read file metadata
    let needs_update = check_fields!(title, artist, album, track, track_number, duration);
    let update_future: Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> = if needs_update {
      info!("not equal, info: {:#?}, db_info: {:#?}", work.info, db_info);

      let conn = Arc::clone(&work.conn);
      let path = work.info.path.clone();
      Box::new(
        wrap_err!(work.conn.update_file(id, (*work.info).clone()))
          .and_then(move |_| wrap_err!(conn.fetch_file(&path)))
          .and_then(|info| Ok(info.unwrap()))
      )
    } else {
      Box::new(future::ok(db_info))
    };

    // Return early if the entry already has a MusicBrainz ID associated with it
    if work.info.mbid != None {
      return update_future;
    }

    debug!("path does not have associated mbid: {}", work.info.path);

    let acoustid = Arc::clone(&work.acoustid);
    let conn = Arc::clone(&work.conn);

    let last_check = wrap_err!(work.conn.get_acoustid_last_check(id));
    let joined = update_future.join(last_check);

    // Must use trait object or rust will not detect the correct boxing
    let future = joined.and_then(move |(db_info, last_check)| -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
      let now: DateTime<Utc> = Utc::now();
      let difference = now.timestamp() - last_check.unwrap_or_else(|| Utc.timestamp(0, 0)).timestamp();

      // 2 weeks = 1,209,600 seconds
      if difference < 1_209_600 {
        debug!("id: {}, last check within 2 weeks, not re-checking", id);
        return Box::new(future::ok(db_info));
      }

      info!("id: {}, path: {}", id, db_info.path);

      debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);
      let conn2 = Arc::clone(&conn);
      let fetch_fingerprint = acoustid.parse_file(db_info.path.clone())
        .and_then(move |mbid| -> Box<Future<Item = (), Error = ProcessorError>> {
          match mbid {
            Some(mbid) => Box::new(wrap_err!(conn2.update_file_uuid(id, mbid))),
            None => Box::new(future::ok(())),
          }
        });

      let last_check = wrap_err!(match last_check {
        Some(_) => conn.update_acoustid_last_check(id, now),
           None => conn.add_acoustid_last_check(id, now),
      });

      let future = last_check
        .join(fetch_fingerprint)
        .inspect(|&(arg1, _)| debug!("last_check add/update: {:?}", arg1))
        .and_then(|(_, _)| Ok(db_info));

      Box::new(future)
    });

    Box::new(future)
  }
}
