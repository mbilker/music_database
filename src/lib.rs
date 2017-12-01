#![feature(conservative_impl_trait)]

extern crate chromaprint;
extern crate chrono;
extern crate crossbeam;
extern crate dotenv;
extern crate elastic;
extern crate ffmpeg;
extern crate futures;
extern crate futures_cpupool;
extern crate hyper;
extern crate hyper_tls;
extern crate mediainfo;
extern crate num_cpus;
extern crate postgres;
extern crate r2d2;
extern crate r2d2_postgres;
extern crate ratelimit;
extern crate serde;
extern crate serde_yaml;
extern crate tokio_core;
extern crate tokio_service;
extern crate uuid;
extern crate walkdir;

#[macro_use] extern crate elastic_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate serde_derive;
#[macro_use] extern crate serde_json;

pub mod acoustid;
pub mod basic_types;
pub mod config;
pub mod database;
pub mod elasticsearch;
pub mod file_scanner;
pub mod fingerprint;
pub mod models;
pub mod processor;
