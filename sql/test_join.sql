SELECT
  "recording".id,
  "recording".gid as "recording_gid",
  "recording".name as "recording_name",
  "recording".length,
  "track".name as "track_name",
  "release".gid as "release_gid",
  "release".name as "release_name",
  "artist_credit".name as "artist_credit_name"
FROM "musicbrainz"."recording"
INNER JOIN "musicbrainz"."track" ON "track".recording = "recording".id
INNER JOIN "musicbrainz"."medium" ON "medium".id = "track".medium
INNER JOIN "musicbrainz"."release" ON "release".id = "medium".release
INNER JOIN "musicbrainz"."artist_credit" ON "artist_credit".id = "release".artist_credit
WHERE
    "recording".gid = '18168ee7-2853-41a0-87c3-ca19631d36f7'
LIMIT 5;
