CREATE TABLE library (
  id            SERIAL PRIMARY KEY,
  path          VARCHAR UNIQUE NOT NULL,
  title         VARCHAR,
  artist        VARCHAR,
  album         VARCHAR,
  track         VARCHAR,
  track_number  OID NOT NULL,
  duration      OID NOT NULL,
  mbid          UUID
);

CREATE INDEX library_mbid ON library (mbid);
