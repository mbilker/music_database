SELECT DISTINCT
  "library".mbid,
  "library".track,
  "recording".name as "recording_name"
FROM library
INNER JOIN "musicbrainz"."recording" ON "recording".gid = "library".mbid
WHERE
    "library".mbid IS NOT NULL
AND LOWER("library".track) != LOWER("recording".name)
LIMIT 100;
