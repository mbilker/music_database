CREATE TABLE library (
  id		SERIAL PRIMARY KEY,
  title		VARCHAR,
  artist	VARCHAR,
  album		VARCHAR,
  track		VARCHAR,
  track_number	OID NOT NULL,
  duration	OID NOT NULL,
  path		VARCHAR UNIQUE
)
