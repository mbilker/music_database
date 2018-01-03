use ffmpeg;
use hyper;
use serde_json::Error as SerdeJsonError;

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
  pub results: Option<Vec<AcoustIdResult>>,
}

#[derive(Debug)]
pub enum ProcessorError {
  NothingUseful,

  ApiKeyError,
  NoFingerprintMatch,
  NoAudioStream,

  HyperError(hyper::Error),
  JsonError(SerdeJsonError),
  IoError(io::Error),
  ThreadError(String),
  MutexError(String),

  FFmpegError(ffmpeg::Error),
  ChromaprintError(String),
}

impl From<hyper::Error> for ProcessorError {
  fn from(value: hyper::Error) -> ProcessorError {
    ProcessorError::HyperError(value)
  }
}

impl From<SerdeJsonError> for ProcessorError {
  fn from(value: SerdeJsonError) -> ProcessorError {
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
