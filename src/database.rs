use postgres::{Connection, TlsMode};
use postgres::error::UNIQUE_VIOLATION;
use std::env;

use models::MediaFileInfo;

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

  pub fn insert_file(&self, info: MediaFileInfo) {
/*
    let query = match self.connection.prepare(INSERT_QUERY) {
      Ok(res) => res,
      Err(err) => {
        error!("{:#?}", err);
        panic!("unable to prepare query");
      },
    };
*/

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
}
