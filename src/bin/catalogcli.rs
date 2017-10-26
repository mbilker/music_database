extern crate clap;
extern crate pretty_env_logger;
extern crate rayon;

#[macro_use]
extern crate log;

extern crate music_card_catalog;

use clap::{App, Arg, SubCommand};
use rayon::prelude::*;

use music_card_catalog::database;
use music_card_catalog::file_scanner;
use music_card_catalog::fingerprint;
use music_card_catalog::config::Config;
use music_card_catalog::models::MediaFileInfo;

// Main entrypoint for the program
fn main() {
  pretty_env_logger::init().unwrap();

  let matches = App::new("Music Card Catalog")
    .version("0.1.0")
    .author("Matt Bilker <me@mbilker.us>")
    .about("Gather data about my music library")
    .subcommand(SubCommand::with_name("scan")
      .about("scan music library directories")
      .author("Matt Bilker <me@mbilker.us>"))
    .subcommand(SubCommand::with_name("info")
      .about("show info about a single file")
      .author("Matt Bilker <me@mbilker.us>")
      .arg(Arg::with_name("path")
        .help("the file path")
        .index(1)
        .required(true)))
    .subcommand(SubCommand::with_name("fingerprint")
      .about("display a file's chromaprint fingerprint")
      .author("Matt Bilker <me@mbilker.us>")
      .arg(Arg::with_name("path")
        .help("the file path")
        .index(1)
        .required(true)))
    .get_matches();

  let config: Config = match Config::read_configuration() {
    Ok(res) => res,
    Err(err) => panic!("Error reading configuration: {:?}", err),
  };
  info!("Config: {:?}", config);

  if let Some(_matches) = matches.subcommand_matches("scan") {
    let conn = database::establish_connection().unwrap();
    info!("Database Connection: {:?}", conn);

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

      database::insert_files(&conn, &files);
    }
  } else if let Some(matches) = matches.subcommand_matches("info") {
    let file_path = matches.value_of("path").unwrap();
    let info = MediaFileInfo::read_file(file_path);

    debug!("{:?}", info);

    if let Some(info) = info {
      println!("Info for {}", file_path);
      println!("Title: {}", info.title.unwrap_or_else(|| String::new()));
      println!("Artist: {}", info.artist.unwrap_or_else(|| String::new()));
      println!("Album: {}", info.album.unwrap_or_else(|| String::new()));
      println!("Track: {}", info.track.unwrap_or_else(|| String::new()));
      println!("Track Number: {}", info.track_number);
      println!("Duration: {} ms", info.duration);
    } else {
      println!("No info could be gathered from the file");
    }
  } else if let Some(matches) = matches.subcommand_matches("fingerprint") {
    let file_path = matches.value_of("path").unwrap();

    let (duration, fingerprint) = match fingerprint::get(file_path) {
      Ok(res) => res,
      Err(err) => {
        error!("error getting file's fingerprint: {:?}", err);
        panic!("error getting file's fingerprint");
      },
    };

    println!("duration: {}", duration);
    println!("fingerprint: {:?}", fingerprint);
  }
}
