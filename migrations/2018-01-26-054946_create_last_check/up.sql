CREATE TABLE acoustid_last_checks (
  id          SERIAL PRIMARY KEY,
  library_id  INTEGER REFERENCES library (id) NOT NULL,
  last_check  TIMESTAMP WITH TIME ZONE DEFAULT NOW() NOT NULL
);
