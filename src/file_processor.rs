use std::sync::Arc;

use futures::Future;
use futures::future;

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

  pub fn call(self, info: NewMediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let path = info.path.clone();

    // Get the previous value from the database if it exists
    let fetch_future = wrap_err!(self.conn.fetch_file(path));

    let future = fetch_future.and_then(move |db_info| match db_info {
      Some(v) => self.update_path_entry(&info, v),
         None => self.insert_path_entry(&info),
    });

    Box::new(future)
  }

  fn insert_path_entry(self, info: &NewMediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    info!("path: {}", info.path);

    let path = info.path.clone();
    let conn = Arc::clone(&self.conn);

    let future = wrap_err!(self.conn.insert_file(info))
      .and_then(move |info| {
        let id = info.id;

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

  fn update_path_entry(self, info: &NewMediaFileInfo, db_info: MediaFileInfo) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
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
    // if the database entry differs from the read file metadata
    let needs_update = check_fields!(title, artist, album, track, track_number, duration, mtime);
    let update_future: Box<Future<Item = MediaFileInfo, Error = ProcessorError> + Send> = if needs_update {
      info!("not equal, info: {:#?}, db_info: {:#?}", info, db_info);

      is_field_not_equal!(title);
      is_field_not_equal!(artist);
      is_field_not_equal!(album);
      is_field_not_equal!(track);
      is_field_not_equal!(track_number);
      is_field_not_equal!(duration);
      is_field_not_equal!(mtime);

      Box::new(
        wrap_err!(self.conn.update_file(id, info.clone()))
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

    let last_check = wrap_err!(self.conn.get_acoustid_last_check(db_info));
    let joined = update_future.join(last_check);

    // Must use trait object or rust will not detect the correct boxing
    let future = joined.and_then(move |(db_info, last_check)| self.handle_acoustid(db_info, last_check));

    Box::new(future)
  }

  fn handle_acoustid(self, db_info: MediaFileInfo, last_check: Option<DateTime<Utc>>) -> Box<Future<Item = MediaFileInfo, Error = ProcessorError>> {
    let id = db_info.id;

    let now: DateTime<Utc> = Utc::now();
    let difference = now.timestamp() - last_check.unwrap_or_else(|| Utc.timestamp(0, 0)).timestamp();

    // 2 weeks = 1,209,600 seconds
    if difference < 1_209_600 {
      debug!("id: {}, path: {}, last check within 2 weeks, not re-checking", id, db_info.path);
      return Box::new(future::ok(db_info));
    }

    info!("id: {}, path: {}, checking for mbid match", id, db_info.path);
    debug!("updating mbid (now: {} - last_check: {:?} = {})", now, last_check, difference);

    let conn = Arc::clone(&self.conn);
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
  }
}
