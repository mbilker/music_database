use std::env;
use std::io;

use chrono::{DateTime, Utc};
use futures::Future;
use futures_cpupool::CpuPool;
use postgres::error::UNIQUE_VIOLATION;
use r2d2::{Config, Pool};
use r2d2_postgres::{TlsMode, PostgresConnectionManager};
use uuid::Uuid;

use models::MediaFileInfo;

#[derive(Clone, Debug)]
pub struct DatabaseConnection {
  pool: Pool<PostgresConnectionManager>,
  thread_pool: CpuPool,
}

impl DatabaseConnection {
  pub fn new(thread_pool: CpuPool) -> Self {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let config = Config::default();
    let manager = PostgresConnectionManager::new(&*database_url, TlsMode::None).unwrap();
    let pool = Pool::new(config, manager).unwrap();

    Self {
      pool,
      thread_pool,
    }
  }

  pub fn fetch_file(&self, path: String) -> impl Future<Item = Option<MediaFileInfo>, Error = io::Error> + Send {
    let db = self.pool.clone();
    let path = path.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        SELECT
          id,
          title,
          artist,
          album,
          track,
          track_number,
          duration,
          path,
          mbid
        FROM library
        WHERE path = $1
      "#) {
        Ok(v) => v,
        Err(err) => {
          return Err(io::Error::new(io::ErrorKind::Other, format!("error preparing fetch_file statement: {:#?}", err)));
        },
      };

      let res = statement.query(&[
        &path
      ]);

      let rows = match res {
        Ok(v) => v,
        Err(err) => {
          return Err(io::Error::new(io::ErrorKind::Other, format!("error retrieving row from database: {:#?}", err)));
        },
      };

      if rows.len() == 0 {
        return Ok(None);
      }

      let row = rows.get(0);

      let id = row.get(0);
      let title = row.get(1);
      let artist = row.get(2);
      let album = row.get(3);
      let track = row.get(4);
      let track_number = row.get(5);
      let duration = row.get(6);
      let db_path: String = row.get(7);
      let mbid = row.get(8);

      if !str::eq(&path, &db_path) {
        warn!("Path from database is not the same as argument, path: {}", path);
      }

      let info = MediaFileInfo::from_db(id, path, title, artist, album, track, track_number, duration, mbid);
      Ok(Some(info))
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

  pub fn insert_file(&self, info: &MediaFileInfo) -> impl Future<Item = (), Error = io::Error> + Send {
    let db = self.pool.clone();
    let info = info.clone();

    self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        INSERT INTO library (
          title,
          artist,
          album,
          track,
          track_number,
          duration,
          path
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
      "#) {
        Ok(v) => v,
        Err(err) => panic!("error preparing insert_file statement: {:#?}", err),
      };

      let res = statement.execute(&[
        &info.title,
        &info.artist,
        &info.album,
        &info.track,
        &info.track_number,
        &info.duration,
        &info.path
      ]);

      if let Err(err) = res {
        if let Some(code) = err.code() {
          if code != &UNIQUE_VIOLATION {
            info!("{}", info.path);
            info!("- {:#?}", info);
            error!("SQL insert error: {:#?}", err);

            return Err(io::Error::new(io::ErrorKind::Other, "unexpected error with SQL insert".to_owned()));
          }
        } else {
          info!("{}", info.path);
          info!("- {:#?}", info);
          error!("SQL insert error: {:#?}", err);

          return Err(io::Error::new(io::ErrorKind::Other, "unexpected error with SQL insert".to_owned()));
        }
      }

      Ok(())
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
        FROM acoustid_last_check
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
      if rows.len() == 0 {
        return Ok(None);
      }

      let row = rows.get(0);
      let last_check = row.get(0);

      Ok(Some(last_check))
    })
  }

  pub fn check_valid_recording_uuid(&self, uuid: &Uuid) -> impl Future<Item = bool, Error = io::Error> + Send {
    let db = self.pool.clone();
    let uuid = uuid.clone();

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
        Err(err) => panic!("error preparing check_valid_recording_uuid statement: {:#?}", err),
      };

      let res = statement.query(&[
        &uuid
      ]);

      let rows = match res {
        Ok(v) => v,
        Err(err) => {
          panic!("error checking valid recording uuid: {:?}", err);
        },
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
    let current_time = current_time.clone();

    let future = self.thread_pool.spawn_fn(move || {
      let conn = db.get().map_err(|e| {
        io::Error::new(io::ErrorKind::Other, format!("timeout: {}", e))
      })?;

      let statement = match conn.prepare_cached(r#"
        INSERT INTO acoustid_last_check (
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
        UPDATE acoustid_last_check
        SET last_check = $2
        WHERE library_id = $1
      "#) {
        Ok(v) => v,
        Err(err) => panic!("error preparing update_acoustid_last_check statement: {:#?}", err),
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
}
