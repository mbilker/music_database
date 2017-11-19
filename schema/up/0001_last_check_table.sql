CREATE TABLE acoustid_last_check (
  library_id	INTEGER REFERENCES library (id),
  last_check	TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
