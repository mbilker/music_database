CREATE TABLE library (
  id		SERIAL PRIMARY KEY,
  title		VARCHAR,
  artist	VARCHAR,
  album		VARCHAR,
  track		VARCHAR,
  track_number	INT NOT NULL,
  duration	INT NOT NULL
)
