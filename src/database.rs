use std::env;
use std::io;

use chrono::{DateTime, Utc};
use diesel::{self, PgConnection};
use diesel::query_dsl::BelongingToDsl;
use diesel::r2d2::ConnectionManager;
use fallible_iterator::FallibleIterator;
use futures::Future;
use futures_cpupool::CpuPool;
use postgres::{Connection, TlsMode};
use r2d2::Pool;
use uuid::Uuid;

use diesel::prelude::*;

use models::{AcoustIdLastCheck, MediaFileInfo, MusicBrainzRecording, NewMediaFileInfo};

fn get_database_url() -> String {
  env::var("DATABASE_URL").expect("DATABASE_URL must be set")
}

pub struct DatabaseConnection {
  pool: Pool<ConnectionManager<PgConnection>>,
  thread_pool: CpuPool,
}

impl DatabaseConnection {
  pub fn new(thread_pool: CpuPool) -> Self {
    let database_url = get_database_url();
    let manager = ConnectionManager::<PgConnection>::new(&*database_url);
    let pool = Pool::builder().build(manager).expect("Failed to create pool");

    Self {
      pool,
      thread_pool,
    }
  }

  pub fn insert_file(&self, info: &NewMediaFileInfo) -> impl Future<Item = MediaFileInfo, Error = io::Error> + Send {
    use schema::library;

    let db = self.pool.clone();
    let info = info.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let info = diesel::insert_into(library::table)
        .values(&info)
        .get_result(&conn)
        .expect("Error saving new media file entry");

      Ok(info)
    })
  }

  pub fn fetch_file(&self, file_path: String) -> impl Future<Item = Option<MediaFileInfo>, Error = io::Error> + Send {
    use schema::library::dsl::{library, path};

    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let info = library.filter(path.eq(&file_path))
        .first::<MediaFileInfo>(&conn)
        .optional()
        .expect("Error loading media file entry");

      Ok(info)
    })
  }

  pub fn update_file(&self, db_id: i32, info: NewMediaFileInfo) -> impl Future<Item = MediaFileInfo, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::library::dsl::{library, id};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let info = diesel::update(library)
        .filter(id.eq(db_id))
        .set(&info)
        .get_result::<MediaFileInfo>(&conn)
        .expect(&format!("Unable to find media file entry for id: {}", db_id));

      Ok(info)
    })
  }

  pub fn delete_file(&self, db_id: i32) -> impl Future<Item = (), Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::library::dsl::{library, id};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      diesel::delete(library)
        .filter(id.eq(db_id))
        .execute(&conn)
        .expect(&format!("Unable to delete media file entry for id: {}", db_id));

      Ok(())
    })
  }

  pub fn get_id(&self, info: &MediaFileInfo) -> impl Future<Item = i32, Error = io::Error> + Send {
    let db = self.pool.clone();
    let file_path = info.path.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::library::dsl::{library, id, path};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let path_id = library.filter(path.eq(&file_path))
        .select(id)
        .first::<i32>(&conn)
        .expect(&format!("Unable to get media file entry id for path: {}", file_path));

      Ok(path_id)
    })
  }

  pub fn get_acoustid_last_check(&self, info: MediaFileInfo) -> impl Future<Item = Option<DateTime<Utc>>, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::acoustid_last_checks::dsl::last_check;

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let last_check_time = AcoustIdLastCheck::belonging_to(&info)
        .select(last_check)
        .first(&conn)
        .optional()
        .expect(&format!("Unable to get acoustid last check for info: {:?}", info));;

      Ok(last_check_time)
    })
  }

  pub fn check_valid_recording_uuid(&self, uuid: &Uuid) -> impl Future<Item = bool, Error = io::Error> + Send {
    let db = self.pool.clone();
    let uuid = *uuid;

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let counts: Vec<MusicBrainzRecording> = diesel::sql_query(r#"
        SELECT
          COUNT(*) as count
        FROM "musicbrainz"."recording"
        WHERE "recording"."gid" = ?
      "#)
        .bind::<diesel::sql_types::Uuid, _>(uuid)
        .get_results(&conn)
        .expect("Error checking MusicBrainz UUID");

      debug!("uuid check count: {:?}", counts);

      Ok(counts[0].count > 0)
    })
  }

  pub fn update_file_uuid(&self, db_id: i32, uuid: Uuid) -> impl Future<Item = (), Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::library::dsl::{library, id, mbid};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      diesel::update(library)
        .filter(id.eq(db_id))
        .set(mbid.eq(uuid))
        .execute(&conn)
        .expect(&format!("Error updating media file entry mbid for id: {}", db_id));

      Ok(())
    })
  }

  pub fn add_acoustid_last_check(&self, db_library_id: i32, current_time: DateTime<Utc>) -> Box<Future<Item = (), Error = io::Error> + Send> {
    let db = self.pool.clone();

    let future = self.thread_pool.spawn_fn(move || {
      use schema::acoustid_last_checks::dsl::{acoustid_last_checks, last_check, library_id};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      diesel::insert_into(acoustid_last_checks)
        .values((
          library_id.eq(db_library_id),
          last_check.eq(current_time)
        ))
        .execute(&conn)
        .expect(&format!("Error adding last check for library id: {}", db_library_id));

      Ok(())
    });

    Box::new(future)
  }

  pub fn update_acoustid_last_check(&self, db_library_id: i32, current_time: DateTime<Utc>) -> Box<Future<Item = (), Error = io::Error> + Send> {
    let db = self.pool.clone();

    let future = self.thread_pool.spawn_fn(move || {
      use schema::acoustid_last_checks::dsl::{acoustid_last_checks, library_id, last_check};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      diesel::update(acoustid_last_checks)
        .filter(library_id.eq(db_library_id))
        .set(last_check.eq(current_time))
        .execute(&conn)
        .expect(&format!("Error updating last check for library id: {}", db_library_id));

      Ok(())
    });

    Box::new(future)
  }

  pub fn delete_acoustid_last_check(&self, db_library_id: i32) -> impl Future<Item = (), Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      use schema::acoustid_last_checks::dsl::{acoustid_last_checks, library_id};

      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      diesel::delete(acoustid_last_checks)
        .filter(library_id.eq(db_library_id))
        .execute(&conn)
        .expect(&format!("Error deleting last check for library id: {}", db_library_id));

      Ok(())
    })
  }

  pub fn path_iter<F: 'static>(&self, cb: F) -> Result<(), io::Error>
    where F: Fn(i32, String) -> ()
  {
    let database_url = get_database_url();
    let conn = try!(Connection::connect(&*database_url, TlsMode::None));
    let stmt = match conn.prepare("SELECT id, path FROM library") {
      Ok(v) => v,
      Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing path_iter statement: {:#?}", err))),
    };

    let trans = try!(conn.transaction());
    let mut rows = try!(stmt.lazy_query(&trans, &[], 100));

    while let Some(row) = rows.next()? {
      let id: i32 = row.get(0);
      let path: String = row.get(1);

      cb(id, path);
    }

    Ok(())
  }
}
