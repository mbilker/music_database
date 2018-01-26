CREATE TABLE acoustid_last_check (
  id          SERIAL PRIMARY KEY,
  library_id  INTEGER REFERENCES library (id),
  last_check  TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
