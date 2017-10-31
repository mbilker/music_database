use postgres::{Connection, TlsMode};
use postgres::error::UNIQUE_VIOLATION;
use uuid::Uuid;

use std::env;

use models::MediaFileInfo;

static FETCH_BY_PATH_QUERY: &'static str = r#"
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
"#;

static FETCH_ID_QUERY: &'static str = r#"
  SELECT
    id
  FROM library
  WHERE path = $1
"#;

static INSERT_QUERY: &'static str = r#"
  INSERT INTO library (
    title,
    artist,
    album,
    track,
    track_number,
    duration,
    path
  ) VALUES ($1, $2, $3, $4, $5, $6, $7)
"#;

static CHECK_VALID_UUID_QUERY: &'static str = r#"
  SELECT
    COUNT(*)
  FROM "musicbrainz"."recording"
  WHERE "recording"."gid" = $1
"#;

static UPDATE_UUID_QUERY: &'static str = r#"
  UPDATE library
  SET mbid = $2
  WHERE path = $1
"#;

static INSERT_LAST_CHECK_QUERY: &'static str = r#"
  INSERT INTO acoustid_last_check (
    library_id,
    last_check
  ) VALUES ($1, NOW())
"#;

static FETCH_LAST_CHECK_QUERY: &'static str = r#"
  SELECT
    CAST(EXTRACT(EPOCH FROM last_check) AS BIGINT)
  FROM acoustid_last_check
  WHERE library_id = $1
"#;

static UPDATE_LAST_CHECK_QUERY: &'static str = r#"
  UPDATE acoustid_last_check
  SET last_check = $2
  WHERE library_id = $1
"#;

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
    let res = self.connection.query(FETCH_BY_PATH_QUERY, &[
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
        panic!("error retrieving row from database: {:?}", err);
      }
    }
  }

  pub fn get_id(&self, info: &MediaFileInfo) -> i32 {
    let rows = match self.connection.query(FETCH_ID_QUERY, &[
      &info.path
    ]) {
      Ok(v) => v,
      Err(err) => panic!("error retrieving id from database: {:#?}", err),
    };

    let row = rows.get(0);

    let id = row.get(0);
    id
  }

  pub fn get_acoustid_last_check(&self, id: i32) -> Option<u64> {
    let rows = match self.connection.query(FETCH_LAST_CHECK_QUERY, &[
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

    let last_check: i64 = row.get(0);
    Some(last_check as u64)
  }

  pub fn insert_file(&self, info: &MediaFileInfo) {
    let res = self.connection.execute(INSERT_QUERY, &[
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

  pub fn check_valid_recording_uuid(&self, uuid: &Uuid) -> bool {
    let res = self.connection.query(CHECK_VALID_UUID_QUERY, &[
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
    let res = self.connection.execute(UPDATE_UUID_QUERY, &[
      &path,
      &uuid
    ]);

    if let Err(err) = res {
      panic!("unexpected error with file_uuid update: {:#?}", err);
    }
  }

  pub fn add_acoustid_last_check(&self, library_id: i32) {
    let res = self.connection.execute(INSERT_LAST_CHECK_QUERY, &[
      &library_id
    ]);
    
    if let Err(err) = res {
      panic!("unexpected error with last_check insert: {:#?}", err);
    }
  }

  pub fn update_acoustid_last_check(&self, library_id: i32) {
    let res = self.connection.execute(UPDATE_LAST_CHECK_QUERY, &[
      &library_id
    ]);

    if let Err(err) = res {
      panic!("unexpected error with last_check update: {:#?}", err);
    }
  }
}
