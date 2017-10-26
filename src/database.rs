use dotenv::dotenv;
use postgres::{Connection, TlsMode};
use postgres::error::UNIQUE_VIOLATION;
use std::env;

use models::MediaFileInfo;

pub fn establish_connection() -> Option<Connection> {
  dotenv().ok();

  let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
  let conn = Connection::connect(&*database_url, TlsMode::None);
  
  conn.ok()
}

pub fn insert_files(conn: &Connection, entries: &[MediaFileInfo]) {
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

  let query = match conn.prepare(INSERT_QUERY) {
    Ok(res) => res,
    Err(err) => {
      error!("{:?}", err);
      panic!("unable to prepare query");
    },
  };

  for info in entries {
    let res = query.execute(&[
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
          info!("- {:?}", info);
          error!("SQL insert error: {:?}", err);

          panic!("unexpected error with SQL insert");
        }
      } else {
        info!("{}", info.path);
        info!("- {:?}", info);
        error!("SQL insert error: {:?}", err);

        panic!("unexpected error with SQL insert");
      }
    }
  }
}
