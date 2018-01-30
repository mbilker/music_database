use std::io;

use std::error::Error;

use ffmpeg;
use hyper;
use serde_json;

use uuid::Uuid;

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

quick_error! {
  #[derive(Debug)]
  pub enum ProcessorError {
    NothingUseful {}

    ApiKey {}
    NoFingerprintMatch {}
    NoAudioStream {}

    HyperError(err: hyper::Error) {
      from()
      cause(err)
      display(me) -> ("{}: {}", me.description(), err)
    }
    JsonError(err: serde_json::Error) {
      from()
      cause(err)
      display(me) -> ("{}: {}", me.description(), err)
    }
    Io(err: io::Error) {
      from()
      cause(err)
      display(me) -> ("{}: {}", me.description(), err)
    }
    FFmpeg(err: ffmpeg::Error) {
      from()
      cause(err)
      display(me) -> ("{} {}", me.description(), err)
    }
    Chromaprint(s: &'static str) {}

    Thread(s: &'static str) {}
    Mutex(s: &'static str) {}
  }
}
