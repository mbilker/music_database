extern crate clap;
extern crate dotenv;
extern crate ffmpeg;
extern crate hyper;
extern crate hyper_tls;
extern crate pretty_env_logger;
extern crate ratelimit;
extern crate tokio_core;

#[macro_use]
extern crate log;

extern crate music_card_catalog;

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;

use clap::{App, Arg, SubCommand};
use dotenv::dotenv;
use hyper::Client;
use hyper_tls::HttpsConnector;
use tokio_core::reactor::Core;

use music_card_catalog::acoustid::AcoustId;
use music_card_catalog::elasticsearch::ElasticSearch;
use music_card_catalog::fingerprint;
use music_card_catalog::config::Config;
use music_card_catalog::models::MediaFileInfo;
use music_card_catalog::processor::Processor;

fn print_file_info(path: &str) {
  let info = MediaFileInfo::read_file(path);

  debug!("{:#?}", info);

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

fn print_fingerprint(api_key: &String, lookup: bool, path: &str) {
  let (duration, fingerprint) = fingerprint::get(path).expect("Error getting file's fingerprint");

  println!("{}", fingerprint);

  if lookup {
    let mut core = Core::new().unwrap();

    let mut limiter = ratelimit::Builder::new().frequency(1).build();
    let limiter_handle = Rc::new(RefCell::new(limiter.make_handle()));

    thread::spawn(move || limiter.run());

    let client = Client::configure()
      .connector(HttpsConnector::new(1, &core.handle()).unwrap())
      .build(&core.handle());
    let client = Rc::new(client);

    let future = AcoustId::lookup(api_key.clone(), duration, fingerprint, client, limiter_handle);

    match core.run(future) {
      Ok(res) => {
        println!("Result: {:#?}", res);
      },
      Err(err) => {
        panic!("error looking up AcoustID for path: {}, {:#?}", path, err);
      },
    };
  }
}

// Main entrypoint for the program
fn main() {
  // Initialize libraries
  pretty_env_logger::init().unwrap();
  dotenv().ok();
  ffmpeg::init().unwrap();

  let matches = App::new("Music Card Catalog")
    .version("0.1.0")
    .author("Matt Bilker <me@mbilker.us>")
    .about("Gather data about my music library")
    .subcommand(SubCommand::with_name("scan")
      .about("scan music library directories")
      .author("Matt Bilker <me@mbilker.us>"))
    .subcommand(SubCommand::with_name("prune")
      .about("prune database of non-existant files")
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
      .arg(Arg::with_name("lookup")
        .help("lookup fingerprint on AcoustId")
        .short("l"))
      .arg(Arg::with_name("path")
        .help("the file path")
        .index(1)
        .required(true)))
    .subcommand(SubCommand::with_name("dump")
      .about("dump mappings")
      .author("Matt Bilker <me@mbilker.us>"))
    .get_matches();

  let config: Config = match Config::read_configuration() {
    Ok(res) => res,
    Err(err) => panic!("Error reading configuration: {:?}", err),
  };
  println!("Config: {:?}", config);

  if let Some(_matches) = matches.subcommand_matches("scan") {
    let mut processor = Processor::new(&config);

    let res = processor.scan_dirs();
    if let Err(err) = res {
      panic!("error scannning directories: {:#?}", err);
    }
  } else if let Some(_matches) = matches.subcommand_matches("prune") {
    let mut processor = Processor::new(&config);

    let res = processor.prune_db();
    if let Err(err) = res {
      panic!("error pruning database: {:#?}", err);
    }
  } else if let Some(matches) = matches.subcommand_matches("info") {
    let file_path = matches.value_of("path").unwrap();

    print_file_info(file_path);
  } else if let Some(matches) = matches.subcommand_matches("fingerprint") {
    let file_path = matches.value_of("path").unwrap();

    let lookup = matches.is_present("lookup");
    let api_key = config.api_keys.get("acoustid").expect("No AcoustID API key defined in config.yaml");

    print_fingerprint(api_key, lookup, file_path);
  } else if let Some(_matches) = matches.subcommand_matches("dump") {
    println!("Elasticsearch mapping: {:#?}", ElasticSearch::body());
  }
}
