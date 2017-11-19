use chrono::{DateTime, Utc};
use postgres::{Connection, TlsMode};
use postgres::error::UNIQUE_VIOLATION;
use uuid::Uuid;

use std::env;

use models::MediaFileInfo;

#[derive(Debug)]
pub struct DatabaseConnection {
  connection: Connection,
}

impl DatabaseConnection {
  pub fn new() -> Option<Self> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let conn = Connection::connect(&*database_url, TlsMode::None);

    match conn {
      Ok(conn) => Some(Self {
        connection: conn,
      }),
      Err(err) => panic!("error connecting to PostgreSQL: {:#?}", err),
    }
  }

  pub fn fetch_file(&self, path: &String) -> Option<MediaFileInfo> {
    let statement = match self.connection.prepare_cached(r#"
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
      Err(err) => panic!("error preparing fetch_file statement: {:#?}", err),
    };

    let res = statement.query(&[
      &path
    ]);

    match res {
      Ok(rows) => {
        if rows.len() == 0 {
          return None;
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

        if !str::eq(path, &db_path) {
          warn!("Path from database is not the same as argument, path: {}", path);
        }

        let path = path.clone();
        let info = MediaFileInfo::from_db(id, path, title, artist, album, track, track_number, duration, mbid);
        Some(info)
      },
      Err(err) => {
        panic!("error retrieving row from database: {:#?}", err);
      }
    }
  }

  pub fn get_id(&self, info: &MediaFileInfo) -> i32 {
    let statement = match self.connection.prepare_cached(r#"
      SELECT
        id
      FROM library
      WHERE path = $1
    "#) {
      Ok(v) => v,
      Err(err) => panic!("error preparing get_id statement: {:#?}", err),
    };

    let rows = match statement.query(&[
      &info.path
    ]) {
      Ok(v) => v,
      Err(err) => panic!("error retrieving id from database: {:#?}", err),
    };

    let row = rows.get(0);

    let id = row.get(0);
    id
  }

  pub fn insert_file(&self, info: &MediaFileInfo) {
    let statement = match self.connection.prepare_cached(r#"
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

          panic!("unexpected error with SQL insert");
        }
      } else {
        info!("{}", info.path);
        info!("- {:#?}", info);
        error!("SQL insert error: {:#?}", err);

        panic!("unexpected error with SQL insert");
      }
    }
  }

  pub fn get_acoustid_last_check(&self, id: i32) -> Option<DateTime<Utc>> {
    let statement = match self.connection.prepare_cached(r#"
      SELECT
        last_check
      FROM acoustid_last_check
      WHERE library_id = $1
    "#) {
      Ok(v) => v,
      Err(err) => panic!("error preparing get_acoustid_last_check statement: {:#?}", err),
    };

    let rows = match statement.query(&[
      &id
    ]) {
      Ok(v) => v,
      Err(err) => panic!("error retrieving last_check from database: {:#?}", err),
    };

    // If the value does not exist in the database, return 0
    if rows.len() == 0 {
      return None;
    }

    let row = rows.get(0);

    let last_check = row.get(0);
    Some(last_check)
  }

  pub fn check_valid_recording_uuid(&self, uuid: &Uuid) -> bool {
    let statement = match self.connection.prepare_cached(r#"
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

    match res {
      Ok(rows) => {
        let row = rows.get(0);
        let count: i32 = row.get(0);
        debug!("uuid check count: {}", count);
        count > 0
      },
      Err(err) => {
        panic!("error checking valid recording uuid: {:?}", err);
      },
    }
  }

  pub fn update_file_uuid(&self, path: &str, uuid: &Uuid) {
    let statement = match self.connection.prepare_cached(r#"
      UPDATE library
      SET mbid = $2
      WHERE path = $1
    "#) {
      Ok(v) => v,
      Err(err) => panic!("error preparing update_file_uuid statement: {:#?}", err),
    };

    let res = statement.execute(&[
      &path,
      &uuid
    ]);

    if let Err(err) = res {
      panic!("unexpected error with file_uuid update: {:#?}", err);
    }
  }

  pub fn add_acoustid_last_check(&self, library_id: i32, current_time: DateTime<Utc>) {
    let statement = match self.connection.prepare_cached(r#"
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
    
    if let Err(err) = res {
      panic!("unexpected error with last_check insert: {:#?}", err);
    }
  }

  pub fn update_acoustid_last_check(&self, library_id: i32, current_time: DateTime<Utc>) {
    let statement = match self.connection.prepare_cached(r#"
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

    if let Err(err) = res {
      panic!("unexpected error with last_check update: {:#?}", err);
    }
  }
}
