use ffmpeg;
use reqwest;
use serde_json;

use uuid::Uuid;

use std::io;

#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdArtist {
  pub id: String,
  pub name: String,
}
    
#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdRecording {
  pub duration: Option<i32>,
  pub title: Option<String>,
  pub id: Uuid,
  pub artists: Option<Vec<AcoustIdArtist>>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AcoustIdResult {
  pub recordings: Option<Vec<AcoustIdRecording>>,
  pub score: f32,
  pub id: String,
}

#[derive(Debug, Deserialize)]
pub struct AcoustIdResponse {
  pub status: String,
  pub results: Vec<AcoustIdResult>,
}

#[derive(Debug)]
pub enum ProcessorError {
  NothingUseful(),

  ApiKeyError(),
  NoFingerprintMatch(),
  NoAudioStream(),

  RequestError(reqwest::Error),
  JsonError(serde_json::Error),
  IoError(io::Error),
  ThreadError(String),
  MutexError(String),

  FFmpegError(ffmpeg::Error),
  ChromaprintError(String),
}

impl From<reqwest::Error> for ProcessorError {
  fn from(value: reqwest::Error) -> ProcessorError {
    ProcessorError::RequestError(value)
  }
}

impl From<serde_json::Error> for ProcessorError {
  fn from(value: serde_json::Error) -> ProcessorError {
    ProcessorError::JsonError(value)
  }
}

impl From<io::Error> for ProcessorError {
  fn from(value: io::Error) -> ProcessorError {
    ProcessorError::IoError(value)
  }
}

impl From<ffmpeg::Error> for ProcessorError {
  fn from(value: ffmpeg::Error) -> ProcessorError {
    ProcessorError::FFmpegError(value)
  }
}
