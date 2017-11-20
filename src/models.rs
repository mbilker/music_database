use mediainfo::MediaInfo;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct MediaFileInfo {
  // This is the id from the database
  pub id: i32,

  pub path: String,

  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub track: Option<String>,
  pub track_number: u32,
  pub duration: u32,

  pub mbid: Option<Uuid>,
}

#[derive(Debug, ElasticType, Serialize)]
pub struct MediaFileInfoDocument {
  pub id: i32,

  pub path: String,

  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub track: Option<String>,
  pub track_number: i32,
  pub duration: i32,

  pub mbid: Option<String>,
}

impl MediaFileInfo {
  pub fn from_db(
    id: i32,
    path: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    track: Option<String>,
    track_number: u32,
    duration: u32,
    mbid: Option<Uuid>
  ) -> Self {
    Self {
      id: id,
      path: path,
      title: title,
      artist: artist,
      album: album,
      track: track,
      track_number: track_number,
      duration: duration,
      mbid: mbid
    }
  }

  pub fn read_file(path: &str) -> Option<Self> {
    let mut media_info: MediaInfo = MediaInfo::new();

    // Fail quickly if the file could not be opened
    if let Err(_) = media_info.open(path) {
      error!(target: path, "could not open file: {}", path);
      return None;
    }

    // Filter out any file without an audio stream
    //
    // `get_with_default_option` throws a ZeroLengthError if there is no value
    // for the parameter
    let audio_streams = media_info.get_with_default_options("AudioCount");
    if let Err(_) = audio_streams {
      trace!(target: path, "no audio streams");
      return None;
    }

    // Filter out any file with no duration
    let duration = media_info.get_duration_ms().unwrap_or(0);
    if duration == 0 {
      trace!(target: path, "duration == 0");
      return None;
    }

    // Filter out m3u8 files, they have a duration according to mediainfo, but
    // I do not want m3u8 files in the database
    let extension = media_info.get_with_default_options("Format/Extensions");
    if let Ok(extension) = extension {
      if extension == "m3u8" {
        trace!(target: path, "m3u8 playlist");
        return None;
      }
    }

    // Store the most relevant details in a struct for easy access
    let file_info = MediaFileInfo {
      id:           -1,

      path:         path.to_owned(),

      title:        media_info.get_title().ok(),
      artist:       media_info.get_performer().ok(),
      album:        media_info.get_album().ok(),
      track:        media_info.get_track_name().ok(),
      track_number: media_info.get_track_number().unwrap_or(0),
      duration:     duration,

      mbid:         None,
    };

    media_info.close();

    if file_info.is_default_values() {
      return None;
    }

    Some(file_info)
  }

  #[inline]
  fn is_default_values(&self) -> bool {
    self.title  == None &&
    self.artist == None &&
    self.album  == None &&
    self.track  == None &&
    self.track_number == 0 &&
    self.duration == 0
  }

  pub fn to_document(&self) -> MediaFileInfoDocument {
    MediaFileInfoDocument {
      id:		self.id.clone(),
      path:		self.path.clone(),
      title:		self.title.clone(),
      artist:		self.artist.clone(),
      album:		self.album.clone(),
      track:		self.track.clone(),
      track_number:	self.track_number as i32,
      duration:		self.duration as i32,
      mbid:		self.mbid.map(|x| x.to_string())
    }
  }
}
