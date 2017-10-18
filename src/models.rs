use mediainfo::MediaInfo;
use postgres::Connection;

#[derive(Debug)]
pub struct MediaFileInfo {
  pub path: String,

  pub title: Option<String>,
  pub artist: Option<String>,
  pub album: Option<String>,
  pub track: Option<String>,
  pub track_number: u32,
  pub duration: u32,
}

impl MediaFileInfo {
  pub fn read_file(path: &String) -> Option<Self> {
    let mut media_info: MediaInfo = MediaInfo::new();

    // Fail quickly if the file could not be opened
    if let Err(_) = media_info.open(path) {
      println!("could not open file: {}", path);
      return None;
    }

    // Filter out any file without an audio stream
    //
    // `get_with_default_option` throws a ZeroLengthError if there is no value
    // for the parameter
    let audio_streams = media_info.get_with_default_options("AudioCount");
    if let Err(_) = audio_streams {
      return None;
    }

    // Filter out any file with no duration
    let duration = media_info.get_duration_ms().unwrap_or(0);
    if duration == 0 {
      return None;
    }

    // Filter out m3u8 files, they have a duration according to mediainfo, but
    // I do not want m3u8 files in the database
    let extension = media_info.get_with_default_options("Format/Extensions");
    if let Ok(extension) = extension {
      if extension == "m3u8" {
        return None;
      }
    }

    // Store the most relevant details in a struct for easy access
    let file_info = MediaFileInfo {
      path:         path.clone(),

      title:        media_info.get_title().ok(),
      artist:       media_info.get_performer().ok(),
      album:        media_info.get_album().ok(),
      track:        media_info.get_track_name().ok(),
      track_number: media_info.get_track_number().unwrap_or(0),
      duration:     duration,
    };

    media_info.close();

    Some(file_info)
  }

  pub fn db_insert(&self, conn: &Connection) -> bool {
    static INSERT_QUERY: &'static str = r#"
      INSERT INTO library (
        title,
        artist,
        album,
        track,
        track_number,
        duration,
        path
      ) VALUES ($1, $2, $3, $4, $5, $6, $7)
    "#;

    let res = conn.execute(INSERT_QUERY, &[
      &self.title,
      &self.artist,
      &self.album,
      &self.track,
      &self.track_number,
      &self.duration,
      &self.path
    ]);

    if let Err(err) = res {
      println!("SQL insert error: {:?}", err);
      false
    } else {
      res.is_ok()
    }
  }

  #[inline]
  pub fn is_default_values(&self) -> bool {
    self.title  == None &&
    self.artist == None &&
    self.album  == None &&
    self.track  == None &&
    self.duration == 0 &&
    self.track_number == 0
  }
}
