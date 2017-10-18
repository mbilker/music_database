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
use rayon::prelude::*;

use config::Config;
use models::MediaFileInfo;

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
        .collect();

      for info in files {
        println!("{}", info.path);
        println!("- {:?}", info);

        if !info.is_default_values() {
          info.db_insert(&conn);
        }
      }
    }
  }
}
