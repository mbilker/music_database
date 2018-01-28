DELETE FROM
  acoustid_last_checks
WHERE
  library_id IN (
    SELECT
      id
    FROM
      library
    WHERE
      mbid IS NOT NULL
  );
