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
  pub fn read_file(path: &str) -> Option<Self> {
    let mut media_info: MediaInfo = MediaInfo::new();

    // Fail quickly if the file could not be opened
    if media_info.open(path).is_err() {
      error!("could not open file: {}", path);
      return None;
    }

    // Filter out any file without an audio stream
    //
    // `get_with_default_option` throws a ZeroLengthError if there is no value
    // for the parameter
    let audio_streams = media_info.get_with_default_options("AudioCount");
    if audio_streams.is_err() {
      trace!("no audio streams");
      return None;
    }

    // Filter out any file with no duration
    let duration = media_info.get_duration_ms().unwrap_or(0);
    if duration == 0 {
      trace!("duration == 0");
      return None;
    }

    // Filter out m3u8 and mpls files, they have a duration according to
    // mediainfo, but I do not want playlist files in the database. 
    // I do not want my backup files to be included (orig and bak files).
    // APE files break decoding and I do not know why and I do not use them.
    let format_extension = media_info.get_with_default_options("Format/Extensions");
    let extension = media_info.get_with_default_options("FileExtension");
    let ignore = match format_extension {
      Ok(ref format_extension) => match format_extension.as_ref() {
        "ape mac" |
        "m3u8" |
        "mpls" |
        "orig" |
        "bak" => true,

        _ => false,
      },
      Err(_) => false,
    } || match extension {
      Ok(ref extension) => match extension.as_ref() {
        "orig" |
        "bak" => true,

        _ => false,
      },
      Err(_) => false,
    };
    if ignore {
      trace!("ignoring {:?} or {:?} extension", format_extension, extension);
      return None;
    }

    // Set the title to the file name (minus extension) if there is no
    // title set in the file's metadata
    let title = match media_info.get_title() {
      Ok(v) => Some(v),
      Err(_) => media_info.get_with_default_options("FileName").ok()
    };

    // Store the most relevant details in a struct for easy access
    let file_info = MediaFileInfo {
      id:           -1,

      path:         path.to_owned(),

      title:        title,
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
      id:           self.id,
      path:         self.path.clone(),
      title:        self.title.clone(),
      artist:       self.artist.clone(),
      album:        self.album.clone(),
      track:        self.track.clone(),
      track_number: self.track_number as i32,
      duration:     self.duration as i32,
      mbid:         self.mbid.map(|x| x.to_string())
    }
  }
}
