extern crate clap;
extern crate dotenv;
extern crate mediainfo;
extern crate postgres;
extern crate rayon;
extern crate serde;
extern crate serde_yaml;
extern crate walkdir;

#[macro_use]
extern crate serde_derive;

mod config;
mod database;
mod file_scanner;
mod models;

use clap::{App, SubCommand};
use postgres::Connection;
use postgres::error::UNIQUE_VIOLATION;
use rayon::prelude::*;

use config::Config;
use models::MediaFileInfo;

fn db_insert(conn: &Connection, entries: &[MediaFileInfo]) {
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
      println!("{:?}", err);
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
          println!("{}", info.path);
          println!("- {:?}", info);
          println!("SQL insert error: {:?}", err);

          panic!("unexpected error with SQL insert");
        }
      } else {
        println!("{}", info.path);
        println!("- {:?}", info);
        println!("SQL insert error: {:?}", err);

        panic!("unexpected error with SQL insert");
      }
    }
  }
}

// Main entrypoint for the program
fn main() {
  let matches = App::new("Music Card Catalog")
    .version("0.1.0")
    .author("Matt Bilker <me@mbilker.us>")
    .about("Gather data about my music library")
    .subcommand(SubCommand::with_name("scan")
      .about("scan music library directories")
      .author("Matt Bilker <me@mbilker.us>"))
    .get_matches();

  if let Some(matches) = matches.subcommand_matches("scan") {
    println!("matched: {:?}", matches);

    let config: Config = match Config::read_configuration() {
      Ok(res) => res,
      Err(err) => panic!("Error reading configuration: {:?}", err),
    };
    println!("Config: {:?}", config);

    let conn = database::establish_connection().unwrap();
    println!("Database Connection: {:?}", conn);

    for path in config.paths {
      println!("Scanning {}", path);

      // TODO(mbilker): either figure out how to make rayon iterate over
      // filtered results using something like `for_each` or make my own
      // work queue
      //
      // `for_each` works, but the SQL connection is not `Sync + Send`
      let dir_walk = file_scanner::scan_dir(&path);
      let files: Vec<MediaFileInfo> = dir_walk.par_iter()
        .filter_map(|e| MediaFileInfo::read_file(e))
        .filter(|e| !e.is_default_values())
        .collect();

      db_insert(&conn, &files);
    }
  }
}
