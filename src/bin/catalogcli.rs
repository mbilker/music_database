extern crate clap;
extern crate dotenv;
extern crate pretty_env_logger;

#[macro_use]
extern crate log;

extern crate music_card_catalog;

use clap::{App, Arg, SubCommand};
use dotenv::dotenv;

use music_card_catalog::acoustid;
use music_card_catalog::fingerprint;
use music_card_catalog::config::Config;
use music_card_catalog::models::MediaFileInfo;
use music_card_catalog::processor::Processor;

fn print_file_info(path: &str) {
  let info = MediaFileInfo::read_file(path);

  debug!("{:?}", info);

  if let Some(info) = info {
    println!("Info for {}", path);
    println!("Title: {}", info.title.unwrap_or_else(|| String::new()));
    println!("Artist: {}", info.artist.unwrap_or_else(|| String::new()));
    println!("Album: {}", info.album.unwrap_or_else(|| String::new()));
    println!("Track: {}", info.track.unwrap_or_else(|| String::new()));
    println!("Track Number: {}", info.track_number);
    println!("Duration: {} ms", info.duration);
  } else {
    println!("No info could be gathered from the file");
  }
}

fn print_fingerprint(api_key: &str, path: &str) {
  let (duration, fingerprint) = fingerprint::get(path).expect("Error getting file's fingerprint");

  println!("duration: {}", duration);
  println!("fingerprint: {:?}", fingerprint);

  acoustid::lookup(api_key, duration, &fingerprint);
}

// Main entrypoint for the program
fn main() {
  pretty_env_logger::init().unwrap();
  dotenv().ok();

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
    let processor = Processor::new(config);
    processor.scan_dirs();
  } else if let Some(matches) = matches.subcommand_matches("info") {
    let file_path = matches.value_of("path").unwrap();

    print_file_info(file_path);
  } else if let Some(matches) = matches.subcommand_matches("fingerprint") {
    let api_key = config.api_keys.get("acoustid").expect("No AcoustID API key defined in config.yaml");
    let file_path = matches.value_of("path").unwrap();

    print_fingerprint(api_key, file_path);
  }
}
