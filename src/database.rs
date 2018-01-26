use std::env;
use std::io;

use chrono::{DateTime, Utc};
use diesel::{self, PgConnection};
use diesel::r2d2::ConnectionManager;
use fallible_iterator::FallibleIterator;
use futures::Future;
use futures_cpupool::CpuPool;
use r2d2::Pool;
use uuid::Uuid;

use diesel::prelude::*;

use models::{MediaFileInfo, NewMediaFileInfo};

pub struct DatabaseConnection {
  pool: Pool<ConnectionManager<PgConnection>>,
  thread_pool: CpuPool,
}

impl DatabaseConnection {
  pub fn new(thread_pool: CpuPool) -> Self {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let manager = ConnectionManager::<PgConnection>::new(&*database_url);
    let pool = Pool::builder().build(manager).expect("Failed to create pool");

    Self {
      pool,
      thread_pool,
    }
  }

  pub fn insert_file(&self, info: NewMediaFileInfo) -> impl Future<Item = MediaFileInfo, Error = io::Error> + Send {
    use schema::library;

    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let info = diesel::insert_into(library::table)
        .values(&info)
        .get_result(&*conn)
        .expect("Error saving new media file entry");

      Ok(info)
    })
  }

  pub fn fetch_file(&self, file_path: &str) -> impl Future<Item = Option<MediaFileInfo>, Error = io::Error> + Send {
    use schema::library::dsl::*;

    let db = self.pool.clone();
    let file_path = file_path.to_string();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let info = library.filter(path.eq(&file_path))
        .first::<MediaFileInfo>(&*conn)
        .expect("Error loading media file entry");

      Ok(Some(info))
    })
  }

  pub fn update_file(&self, id: i32, info: MediaFileInfo) -> impl Future<Item = u64, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        UPDATE library
        SET
          title = $2,
          artist = $3,
          album = $4,
          track = $5,
          track_number = $6,
          duration = $7
        WHERE id = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing update_file statement: {:?}", err))),
      };

      let res = statement.execute(&[
        &id,
        &info.title,
        &info.artist,
        &info.album,
        &info.track,
        &info.track_number,
        &info.duration,
      ]);

      match res {
        Ok(v) => Ok(v),
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("unexpected error with update_file update: {:?}", err))),
      }
    })
  }

  pub fn delete_file(&self, id: i32) -> impl Future<Item = u64, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        DELETE
        FROM library
        WHERE id = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing delete_file statement: {:#?}", err))),
      };

      let res = statement.execute(&[
        &id,
      ]);

      match res {
        Ok(v) => Ok(v),
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("unexpected error with delete_file delete: {:?}", err))),
      }
    })
  }

  pub fn get_id(&self, info: &MediaFileInfo) -> impl Future<Item = i32, Error = io::Error> + Send {
    let db = self.pool.clone();
    let path = info.path.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        SELECT
          id
        FROM library
        WHERE path = $1
      "#) {
        Ok(v) => v,
        Err(err) => panic!("error preparing get_id statement: {:#?}", err),
      };

      let rows = match statement.query(&[
        &path
      ]) {
        Ok(v) => v,
        Err(err) => panic!("error retrieving id from database: {:#?}", err),
      };

      let row = rows.get(0);

      let id: i32 = row.get(0);
      Ok(id)
    })
  }

  pub fn get_acoustid_last_check(&self, id: i32) -> impl Future<Item = Option<DateTime<Utc>>, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        SELECT
          last_check
        FROM acoustid_last_checks
        WHERE library_id = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing get_acoustid_last_check statement: {:#?}", err))),
      };

      let rows = match statement.query(&[
        &id
      ]) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error retrieving last_check from database: {:#?}", err))),
      };

      // If the value does not exist in the database, return 0
      if rows.is_empty() {
        return Ok(None);
      }

      let row = rows.get(0);
      let last_check = row.get(0);

      Ok(Some(last_check))
    })
  }

  pub fn check_valid_recording_uuid(&self, uuid: &Uuid) -> impl Future<Item = bool, Error = io::Error> + Send {
    let db = self.pool.clone();
    let uuid = *uuid;

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        SELECT
          COUNT(*)
        FROM "musicbrainz"."recording"
        WHERE "recording"."gid" = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing check_valid_recording_uuid statement: {:#?}", err))),
      };

      let res = statement.query(&[
        &uuid
      ]);

      let rows = match res {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error checking valid recording uuid: {:?}", err))),
      };

      let row = rows.get(0);
      let count: i32 = row.get(0);
      debug!("uuid check count: {}", count);

      Ok(count > 0)
    })
  }

  pub fn update_file_uuid(&self, id: i32, uuid: Uuid) -> impl Future<Item = (), Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        UPDATE library
        SET mbid = $2
        WHERE id = $1
      "#) {
        Ok(v) => v,
        Err(err) => panic!("error preparing update_file_uuid statement: {:#?}", err),
      };

      let res = statement.execute(&[
        &id,
        &uuid
      ]);

      if let Err(err) = res {
        panic!("unexpected error with file_uuid update: {:#?}", err);
      }

      Ok(())
    })
  }

  pub fn add_acoustid_last_check(&self, library_id: i32, current_time: DateTime<Utc>) -> Box<Future<Item = u64, Error = io::Error> + Send> {
    let db = self.pool.clone();

    let future = self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        INSERT INTO acoustid_last_checks (
          library_id,
          last_check
        ) VALUES ($1, $2)
      "#) {
        Ok(v) => v,
        Err(err) => panic!("error preparing add_acoustid_last_check statement: {:#?}", err),
      };

      let res = statement.execute(&[
        &library_id,
        &current_time
      ]);

      match res {
        Ok(v) => Ok(v),
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("unexpected error with last_check insert: {:#?}", err))),
      }
    });

    Box::new(future)
  }

  pub fn update_acoustid_last_check(&self, library_id: i32, current_time: DateTime<Utc>) -> Box<Future<Item = u64, Error = io::Error> + Send> {
    let db = self.pool.clone();

    let future = self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        UPDATE acoustid_last_checks
        SET last_check = $2
        WHERE library_id = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing update_acoustid_last_check statement: {:#?}", err))),
      };

      let res = statement.execute(&[
        &library_id,
        &current_time
      ]);

      match res {
        Ok(v) => Ok(v),
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("unexpected error with last_check update: {:#?}", err))),
      }
    });

    Box::new(future)
  }

  pub fn delete_acoustid_last_check(&self, library_id: i32) -> impl Future<Item = u64, Error = io::Error> + Send {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        DELETE FROM acoustid_last_checks
        WHERE library_id = $1
      "#) {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing update_acoustid_last_check statement: {:#?}", err))),
      };

      let res = statement.execute(&[
        &library_id,
      ]);

      match res {
        Ok(v) => Ok(v),
        Err(err) => Err(io::Error::new(io::ErrorKind::Other, format!("unexpected error with last_check delete: {:?}", err))),
      }
    })
  }

  pub fn path_iter<F: 'static>(&self, cb: F) -> impl Future<Item = (), Error = io::Error> + Send
    where F: Send + Fn(i32, String) -> ()
  {
    let db = self.pool.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let stmt = match conn.prepare_cached("SELECT id, path FROM library") {
        Ok(v) => v,
        Err(err) => return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing path_iter statement: {:#?}", err))),
      };

      let trans = conn.transaction().unwrap();
      let mut rows = stmt.lazy_query(&trans, &[], 100).unwrap();

      while let Some(row) = rows.next().unwrap() {
        let id: i32 = row.get(0);
        let path: String = row.get(1);

        cb(id, path);
      }

      Ok(())
    })
  }
}
